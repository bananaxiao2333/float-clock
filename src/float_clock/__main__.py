"""命令行入口。"""

from __future__ import annotations

import argparse
import sys
from datetime import datetime, timedelta
from pathlib import Path

from . import __version__
from .config import (
    Config,
    default_config_path,
    load_config,
    write_default_config,
)
from .notifier import MomentNotifier
from .timefmt import format_hms, parse_duration, parse_target, render_info, split_delta


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="float-clock",
        description="无背景悬浮 T± 倒计时浮窗（绿色粗体等宽字体 / 可拖动 / 右键锁定 / 临近时间点系统通知）",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=(
            "示例：\n"
            "  uv run float-clock --init-config\n"
            '  uv run float-clock --target "2026-01-01 09:30" --offset -00:05:00\n'
            "  uv run float-clock --print            # 不开窗口，只打印当前 T± 与提醒计划\n"
        ),
    )
    parser.add_argument("--config", type=Path, help="配置文件路径（默认 ./config.toml）")
    parser.add_argument("--init-config", action="store_true", help="生成默认配置后退出")
    parser.add_argument("--force", action="store_true", help="配合 --init-config，覆盖已有配置")
    parser.add_argument("--target", help="目标时间点，如 '2026-01-01T09:30:00' / '09:30' / '+1h'")
    parser.add_argument("--offset", help="偏移，如 '-00:05:00' / '1h30m' / '-300'")
    parser.add_argument("--settings", action="store_true", help="启动后直接打开设置窗口")
    parser.add_argument("--print", dest="print_only", action="store_true", help="只打印状态，不打开窗口")
    parser.add_argument("--selftest", type=float, metavar="SECONDS", help="打开窗口 N 秒后自动退出（自检用）")
    parser.add_argument("--test-notify", action="store_true", help="发一条测试通知后退出")
    parser.add_argument(
        "--diagnose",
        action="store_true",
        help="打开窗口 1 秒后打印窗口状态（无边框 / 透明是否真的生效）并退出",
    )
    parser.add_argument("--version", action="version", version=f"float-clock {__version__}")
    return parser


def _normalize_argv(argv: list[str]) -> list[str]:
    """让 ``--offset -00:05:00`` 这种以负号开头的取值不被 argparse 当成选项。

    argparse 只认 ``-1`` / ``-1.5`` 形式的负数，``-00:05:00`` 会被当成未知选项，
    这里统一改写为 ``--offset=-00:05:00``。
    """
    result: list[str] = []
    index = 0
    while index < len(argv):
        item = argv[index]
        nxt = argv[index + 1] if index + 1 < len(argv) else None
        if item in ("--offset", "--target") and nxt is not None and nxt.startswith("-") and not nxt.startswith("--"):
            result.append(f"{item}={nxt}")
            index += 2
            continue
        result.append(item)
        index += 1
    return result


def _apply_overrides(config: Config, args: argparse.Namespace) -> None:
    if args.target:
        config.time.target = args.target
    if args.offset is not None:
        config.time.offset = args.offset


def _print_status(config: Config) -> int:
    now = datetime.now()
    target = parse_target(config.time.target, now)
    offset = parse_duration(config.time.offset)
    mark = target + timedelta(seconds=offset)

    sign, secs = split_delta((mark - now).total_seconds())
    main_text = config.display.main_template.replace("{sign}", sign).replace(
        "{clock}", format_hms(secs, config.display.show_days)
    )
    sign, secs = split_delta((target - now).total_seconds())
    sub_text = config.display.sub_template.replace("{sign}", sign).replace(
        "{clock}", format_hms(secs, config.display.show_days)
    )

    print(f"现在        : {now:%Y-%m-%d %H:%M:%S}")
    print(f"目标时间点 T: {target:%Y-%m-%d %H:%M:%S}")
    print(f"偏移        : {offset:+.0f} 秒")
    print(f"偏移时刻 M  : {mark:%Y-%m-%d %H:%M:%S}")
    print(f"主标题      : {main_text}")
    print(f"副标题      : {sub_text}")
    info_text = render_info(
        config.display.info_template, target, mark, offset, config.display.show_days
    )
    print(f"第三行      : {info_text or '（已关闭）'}")

    notifier = MomentNotifier(config.notify)
    notifier.arm([("偏移时刻", mark), ("目标时间点", target)], now)
    upcoming = notifier.upcoming(now)
    if upcoming:
        print("接下来提醒  :")
        for fire_at, title in upcoming:
            print(f"  {fire_at:%H:%M:%S}  {title}")
    else:
        print("接下来提醒  : 无")
    return 0


def main(argv: list[str] | None = None) -> int:
    raw = list(sys.argv[1:] if argv is None else argv)
    args = build_parser().parse_args(_normalize_argv(raw))
    config_path = args.config.expanduser().resolve() if args.config else default_config_path()

    if args.init_config:
        if config_path.exists() and not args.force:
            print(f"配置已存在：{config_path}（要覆盖请加 --force）", file=sys.stderr)
            return 1
        write_default_config(config_path)
        print(f"已生成默认配置：{config_path}")
        return 0

    if not config_path.exists():
        write_default_config(config_path)
        print(f"未找到配置，已生成默认配置：{config_path}")

    try:
        config = load_config(config_path)
    except Exception as exc:  # noqa: BLE001 - TOML 语法错误等，直接提示用户
        print(f"读取配置失败：{exc}", file=sys.stderr)
        return 1

    _apply_overrides(config, args)

    if args.test_notify:
        from . import notify as notify_module

        print(f"通知后端：{notify_module.backend()}")
        ok = notify_module.send(
            "[FloatClock] 测试通知",
            "如果你看到这一条，通知通道就通了。",
            config.notify.sound_name if config.notify.sound else None,
        )
        print("发送结果：", "成功" if ok else "失败（详见上面的 stderr）")
        return 0 if ok else 1

    if args.print_only:
        return _print_status(config)

    from .overlay import FloatingClock  # 延迟导入：--print / --init-config 不依赖 GUI

    app = FloatingClock(config, open_settings=args.settings)
    if args.diagnose:
        def report() -> None:
            print(app.window_report())
            app.quit()

        app.root.after(1000, report)
    elif args.selftest:
        app.root.after(int(args.selftest * 1000), app.quit)
    try:
        return app.run()
    except KeyboardInterrupt:
        return 0


if __name__ == "__main__":
    raise SystemExit(main())
