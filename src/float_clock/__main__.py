"""Command-line entry point."""

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
        description=(
            "Borderless, transparent T± countdown overlay"
            " (bold green monospace text / draggable / right-click to lock"
            " / system notifications near each moment)"
        ),
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=(
            "Examples:\n"
            "  uv run float-clock --init-config\n"
            '  uv run float-clock --target "2026-01-01 09:30" --offset -00:05:00\n'
            "  uv run float-clock --print            "
            "# print the current T± state and the reminder schedule, no window\n"
        ),
    )
    parser.add_argument("--config", type=Path, help="path to the config file (default ./config.toml)")
    parser.add_argument("--init-config", action="store_true", help="write a default config file, then exit")
    parser.add_argument("--force", action="store_true", help="with --init-config, overwrite an existing config")
    parser.add_argument("--target", help="target time T, e.g. '2026-01-01T09:30:00' / '09:30' / '+1h'")
    parser.add_argument("--offset", help="offset, e.g. '-00:05:00' / '1h30m' / '-300'")
    parser.add_argument("--settings", action="store_true", help="open the settings window on startup")
    parser.add_argument("--print", dest="print_only", action="store_true", help="print the state only, without opening a window")
    parser.add_argument("--selftest", type=float, metavar="SECONDS", help="close the window automatically after N seconds (self-check)")
    parser.add_argument("--test-notify", action="store_true", help="send one test notification, then exit")
    parser.add_argument(
        "--diagnose",
        action="store_true",
        help=(
            "print the window state one second after it opens"
            " (whether borderless / transparent really took effect), then exit"
        ),
    )
    parser.add_argument("--version", action="version", version=f"float-clock {__version__}")
    return parser


def _normalize_argv(argv: list[str]) -> list[str]:
    """Keep a negative value such as ``--offset -00:05:00`` from being read as an option.

    argparse only recognises negative numbers of the form ``-1`` / ``-1.5``, so
    ``-00:05:00`` would be taken for an unknown option; both are rewritten here as
    ``--offset=-00:05:00``.
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

    print(f"now          : {now:%Y-%m-%d %H:%M:%S}")
    print(f"target  T    : {target:%Y-%m-%d %H:%M:%S}")
    print(f"offset       : {offset:+.0f} s")
    print(f"offset  M    : {mark:%Y-%m-%d %H:%M:%S}")
    print(f"main title   : {main_text}")
    print(f"subtitle     : {sub_text}")
    info_text = render_info(
        config.display.info_template, target, mark, offset, config.display.show_days
    )
    print(f"third line   : {info_text or '(switched off)'}")

    notifier = MomentNotifier(config.notify)
    notifier.arm([("Offset moment", mark), ("Target time", target)], now)
    upcoming = notifier.upcoming(now)
    if upcoming:
        print("upcoming reminders:")
        for fire_at, title in upcoming:
            print(f"  {fire_at:%H:%M:%S}  {title}")
    else:
        print("upcoming reminders: none")
    return 0


def main(argv: list[str] | None = None) -> int:
    raw = list(sys.argv[1:] if argv is None else argv)
    args = build_parser().parse_args(_normalize_argv(raw))
    config_path = args.config.expanduser().resolve() if args.config else default_config_path()

    if args.init_config:
        if config_path.exists() and not args.force:
            print(f"the config already exists: {config_path} (pass --force to overwrite it)", file=sys.stderr)
            return 1
        write_default_config(config_path)
        print(f"wrote {config_path}")
        return 0

    if not config_path.exists():
        write_default_config(config_path)
        print(f"no config found, wrote a default one to {config_path}")

    try:
        config = load_config(config_path)
    except Exception as exc:  # noqa: BLE001 - TOML syntax errors and the like: just tell the user
        print(f"could not read the config: {exc}", file=sys.stderr)
        return 1

    _apply_overrides(config, args)

    if args.test_notify:
        from . import notify as notify_module

        print(f"notification backend: {notify_module.backend()}")
        ok = notify_module.send(
            "[T-00:00:00] FloatClock test",
            "If you can see this, system notifications work",
            config.notify.sound_name if config.notify.sound else None,
        )
        print("send result:", "ok" if ok else "failed (see the stderr above)")
        return 0 if ok else 1

    if args.print_only:
        return _print_status(config)

    from .overlay import FloatingClock  # imported lazily: --print / --init-config need no GUI

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
