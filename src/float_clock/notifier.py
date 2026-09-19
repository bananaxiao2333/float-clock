"""Scheduling of system notifications near each moment (with de-duplication and a "reload does not resend" guard)."""

from __future__ import annotations

from datetime import datetime, timedelta

from . import notify
from .config import NotifyConfig
from .timefmt import format_hms, format_human

__all__ = ["MomentNotifier", "render_copy"]


def render_copy(template: str, values: dict[str, str]) -> str:
    """Render notification copy from placeholders; unrecognised placeholders are left as-is."""
    text = template
    for key, value in values.items():
        text = text.replace("{" + key + "}", value)
    return text

_GRACE_SECONDS = 1.5


class MomentNotifier:
    """Queue reminders for a number of moments and fire the due ones on each tick."""

    def __init__(self, config: NotifyConfig) -> None:
        self.config = config
        self._moments: list[tuple[str, datetime]] = []
        self._events: list[tuple[datetime, str, str, str]] = []
        self._fired: set[str] = set()
        self._armed_at: datetime | None = None

    # ---------------------------------------------------------------- building
    def arm(self, moments: list[tuple[str, datetime]], now: datetime) -> None:
        """(Re-)arm. When the moments are unchanged the sent record is kept, so nothing pops up twice."""
        if moments == self._moments:
            self._armed_at = now
            return

        self._moments = list(moments)
        self._fired.clear()
        self._armed_at = now
        events: list[tuple[datetime, str, str, str]] = []

        for label, moment in moments:
            base = {
                "label": label,
                "time": moment.strftime("%H:%M:%S"),
                "datetime": moment.strftime("%Y-%m-%d %H:%M:%S"),
            }
            for lead in sorted({int(v) for v in self.config.before if int(v) > 0}):
                fire_at = moment - timedelta(seconds=lead)
                values = base | {
                    "sign": "-",
                    "clock": format_hms(lead),
                    "human": format_human(lead),
                }
                events.append(
                    (
                        fire_at,
                        f"{label}|-{lead}",
                        render_copy(self.config.title_template, values),
                        render_copy(self.config.body_before, values),
                    )
                )
            if self.config.at_moment:
                values = base | {"sign": "-", "clock": format_hms(0), "human": "0s"}
                events.append(
                    (
                        moment,
                        f"{label}|0",
                        render_copy(self.config.title_template, values),
                        render_copy(self.config.body_at, values),
                    )
                )
            for trail in sorted({int(v) for v in self.config.after if int(v) > 0}):
                fire_at = moment + timedelta(seconds=trail)
                values = base | {
                    "sign": "+",
                    "clock": format_hms(trail),
                    "human": format_human(trail),
                }
                events.append(
                    (
                        fire_at,
                        f"{label}|+{trail}",
                        render_copy(self.config.title_template, values),
                        render_copy(self.config.body_after, values),
                    )
                )

        events.sort(key=lambda item: item[0])
        self._events = events

    # ------------------------------------------------------------------ running
    def tick(self, now: datetime) -> list[str]:
        """Send notifications that are due and became due after this startup; returns the titles sent this tick."""
        if not self.config.enabled or self._armed_at is None:
            return []
        cutoff = self._armed_at - timedelta(seconds=_GRACE_SECONDS)
        sent: list[str] = []
        for fire_at, key, title, body in self._events:
            if key in self._fired:
                continue
            if now >= fire_at >= cutoff:
                self._fired.add(key)
                notify.send_async(title, body, self.config.sound_name if self.config.sound else None)
                sent.append(title)
        return sent

    # ------------------------------------------------------------------ helpers
    def upcoming(self, now: datetime, limit: int = 6) -> list[tuple[datetime, str]]:
        """The nearest reminders that have not been sent yet, for the --print preview."""
        result = [
            (fire_at, title)
            for fire_at, key, title, _ in self._events
            if key not in self._fired and fire_at >= now
        ]
        return result[:limit]
