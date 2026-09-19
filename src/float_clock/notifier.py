"""临近时间点的系统通知调度（带去重与「重载不重发」保护）。"""

from __future__ import annotations

from datetime import datetime, timedelta

from . import notify
from .config import NotifyConfig
from .timefmt import format_hms, format_human

__all__ = ["MomentNotifier", "render_copy"]


def render_copy(template: str, values: dict[str, str]) -> str:
    """按占位符渲染通知文案；认不出的占位符原样保留。"""
    text = template
    for key, value in values.items():
        text = text.replace("{" + key + "}", value)
    return text

_GRACE_SECONDS = 1.5


class MomentNotifier:
    """为若干「时间点」排好提醒队列，tick 时把到点的发出去。"""

    def __init__(self, config: NotifyConfig) -> None:
        self.config = config
        self._moments: list[tuple[str, datetime]] = []
        self._events: list[tuple[datetime, str, str, str]] = []
        self._fired: set[str] = set()
        self._armed_at: datetime | None = None

    # ------------------------------------------------------------------ 构建
    def arm(self, moments: list[tuple[str, datetime]], now: datetime) -> None:
        """（重新）布防。时间点没变化时保留已发送记录，避免重复弹窗。"""
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
                values = base | {"sign": "-", "clock": format_hms(0), "human": "0 秒"}
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

    # -------------------------------------------------------------------- 运行
    def tick(self, now: datetime) -> list[str]:
        """发送到点且本次启动后到点的通知，返回本次发出的标题列表。"""
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

    # ------------------------------------------------------------------ 辅助
    def upcoming(self, now: datetime, limit: int = 6) -> list[tuple[datetime, str]]:
        """还没发的最近若干条提醒，供 --print 预览。"""
        result = [
            (fire_at, title)
            for fire_at, key, title, _ in self._events
            if key not in self._fired and fire_at >= now
        ]
        return result[:limit]
