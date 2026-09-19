"""时间解析与 T± 格式化工具。

T± 语义约定（全程序统一）::

    T-00:05:00   距离该时刻还有 5 分 00 秒
    T+00:00:12   该时刻已经过去 12 秒
    T-00:00:00   正点（该时刻所在的这一秒）

程序里有两个时刻：

* 目标时间点 ``T``            —— 副标题（距离时间点过去了多久）
* 偏移时刻 ``M = T + offset`` —— 主标题（距离偏移还有多久）

所有时间均按本机本地时间（naive datetime）计算。
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
    """解析冒号时长：``HH:MM:SS`` / ``MM:SS`` / ``DD:HH:MM:SS``，可带前导正负号。"""
    sign = 1.0
    if text[:1] in "+-":
        sign = -1.0 if text[0] == "-" else 1.0
        text = text[1:]
    parts = text.split(":")
    if not 1 <= len(parts) <= 4 or not all(p.strip().isdigit() for p in parts):
        raise ValueError(f"无法识别的时长：{text!r}")
    values = [int(p) for p in reversed(parts)]  # 秒, 分, 时, 天
    values += [0] * (4 - len(values))
    sec, minute, hour, day = values
    return sign * (day * 86400 + hour * 3600 + minute * 60 + sec)


def parse_duration(spec: str | float | int | None) -> float:
    """解析时长，返回秒数（float）。

    支持 ``30`` / ``-30``（秒）、``90s`` / ``5m`` / ``1h30m`` / ``1d2h``、
    ``00:05:00`` / ``5:00`` / ``-00:05:00`` 等写法。
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
        raise ValueError(f"无法识别的时长：{spec!r}（示例：-00:05:00 / 1h30m / 90）")
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
    """解析目标时间点。

    支持：

    * ``2026-01-01T09:30:00`` / ``2026-01-01 09:30`` / ``2026-01-01``（ISO / 常见格式）
    * ``09:30`` / ``09:30:00``（今天该时刻，已过则顺延到明天）
    * ``+1h30m`` / ``-10m``（相对现在，必须带单位）
    """
    now = now or datetime.now()
    text = (spec or "").strip()
    if not text:
        raise ValueError("目标时间点为空，请在 config.toml 的 [time] target 里设置")

    # 相对现在：+1h30m / -10m
    if text[0] in "+-" and re.search(r"[a-zA-Z]$", text):
        delta = parse_duration(text)
        if delta:
            return now + timedelta(seconds=delta)

    # 只有时间：今天该时刻，已过则明天
    match = _TIME_ONLY_RE.match(text)
    if match:
        hour, minute = int(match["h"]), int(match["m"])
        second = int(match["s"] or 0)
        if hour > 23 or minute > 59 or second > 59:
            raise ValueError(f"时间超出范围：{spec!r}")
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
            parsed = parsed.replace(year=now.year)
        return parsed

    raise ValueError(
        f"无法识别的时间格式：{spec!r}"
        "（示例：2026-01-01T09:30:00 / 2026-01-01 09:30 / 09:30 / +1h30m）"
    )


def split_delta(delta_seconds: float) -> tuple[str, int]:
    """把「时刻 - 现在」的秒差拆成 (符号, 补零用的总秒数)。

    还没到 → ``("-", 剩余秒数)``；已经过去 → ``("+", 已过秒数)``。
    剩余量向上取整、已过量向下取整，这样正点那一秒会显示 ``T-00:00:00``。
    """
    if delta_seconds >= 0:
        return "-", int(math.ceil(delta_seconds))
    return "+", int(math.floor(-delta_seconds))


def format_hms(total_seconds: int | float, show_days: bool = True) -> str:
    """补零等宽时间串：``HH:MM:SS``，超过一天时 ``DD:HH:MM:SS``。"""
    seconds = max(0, int(total_seconds))
    days, rest = divmod(seconds, 86400)
    hours, rest = divmod(rest, 3600)
    minutes, secs = divmod(rest, 60)
    if days and show_days:
        return f"{days:02d}:{hours:02d}:{minutes:02d}:{secs:02d}"
    return f"{days * 24 + hours:02d}:{minutes:02d}:{secs:02d}"


def format_human(total_seconds: int | float) -> str:
    """人类可读的时长，用于通知正文：``1 小时 30 分``。"""
    seconds = max(0, int(round(float(total_seconds))))
    if seconds < 60:
        return f"{seconds} 秒"
    days, rest = divmod(seconds, 86400)
    hours, rest = divmod(rest, 3600)
    minutes, secs = divmod(rest, 60)
    parts: list[str] = []
    if days:
        parts.append(f"{days} 天")
    if hours:
        parts.append(f"{hours} 小时")
    if minutes:
        parts.append(f"{minutes} 分")
    if secs and not days:
        parts.append(f"{secs} 秒")
    return " ".join(parts) or "0 秒"


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
    """渲染「时间点 + 偏移」那一行。

    可用占位符：``{date} {time} {datetime}``（目标时间点 T）、
    ``{mark} {mark_datetime}``（偏移时刻 M）、``{delta}``（+02:00:00）、
    ``{delta_human}``（+2 小时）。认不出的占位符原样保留。
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
