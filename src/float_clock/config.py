"""Configuration: TOML loading, default merging, comment-preserving write-back, default config generation."""

from __future__ import annotations

import json
import re
import tomllib
from dataclasses import dataclass, field, fields
from datetime import datetime, timedelta
from pathlib import Path
from typing import Any

__all__ = [
    "Config",
    "WindowConfig",
    "DisplayConfig",
    "TimeConfig",
    "NotifyConfig",
    "load_config",
    "write_default_config",
    "patch_toml",
    "default_config_path",
]

DEFAULT_CONFIG_NAME = "config.toml"

_FULL_UPDATERS = {"before", "after"}


@dataclass
class WindowConfig:
    x: int = 80
    y: int = 80
    borderless: bool = True
    topmost: bool = True
    locked: bool = False
    opacity: float = 1.0


@dataclass
class DisplayConfig:
    font_family: str = ""
    main_size: int = 46
    sub_size: int = 18
    color: str = "#00FF66"
    sub_color: str = ""
    bold: bool = True
    show_days: bool = True
    gap: int = 2
    # third line: shows the target time and the offset (same green text, transparent background)
    info_template: str = "{time} | {delta}"
    info_size: int = 14
    info_color: str = ""
    # third-line style: auto = knock out when a native overlay is available, otherwise plain green
    # text; knockout / text force one of the two
    info_style: str = "auto"
    x11_background: str = "#101010"
    interval_ms: int = 200
    main_template: str = "T{sign}{clock}"
    sub_template: str = "T{sign}{clock}"
    # the locked state is shown by an outline: solid = locked, dashed = draggable
    lock_indicator: str = "border"
    border_color: str = ""
    border_width: int = 2
    solid_when_locked: bool = True


@dataclass
class TimeConfig:
    target: str = ""
    offset: str = "0"


@dataclass
class NotifyConfig:
    enabled: bool = True
    before: list[int] = field(
        default_factory=lambda: [3600, 1800, 900, 600, 300, 180, 120, 60, 30, 10, 5, 3, 2, 1]
    )
    after: list[int] = field(default_factory=lambda: [1, 5, 30, 60, 300])
    at_moment: bool = True
    sound: bool = True
    sound_name: str = "Glass"
    # copy templates, plain text, decorated with symbols such as []
    title_template: str = "[T{sign}{clock}] {label}"
    body_before: str = "{human} until {label}"
    body_at: str = "{label} reached at {time}"
    body_after: str = "{label} passed {human} ago"


@dataclass
class Config:
    path: Path
    window: WindowConfig = field(default_factory=WindowConfig)
    display: DisplayConfig = field(default_factory=DisplayConfig)
    time: TimeConfig = field(default_factory=TimeConfig)
    notify: NotifyConfig = field(default_factory=NotifyConfig)


_SECTIONS = {
    "window": WindowConfig,
    "display": DisplayConfig,
    "time": TimeConfig,
    "notify": NotifyConfig,
}


def default_config_path() -> Path:
    return Path.cwd() / DEFAULT_CONFIG_NAME


def _fill(dataclass_type: type, raw: dict[str, Any]) -> Any:
    """Pick the keys the dataclass knows out of the config, coercing types as closely as possible."""
    known = {f.name: f for f in fields(dataclass_type)}
    kwargs: dict[str, Any] = {}
    for key, value in raw.items():
        if key not in known:
            continue
        if key in _FULL_UPDATERS:
            kwargs[key] = [int(v) for v in value]
        else:
            default = known[key].default
            if isinstance(default, bool):
                kwargs[key] = bool(value)
            elif isinstance(default, int) and not isinstance(default, bool):
                kwargs[key] = int(value)
            elif isinstance(default, float):
                kwargs[key] = float(value)
            else:
                kwargs[key] = value
    return dataclass_type(**kwargs)


def load_config(path: Path) -> Config:
    """Read the config; a missing file or a missing section falls back to the defaults."""
    path = Path(path)
    raw: dict[str, Any] = {}
    if path.exists():
        with path.open("rb") as handle:
            raw = tomllib.load(handle)
    config = Config(path=path)
    for name, dataclass_type in _SECTIONS.items():
        section = raw.get(name)
        if isinstance(section, dict):
            setattr(config, name, _fill(dataclass_type, section))
    return config


def _toml_value(value: Any) -> str:
    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, (list, tuple)):
        return "[" + ", ".join(_toml_value(v) for v in value) + "]"
    if isinstance(value, (int, float)):
        return repr(value)
    return json.dumps(str(value), ensure_ascii=False)


_KEY_RE = re.compile(r"^([A-Za-z0-9_-]+)\s*=")


def patch_toml(path: Path, updates: dict[str, Any]) -> None:
    """Write ``"section.key": value`` back into the TOML in place, preserving comments and existing order.

    A missing key is appended at the end of its section; a section that does not
    exist yet is created.
    """
    path = Path(path)
    lines = path.read_text(encoding="utf-8").splitlines() if path.exists() else []
    pending = dict(updates)
    section = ""
    out: list[str] = []
    seen_sections: list[str] = []

    for original in lines:
        stripped = original.strip()
        if stripped.startswith("[") and stripped.endswith("]"):
            section = stripped[1:-1].strip()
            seen_sections.append(section)
            out.append(original)
            continue
        match = _KEY_RE.match(stripped)
        if match and not stripped.startswith("#"):
            full_key = f"{section}.{match.group(1)}" if section else match.group(1)
            if full_key in pending:
                indent = original[: len(original) - len(original.lstrip())]
                out.append(f"{indent}{match.group(1)} = {_toml_value(pending.pop(full_key))}")
                continue
        out.append(original)

    for full_key, value in pending.items():
        target_section, _, key = full_key.rpartition(".")
        line = f"{key} = {_toml_value(value)}"
        if not target_section:
            out.append(line)
            continue
        if target_section not in seen_sections:
            # the section does not exist: create it and put the key right after the header
            if out and out[-1].strip():
                out.append("")
            out.append(f"[{target_section}]")
            out.append(line)
            seen_sections.append(target_section)
            continue
        # the section exists: insert after the section's last key line
        insert_at = len(out)
        for index, existing in enumerate(out):
            if existing.strip() != f"[{target_section}]":
                continue
            insert_at = index + 1
            while insert_at < len(out):
                nxt = out[insert_at].strip()
                if nxt.startswith("[") and nxt.endswith("]"):
                    break
                insert_at += 1
            break
        out.insert(insert_at, line)

    path.write_text("\n".join(out).rstrip("\n") + "\n", encoding="utf-8")


TEMPLATE = """\
# FloatClock floating countdown configuration
# Edit and save; the program reloads within about a second (no restart needed).
# The --target / --offset command-line flags temporarily override this file.

[window]
# Floating window top-left corner (written back here automatically after a drag)
x = {x}
y = {y}
# Drop the title bar; together with a transparent background this gives the "text only" floating look
borderless = true
# Always on top
topmost = true
# A locked window cannot be dragged (right-click toggles it at any time; the locked state is written back here too)
locked = false
# Overall opacity 0.05~1.0 (the transparent background is usually enough, keep 1.0 in normal use)
opacity = 1.0

[display]
# Monospace font; leave empty to pick a system monospace font automatically (Menlo / Consolas / DejaVu Sans Mono ...)
font_family = ""
# Main title (how long until the offset moment) font size
main_size = 46
# Subtitle (how long ago the target time passed) font size
sub_size = 18
# Green and bold
color = "#00FF66"
# Subtitle colour; leave empty to match the main title
sub_color = ""
bold = true
# Show DD:HH:MM:SS once the duration exceeds a day (every field zero-padded for alignment)
show_days = true
# Gap between the main title and the subtitle (pixels)
gap = 2
# Third line: target time + offset (same green text, transparent background). Set it to the empty string "" to hide the whole line.
# Available placeholders:
#   {{date}} {{time}} {{datetime}}    target time T
#   {{mark}} {{mark_datetime}}      offset moment M = T + offset
#   {{delta}}                   zero-padded offset, e.g. +02:00:00
#   {{delta_human}}             compact English offset, e.g. +2h
info_template = "{{time}} | {{delta}}"
# Third-line font size
info_size = 14
# Third-line colour; leave empty = same as the main title (green)
info_color = ""
# Third-line style:
#   "auto"     = knock out when a native overlay is available (macOS), plain green text elsewhere
#   "knockout" = force knockout (falls back to plain green text off macOS)
#   "text"     = force plain green text on a transparent background
info_style = "auto"
# Background colour used on platforms without transparent background support, Linux for instance
x11_background = "#101010"
# Refresh interval (milliseconds)
interval_ms = 200
# Lock indicator: border = show an outline (solid = locked, dashed = draggable), none = hide it
lock_indicator = "border"
# Outline colour; leave empty = same colour as the text
border_color = ""
# Outline width (pixels)
border_width = 2
# true: solid while locked and dashed while unlocked; false is the other way round
solid_when_locked = true
# Display templates: {{sign}} is + / -, {{clock}} is the zero-padded time
main_template = "T{{sign}}{{clock}}"
sub_template = "T{{sign}}{{clock}}"

[time]
# Target time. Supported:
#   2026-01-01T09:30:00   2026-01-01 09:30   2026-01-01
#   09:30:00 (that clock time today, rolled over to tomorrow once it has passed; use this form for a daily repeating schedule)
#   +1h30m (relative to now)
# Once an absolute target time has passed, the display flips over to T+... on its own
target = "{target}"
# Offset. The moment the main title counts down to = target time + offset.
# Accepts forms such as -00:05:00 / 5:00 / 1h30m / -300 / 0
offset = "{offset}"

[notify]
# Master switch
enabled = true
# Notify once before every moment, at each of these numbers of seconds
before = [3600, 1800, 900, 600, 300, 180, 120, 60, 30, 10, 5, 3, 2, 1]
# Notify once after every moment, at each of these numbers of seconds
after = [1, 5, 30, 60, 300]
# Notify exactly on the moment
at_moment = true
# Notification sound
sound = true
# macOS alert sounds: Glass / Ping / Pop / Funk / Basso / Blow / Bottle / Frog / Hero / Morse / Purr / Sosumi / Submarine / Tink
sound_name = "Glass"
# Notification copy templates (plain text, no emoji, decorated with symbols such as []). Placeholders:
#   {{label}} moment name   {{sign}} - or +   {{clock}} zero-padded time   {{human}} human-readable duration
#   {{time}} HH:MM:SS of that moment   {{datetime}} full timestamp of that moment
title_template = "[T{{sign}}{{clock}}] {{label}}"
body_before = "{{human}} until {{label}}"
body_at = "{{label}} reached at {{time}}"
body_after = "{{label}} passed {{human}} ago"
"""


def default_target(now: datetime | None = None) -> str:
    """Default target time: the next whole hour."""
    now = now or datetime.now()
    nxt = (now + timedelta(hours=1)).replace(minute=0, second=0, microsecond=0)
    return nxt.strftime("%Y-%m-%dT%H:%M:%S")


def write_default_config(
    path: Path,
    *,
    x: int = 80,
    y: int = 80,
    target: str | None = None,
    offset: str = "-00:05:00",
) -> Path:
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        TEMPLATE.format(
            x=x,
            y=y,
            target=target or default_target(),
            offset=offset,
        ),
        encoding="utf-8",
    )
    return path
