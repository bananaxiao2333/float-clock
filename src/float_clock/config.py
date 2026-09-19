"""配置：TOML 读取、默认值合并、保留注释的回写、默认配置生成。"""

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
    # 第三行：显示目标时间点与偏移（同样绿字、透明底）
    info_template: str = "{time} · {delta}"
    info_size: int = 14
    info_color: str = ""
    # 第三行样式：auto = 有原生叠层就镂空、否则普通绿字；knockout / text 强制指定
    info_style: str = "auto"
    x11_background: str = "#101010"
    interval_ms: int = 200
    main_template: str = "T{sign}{clock}"
    sub_template: str = "T{sign}{clock}"
    # 锁定状态用外框线表示：实线=锁定，虚线=可拖动
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
    # 文案模板，纯文本，用 [] 这类符号做装饰
    title_template: str = "[T{sign}{clock}] {label}"
    body_before: str = "距离{label}还有 {human}"
    body_at: str = "{label}已到 · {time}"
    body_after: str = "{label}已过去 {human}"


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
    """按 dataclass 字段挑出配置里认识的键，类型尽量对齐。"""
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
    """读取配置；文件不存在或某段缺失时用默认值补齐。"""
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
    """就地把 ``"节.键": 值`` 写回 TOML，保留注释与原有顺序。

    缺失的键会追加到对应小节末尾；小节不存在则新建。
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
            # 小节不存在：新建，键直接跟在节头后面
            if out and out[-1].strip():
                out.append("")
            out.append(f"[{target_section}]")
            out.append(line)
            seen_sections.append(target_section)
            continue
        # 小节已存在：插到该节最后一行键的后面
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
# FloatClock 悬浮倒计时配置
# 改完保存即可，程序约 1 秒内自动重载（无需重启）。
# 命令行 --target / --offset 可临时覆盖本文件。

[window]
# 浮窗左上角坐标（拖动后会自动写回这里）
x = {x}
y = {y}
# 去掉标题栏，配合透明背景实现「只有文字」的悬浮效果
borderless = true
# 永远置顶
topmost = true
# 锁定后不能拖动（右键随时切换；锁定状态也会写回这里）
locked = false
# 整体不透明度 0.05~1.0（透明背景已够用，一般保持 1.0）
opacity = 1.0

[display]
# 等宽字体；留空自动挑选系统等宽字体（Menlo / Consolas / DejaVu Sans Mono ...）
font_family = ""
# 主标题（距离偏移还有多久）字号
main_size = 46
# 副标题（距离目标时间点过去了多久）字号
sub_size = 18
# 绿色粗体
color = "#00FF66"
# 副标题颜色，留空表示跟主标题一致
sub_color = ""
bold = true
# 超过一天时显示 DD:HH:MM:SS（所有位都补 0 对齐）
show_days = true
# 主副标题之间的间距（像素）
gap = 2
# 第三行：目标时间点 + 偏移（同样绿字、透明底）。设成空字符串 "" 则整行不显示。
# 可用占位符：
#   {{date}} {{time}} {{datetime}}    目标时间点 T
#   {{mark}} {{mark_datetime}}      偏移时刻 M = T + offset
#   {{delta}}                   偏移的补零写法，如 +02:00:00
#   {{delta_human}}             偏移的中文写法，如 +2 小时
info_template = "{{time}} · {{delta}}"
# 第三行字号
info_size = 14
# 第三行颜色，留空 = 跟主标题一致（绿色）
info_color = ""
# 第三行样式：
#   "auto"     = 有原生叠层（macOS）就镂空，其它平台自动用普通绿字
#   "knockout" = 强制镂空（非 macOS 会退回普通绿字）
#   "text"     = 强制普通绿字、透明底
info_style = "auto"
# Linux 等不支持透明背景的平台上用的底色
x11_background = "#101010"
# 刷新间隔（毫秒）
interval_ms = 200
# 锁定状态指示：border = 用外框线表示（实线=锁定，虚线=可拖动），none = 不显示
lock_indicator = "border"
# 外框线颜色，留空 = 跟文字同色
border_color = ""
# 外框线宽（像素）
border_width = 2
# true：锁定时实线、解锁时虚线；false 反过来
solid_when_locked = true
# 显示模板：{{sign}} 就是 + / -，{{clock}} 是补零时间
main_template = "T{{sign}}{{clock}}"
sub_template = "T{{sign}}{{clock}}"

[time]
# 目标时间点。支持：
#   2026-01-01T09:30:00   2026-01-01 09:30   2026-01-01
#   09:30:00（今天该时刻，已过则自动顺延到明天 —— 每天重复的日程用这种写法）
#   +1h30m（相对现在）
# 绝对时间点过去之后，显示会自然翻转成 T+…
target = "{target}"
# 偏移。主标题倒计时指向的时刻 = 目标时间点 + 偏移。
# 支持 -00:05:00 / 5:00 / 1h30m / -300 / 0 等写法
offset = "{offset}"

[notify]
# 总开关
enabled = true
# 在每个时间点【之前】这些秒数各提醒一次
before = [3600, 1800, 900, 600, 300, 180, 120, 60, 30, 10, 5, 3, 2, 1]
# 在每个时间点【之后】这些秒数各提醒一次
after = [1, 5, 30, 60, 300]
# 正点提醒
at_moment = true
# 通知声音
sound = true
# macOS 提示音：Glass / Ping / Pop / Funk / Basso / Blow / Bottle / Frog / Hero / Morse / Purr / Sosumi / Submarine / Tink
sound_name = "Glass"
# 通知文案模板（纯文本，不带 emoji，用 [] 这类符号做装饰）。占位符：
#   {{label}} 时间点名称   {{sign}} - 或 +   {{clock}} 补零时间   {{human}} 人话时长
#   {{time}} 该时刻 HH:MM:SS   {{datetime}} 该时刻完整时间
title_template = "[T{{sign}}{{clock}}] {{label}}"
body_before = "距离{{label}}还有 {{human}}"
body_at = "{{label}}已到 · {{time}}"
body_after = "{{label}}已过去 {{human}}"
"""


def default_target(now: datetime | None = None) -> str:
    """默认目标时间点：下一个整点。"""
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
