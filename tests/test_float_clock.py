"""单元测试：`uv run python -m unittest discover -s tests -v`"""

from __future__ import annotations

import tempfile
import unittest
from datetime import datetime, timedelta
from pathlib import Path

from float_clock import knockout, notify
from float_clock.config import load_config, patch_toml, write_default_config
from float_clock.notifier import MomentNotifier
from float_clock.timefmt import (
    format_hms,
    format_human,
    parse_duration,
    parse_target,
    split_delta,
)

NOW = datetime(2026, 1, 1, 12, 0, 0)


class TestParseDuration(unittest.TestCase):
    def test_plain_seconds(self):
        self.assertEqual(parse_duration("90"), 90.0)
        self.assertEqual(parse_duration("-300"), -300.0)
        self.assertEqual(parse_duration(42), 42.0)
        self.assertEqual(parse_duration(""), 0.0)
        self.assertEqual(parse_duration(None), 0.0)

    def test_colon(self):
        self.assertEqual(parse_duration("-00:05:00"), -300.0)
        self.assertEqual(parse_duration("00:05:00"), 300.0)
        self.assertEqual(parse_duration("5:00"), 300.0)
        self.assertEqual(parse_duration("1:02:03"), 3723.0)
        self.assertEqual(parse_duration("-01:02:03:04"), -(86400 + 7200 + 180 + 4.0))
        self.assertEqual(parse_duration("+00:00:30"), 30.0)

    def test_units(self):
        self.assertEqual(parse_duration("1h30m"), 5400.0)
        self.assertEqual(parse_duration("-1h30m"), -5400.0)
        self.assertEqual(parse_duration("2d"), 172800.0)
        self.assertEqual(parse_duration("90s"), 90.0)
        self.assertEqual(parse_duration("1d2h3m4s"), 86400 + 7200 + 180 + 4.0)

    def test_invalid(self):
        for bad in ("abc", "1x", "--5m"):
            with self.assertRaises(ValueError):
                parse_duration(bad)


class TestParseTarget(unittest.TestCase):
    def test_iso(self):
        self.assertEqual(parse_target("2026-01-01T09:30:00", NOW), datetime(2026, 1, 1, 9, 30))
        self.assertEqual(parse_target("2026-01-01 09:30:00", NOW), datetime(2026, 1, 1, 9, 30))
        self.assertEqual(parse_target("2026-01-01", NOW), datetime(2026, 1, 1, 0, 0))

    def test_time_only_rolls_over(self):
        self.assertEqual(parse_target("18:00", NOW), datetime(2026, 1, 1, 18, 0))
        self.assertEqual(parse_target("09:30", NOW), datetime(2026, 1, 2, 9, 30))
        self.assertEqual(parse_target("12:00:00", NOW), datetime(2026, 1, 2, 12, 0))

    def test_relative(self):
        self.assertEqual(parse_target("+1h30m", NOW), NOW + timedelta(minutes=90))
        self.assertEqual(parse_target("-10m", NOW), NOW - timedelta(minutes=10))

    def test_other_formats(self):
        self.assertEqual(parse_target("2026/02/03 08:05", NOW), datetime(2026, 2, 3, 8, 5))
        self.assertEqual(parse_target("02-03 08:05", NOW), datetime(2026, 2, 3, 8, 5))

    def test_invalid(self):
        with self.assertRaises(ValueError):
            parse_target("明天早上", NOW)
        with self.assertRaises(ValueError):
            parse_target("", NOW)


class TestFormatting(unittest.TestCase):
    def test_split_delta(self):
        self.assertEqual(split_delta(90.0), ("-", 90))
        self.assertEqual(split_delta(0.4), ("-", 1))
        self.assertEqual(split_delta(0.0), ("-", 0))
        self.assertEqual(split_delta(-0.4), ("+", 0))
        self.assertEqual(split_delta(-90.6), ("+", 90))

    def test_format_hms_zero_padded(self):
        self.assertEqual(format_hms(0), "00:00:00")
        self.assertEqual(format_hms(5), "00:00:05")
        self.assertEqual(format_hms(65), "00:01:05")
        self.assertEqual(format_hms(3723), "01:02:03")
        self.assertEqual(format_hms(86400 + 3723), "01:01:02:03")
        self.assertEqual(format_hms(86400 + 3723, show_days=False), "25:02:03")

    def test_format_human(self):
        self.assertEqual(format_human(0), "0 秒")
        self.assertEqual(format_human(45), "45 秒")
        self.assertEqual(format_human(60), "1 分")
        self.assertEqual(format_human(3600), "1 小时")
        self.assertEqual(format_human(3600 + 120), "1 小时 2 分")
        self.assertEqual(format_human(86400 + 3600 + 60 + 5), "1 天 1 小时 1 分")


# 通知文案里不允许出现的 emoji / 符号区段
_EMOJI_RANGES = (
    (0x1F000, 0x1FAFF),
    (0x2190, 0x23FF),
    (0x25A0, 0x27BF),
    (0x2B00, 0x2BFF),
    (0xFE0F, 0xFE0F),
)


def _has_emoji(text: str) -> bool:
    return any(
        any(low <= ord(ch) <= high for low, high in _EMOJI_RANGES) for ch in text
    )


class TestNotifyCopy(unittest.TestCase):
    """通知文案：不要 emoji，改用 [] 这类括号装饰。"""

    def _events(self, **kwargs):
        from float_clock.config import NotifyConfig

        config = NotifyConfig(before=[3600, 60], after=[5], sound=False)
        for key, value in kwargs.items():
            setattr(config, key, value)
        notifier = MomentNotifier(config)
        start = datetime(2026, 1, 1, 12, 0, 0)
        notifier.arm([("偏移时刻", start + timedelta(seconds=7200))], start)
        return notifier._events

    def test_no_emoji_and_brackets_used(self):
        for _fire_at, _key, title, body in self._events():
            self.assertFalse(_has_emoji(title), f"标题里还有 emoji：{title!r}")
            self.assertFalse(_has_emoji(body), f"正文里还有 emoji：{body!r}")
            self.assertIn("[", title)
            self.assertIn("]", title)

    def test_copy_is_rendered(self):
        rendered = [(title, body) for _f, _k, title, body in self._events()]
        self.assertIn(("[T-01:00:00] 偏移时刻", "距离偏移时刻还有 1 小时"), rendered)
        self.assertIn(("[T-00:01:00] 偏移时刻", "距离偏移时刻还有 1 分"), rendered)
        self.assertIn("[T+00:00:05] 偏移时刻", [t for t, _b in rendered])
        self.assertTrue(any("已到" in body for _t, body in rendered))

    def test_copy_templates_are_configurable(self):
        events = self._events(
            title_template="{label} {sign}{clock}",
            body_before="{human} 后到点",
            body_at="到点了（{time}）",
            body_after="已过 {human}",
        )
        joined = " ".join(t + b for _f, _k, t, b in events)
        self.assertIn("偏移时刻 -01:00:00", joined)
        self.assertIn("1 小时 后到点", joined)
        self.assertIn("到点了（14:00:00）", joined)
        self.assertIn("已过 5 秒", joined)


class TestConfig(unittest.TestCase):
    def test_default_roundtrip(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "config.toml"
            write_default_config(path, target="2026-01-01T09:00:00", offset="-00:05:00")
            cfg = load_config(path)
            self.assertEqual(cfg.time.target, "2026-01-01T09:00:00")
            self.assertEqual(cfg.time.offset, "-00:05:00")
            self.assertTrue(cfg.display.bold)
            self.assertEqual(cfg.display.color, "#00FF66")
            self.assertEqual(cfg.display.main_size, 46)

    def test_patch_keeps_comments_and_updates(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "config.toml"
            write_default_config(path, x=10, y=20)
            patch_toml(path, {"window.x": 123, "window.locked": True, "window.y": 456})
            text = path.read_text(encoding="utf-8")
            self.assertIn("FloatClock 悬浮倒计时配置", text)
            cfg = load_config(path)
            self.assertEqual((cfg.window.x, cfg.window.y), (123, 456))
            self.assertTrue(cfg.window.locked)

    def test_patch_inserts_missing_section(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "config.toml"
            path.write_text("# hi\n[time]\ntarget = \"09:00\"\n", encoding="utf-8")
            patch_toml(path, {"notify.before": [60, 30], "time.offset": "5m"})
            cfg = load_config(path)
            self.assertEqual(cfg.notify.before, [60, 30])
            self.assertEqual(cfg.time.offset, "5m")
            self.assertIn("# hi", path.read_text(encoding="utf-8"))


class TestNotifier(unittest.TestCase):
    def setUp(self):
        self.sent: list[tuple[str, str]] = []
        self._orig = notify.send_async
        notify.send_async = lambda title, body, sound=None: self.sent.append((title, body))

    def tearDown(self):
        notify.send_async = self._orig

    def _notifier(self, **kwargs):
        from float_clock.config import NotifyConfig

        cfg = NotifyConfig(before=[60, 10], after=[5], at_moment=True, sound=False)
        for key, value in kwargs.items():
            setattr(cfg, key, value)
        return MomentNotifier(cfg)

    def test_fires_once_in_order(self):
        start = datetime(2026, 1, 1, 12, 0, 0)
        moment = start + timedelta(seconds=120)
        notifier = self._notifier()
        notifier.arm([("目标时间点", moment)], start)

        notifier.tick(start)
        self.assertEqual(self.sent, [])

        notifier.tick(start + timedelta(seconds=61))
        notifier.tick(start + timedelta(seconds=62))
        self.assertEqual(len(self.sent), 1)
        self.assertIn("T-00:01:00", self.sent[0][0])

        notifier.tick(start + timedelta(seconds=111))
        self.assertEqual(len(self.sent), 2)
        self.assertIn("T-00:00:10", self.sent[1][0])

        notifier.tick(moment)
        self.assertEqual(len(self.sent), 3)
        title, body = self.sent[2]
        self.assertEqual(title, "[T-00:00:00] 目标时间点")
        self.assertIn("已到", body)

        notifier.tick(moment + timedelta(seconds=5))
        self.assertEqual(len(self.sent), 4)
        self.assertIn("T+00:00:05", self.sent[3][0])

    def test_past_events_not_replayed(self):
        start = datetime(2026, 1, 1, 12, 0, 0)
        moment = start - timedelta(seconds=600)
        notifier = self._notifier()
        notifier.arm([("目标时间点", moment)], start)
        notifier.tick(start)
        self.assertEqual(self.sent, [])

    def test_rearm_keeps_fired_state(self):
        start = datetime(2026, 1, 1, 12, 0, 0)
        moment = start + timedelta(seconds=120)
        notifier = self._notifier()
        notifier.arm([("目标时间点", moment)], start)
        notifier.tick(start + timedelta(seconds=61))
        self.assertEqual(len(self.sent), 1)
        # 拖动窗口会写回 config.toml 触发重载 → 重新布防不应重复弹窗
        notifier.arm([("目标时间点", moment)], start + timedelta(seconds=62))
        notifier.tick(start + timedelta(seconds=63))
        self.assertEqual(len(self.sent), 1)


class TestKnockout(unittest.TestCase):
    """第三行「绿底 + 镂空字」的位图生成（不需要窗口系统）。"""

    @classmethod
    def setUpClass(cls):
        if not knockout.available():
            raise unittest.SkipTest("没装 Pillow")
        cls.face = knockout.find_font_file("Menlo", bold=True)
        if cls.face is None:
            raise unittest.SkipTest("没有可用的等宽字体文件")

    def test_hex_to_rgb(self):
        self.assertEqual(knockout.hex_to_rgb("#00FF66"), (0, 255, 102))
        self.assertEqual(knockout.hex_to_rgb("00ff66"), (0, 255, 102))
        self.assertEqual(knockout.hex_to_rgb("#0f6"), (0, 255, 102))
        self.assertEqual(knockout.hex_to_rgb("乱写"), (0, 255, 102))

    def test_calibrate_px_size_is_monotonic(self):
        small = knockout.calibrate_px_size(20, *self.face)
        big = knockout.calibrate_px_size(60, *self.face)
        self.assertLess(small, big)
        self.assertGreaterEqual(small, 6)

    def test_render_knockout_has_block_and_holes(self):
        image = knockout.render_knockout(
            "20:29:00", self.face[0], self.face[1], 27, (220, 40), (0, 255, 102)
        )
        self.assertEqual(image.size, (220, 40))
        self.assertEqual(image.mode, "RGBA")
        alphas = [image.getpixel((x, y))[3] for y in range(40) for x in range(220)]
        holes = sum(1 for a in alphas if a == 0)
        solid = sum(1 for a in alphas if a == 255)
        self.assertGreater(solid, len(alphas) * 0.5, "绿底没铺满")
        self.assertGreater(holes, len(alphas) * 0.03, "字没挖出来")
        self.assertLess(holes, len(alphas) * 0.6, "挖太多了，字不成形")
        # 挖空的地方必须是「整块颜色保留、只有 alpha 归零」
        hole_pixel = next(
            (x, y) for y in range(40) for x in range(220) if image.getpixel((x, y))[3] == 0
        )
        self.assertEqual(image.getpixel(hole_pixel)[:3], (0, 255, 102))

    def test_empty_text_is_handled(self):
        image = knockout.render_knockout(
            "", self.face[0], self.face[1], 20, (40, 20), (0, 255, 102)
        )
        self.assertEqual(image.size, (40, 20))


if __name__ == "__main__":
    unittest.main()
