"""Time parsing and T± formatting helpers.

T± semantics (uniform across the whole program)::

    T-00:05:00   5 minutes 00 seconds remain until that moment
    T+00:00:12   that moment passed 12 seconds ago
    T-00:00:00   exactly on the moment (the second it falls in)

The program deals with two moments:

* Target time ``T``                -- subtitle (how long ago the target time passed)
* Offset moment ``M = T + offset`` -- main title (how long until the offset moment)

All times are computed in the machine's local time (naive datetime).
"""

from __future__ import annotations

import math
import re
from datetime import datetime, timedelta

__all__ = [
    "parse_duration",
    "parse_target",
    "split_delta",
    "format_hms",
    "format_human",
    "render_info",
]

_PLAIN_NUMBER_RE = re.compile(r"^[+-]?\d+(?:\.\d+)?$")
_TIME_ONLY_RE = re.compile(r"^(?P<h>\d{1,2}):(?P<m>\d{1,2})(?::(?P<s>\d{1,2}))?$")
_UNIT_RE = re.compile(
    r"^(?P<sign>[+-]?)\s*"
    r"(?:(?P<days>\d+(?:\.\d+)?)\s*d(?:ay)?s?)?\s*"
    r"(?:(?P<hours>\d+(?:\.\d+)?)\s*h(?:(?:ou)?rs?)?)?\s*"
    r"(?:(?P<minutes>\d+(?:\.\d+)?)\s*m(?:in(?:ute)?s?)?)?\s*"
    r"(?:(?P<seconds>\d+(?:\.\d+)?)\s*s(?:ec(?:ond)?s?)?)?$",
    re.IGNORECASE,
)

_TARGET_FORMATS = (
    "%Y/%m/%d %H:%M:%S",
    "%Y/%m/%d %H:%M",
    "%Y/%m/%d",
    "%Y.%m.%d %H:%M:%S",
    "%Y.%m.%d %H:%M",
    "%m-%d %H:%M:%S",
    "%m-%d %H:%M",
    "%m/%d %H:%M:%S",
    "%m/%d %H:%M",
    "%d %H:%M:%S",
)


def _parse_colon(text: str) -> float:
    """Parse a colon duration: ``HH:MM:SS`` / ``MM:SS`` / ``DD:HH:MM:SS``, with an optional leading sign."""
    sign = 1.0
    if text[:1] in "+-":
        sign = -1.0 if text[0] == "-" else 1.0
        text = text[1:]
    parts = text.split(":")
    if not 1 <= len(parts) <= 4 or not all(p.strip().isdigit() for p in parts):
        raise ValueError(f"unrecognised duration: {text!r}")
    values = [int(p) for p in reversed(parts)]  # seconds, minutes, hours, days
    values += [0] * (4 - len(values))
    sec, minute, hour, day = values
    return sign * (day * 86400 + hour * 3600 + minute * 60 + sec)


def parse_duration(spec: str | float | int | None) -> float:
    """Parse a duration and return the number of seconds as a float.

    Accepts ``30`` / ``-30`` (seconds), ``90s`` / ``5m`` / ``1h30m`` / ``1d2h``,
    ``00:05:00`` / ``5:00`` / ``-00:05:00`` and similar forms.
    """
    if spec is None:
        return 0.0
    if isinstance(spec, (int, float)):
        return float(spec)
    text = str(spec).strip()
    if not text:
        return 0.0
    if _PLAIN_NUMBER_RE.match(text):
        return float(text)
    if ":" in text:
        return _parse_colon(text)
    match = _UNIT_RE.match(text)
    if not match or not any(
        match.group(g) for g in ("days", "hours", "minutes", "seconds")
    ):
        raise ValueError(f"unrecognised duration: {spec!r} (examples: -00:05:00 / 1h30m / 90)")
    total = 0.0
    for group, unit in (
        ("days", 86400.0),
        ("hours", 3600.0),
        ("minutes", 60.0),
        ("seconds", 1.0),
    ):
        value = match.group(group)
        if value:
            total += float(value) * unit
    return -total if match.group("sign") == "-" else total


def parse_target(spec: str, now: datetime | None = None) -> datetime:
    """Parse a target time.

    Accepts:

    * ``2026-01-01T09:30:00`` / ``2026-01-01 09:30`` / ``2026-01-01`` (ISO / common formats)
    * ``09:30`` / ``09:30:00`` (that clock time today; rolls over to tomorrow once it has passed)
    * ``+1h30m`` / ``-10m`` (relative to now, the unit is required)
    """
    now = now or datetime.now()
    text = (spec or "").strip()
    if not text:
        raise ValueError("the target time is empty; set [time] target in config.toml")

    # relative to now: +1h30m / -10m
    if text[0] in "+-" and re.search(r"[a-zA-Z]$", text):
        delta = parse_duration(text)
        if delta:
            return now + timedelta(seconds=delta)

    # time only: that clock time today, or tomorrow once it has passed
    match = _TIME_ONLY_RE.match(text)
    if match:
        hour, minute = int(match["h"]), int(match["m"])
        second = int(match["s"] or 0)
        if hour > 23 or minute > 59 or second > 59:
            raise ValueError(f"time out of range: {spec!r}")
        target = datetime(now.year, now.month, now.day, hour, minute, second)
        if target <= now:
            target += timedelta(days=1)
        return target

    try:
        return datetime.fromisoformat(text)
    except ValueError:
        pass

    for fmt in _TARGET_FORMATS:
        try:
            parsed = datetime.strptime(text, fmt)
        except ValueError:
            continue
        if "%Y" not in fmt:
            # no year in the spec: guess the current year
            parsed = parsed.replace(year=now.year)
        return parsed

    raise ValueError(
        f"unrecognised time format: {spec!r}"
        " (examples: 2026-01-01T09:30:00 / 2026-01-01 09:30 / 09:30 / +1h30m)"
    )


def split_delta(delta_seconds: float) -> tuple[str, int]:
    """Split the "moment - now" second difference into (sign, whole seconds to zero-pad).

    Not reached yet -> ``("-", seconds remaining)``; already past -> ``("+", seconds elapsed)``.
    The remaining amount is rounded up and the elapsed amount rounded down, so the exact
    moment second displays as ``T-00:00:00``.
    """
    if delta_seconds >= 0:
        return "-", int(math.ceil(delta_seconds))
    return "+", int(math.floor(-delta_seconds))


def format_hms(total_seconds: int | float, show_days: bool = True) -> str:
    """Zero-padded fixed-width time string: ``HH:MM:SS``, or ``DD:HH:MM:SS`` past a day."""
    seconds = max(0, int(total_seconds))
    days, rest = divmod(seconds, 86400)
    hours, rest = divmod(rest, 3600)
    minutes, secs = divmod(rest, 60)
    if days and show_days:
        return f"{days:02d}:{hours:02d}:{minutes:02d}:{secs:02d}"
    return f"{days * 24 + hours:02d}:{minutes:02d}:{secs:02d}"


def format_human(total_seconds: int | float) -> str:
    """Human-readable duration used by notification bodies: ``1h 30m``.

    Zero-valued components are omitted and the rest joined with a single space.
    Units are ``d``, ``h``, ``m`` and ``s``; once a day component is present the
    seconds component is dropped entirely. Never returns an empty string.
    """
    seconds = max(0, int(round(float(total_seconds))))
    if seconds < 60:
        return f"{seconds}s"
    days, rest = divmod(seconds, 86400)
    hours, rest = divmod(rest, 3600)
    minutes, secs = divmod(rest, 60)
    parts: list[str] = []
    if days:
        parts.append(f"{days}d")
    if hours:
        parts.append(f"{hours}h")
    if minutes:
        parts.append(f"{minutes}m")
    if secs and not days:
        parts.append(f"{secs}s")
    return " ".join(parts) or "0s"


_INFO_TOKENS = (
    "datetime",
    "mark_datetime",
    "date",
    "time",
    "mark",
    "delta_human",
    "delta",
)


def render_info(
    template: str,
    target: datetime,
    mark: datetime,
    offset: float,
    show_days: bool = True,
) -> str:
    """Render the "target time + offset" line.

    Available placeholders: ``{date} {time} {datetime}`` (target time T),
    ``{mark} {mark_datetime}`` (offset moment M), ``{delta}`` (+02:00:00),
    ``{delta_human}`` (+2h). Unrecognised placeholders are left as-is.
    """
    if not template.strip():
        return ""
    sign = "+" if offset >= 0 else "-"
    magnitude = abs(offset)
    values = {
        "date": target.strftime("%Y-%m-%d"),
        "time": target.strftime("%H:%M:%S"),
        "datetime": target.strftime("%Y-%m-%d %H:%M:%S"),
        "mark": mark.strftime("%H:%M:%S"),
        "mark_datetime": mark.strftime("%Y-%m-%d %H:%M:%S"),
        "delta": f"{sign}{format_hms(magnitude, show_days)}",
        "delta_human": f"{sign}{format_human(magnitude)}",
    }
    text = template
    for key in _INFO_TOKENS:
        text = text.replace("{" + key + "}", values[key])
    return text
