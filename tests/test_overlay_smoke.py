"""GUI 冒烟测试：真实创建 Tk 窗口，验证透明背景、拖动、锁定写回、补零渲染。

没有图形环境（无 DISPLAY / 无窗口服务器）时整组跳过。
运行：``uv run python -m unittest discover -s tests -v``
"""

from __future__ import annotations

import subprocess
import sys
import tempfile
import unittest
from datetime import datetime
from pathlib import Path

from float_clock.tclenv import ensure_tcl_library

# venv 里必须先指好 Tcl/Tk 数据目录，否则 Tk() 直接起不来
ensure_tcl_library()

try:
    import tkinter as tk

    _root_probe = tk.Tk()
    _root_probe.withdraw()
    _root_probe.destroy()
    TK_OK = True
    TK_REASON = ""
except Exception as exc:  # pragma: no cover - 无图形环境
    TK_OK = False
    TK_REASON = f"没有可用的 Tk 图形环境：{exc}"

if TK_OK:
    from float_clock import notify as _notify

    # 测试期间不要弹真实系统通知
    _notify.send_async = lambda *args, **kwargs: None

    from float_clock.config import load_config, patch_toml, write_default_config
    from float_clock.overlay import FloatingClock

    try:
        import macos_probe
    except Exception:  # pragma: no cover
        macos_probe = None

# 基线：目标时间点 2026-06-01 09:30:00，偏移 -5 分钟 → 偏移时刻 09:25:00
BASE_TARGET = "2026-06-01T09:30:00"
BASE_OFFSET = "-00:05:00"
WINDOW_TITLE = "FloatClock 悬浮倒计时"


class _Event:
    """伪造 Tk 鼠标事件。"""

    def __init__(self, x_root: int, y_root: int) -> None:
        self.x_root = x_root
        self.y_root = y_root


@unittest.skipUnless(TK_OK, TK_REASON)
class TestOverlaySmoke(unittest.TestCase):
    tmp: tempfile.TemporaryDirectory
    cfg_path: Path
    app: "FloatingClock"

    @classmethod
    def setUpClass(cls) -> None:
        cls.tmp = tempfile.TemporaryDirectory()
        cls.cfg_path = Path(cls.tmp.name) / "config.toml"
        write_default_config(
            cls.cfg_path,
            x=200,
            y=150,
            target=BASE_TARGET,
            offset=BASE_OFFSET,
        )
        cfg = load_config(cls.cfg_path)
        cfg.display.interval_ms = 50
        cls.app = FloatingClock(cfg)
        cls.app.root.update()

    @classmethod
    def tearDownClass(cls) -> None:
        try:
            cls.app.quit()
        finally:
            cls.tmp.cleanup()

    @classmethod
    def setUpClass_extra(cls):  # pragma: no cover - 占位
        pass

    def setUp(self) -> None:
        """每个用例都回到基线，避免相互污染。"""
        app = self.app
        app.locked = False
        patch_toml(
            self.cfg_path,
            {
                "time.target": BASE_TARGET,
                "time.offset": BASE_OFFSET,
                "window.locked": False,
            },
        )
        app.reload_config(force=True)
        app.root.update()

    # ------------------------------------------------------------------ 透明
    def test_background_is_transparent(self):
        app = self.app
        if sys.platform == "darwin":
            self.assertEqual(app.bg, "systemTransparent")
            self.assertTrue(bool(app.root.wm_attributes("-transparent")))
        # 所有控件都用同一个「透明底色」，不能出现实心色块
        for widget in (app.frame, app.main_label, app.sub_label, app.info_label):
            self.assertEqual(str(widget.cget("bg")), app.bg)
        self.assertEqual(str(app.root.cget("bg")), app.bg)

    @unittest.skipUnless(
        macos_probe is not None and macos_probe.AVAILABLE,
        "只在 macOS 上做像素级透明度检查",
    )
    def test_background_pixels_are_really_transparent(self):
        """核心回归测试。

        Tk 9.0 的 macOS 后端会把 `systemTransparent` 画成不透明黑（浮窗变黑底 +
        文字拖影），这里直接读窗口渲染出来的像素 alpha，黑底会被立刻抓出来。
        """
        app = self.app
        app.root.update()
        pixels = macos_probe.sample_window(WINDOW_TITLE)
        self.assertIsNotNone(pixels, "找不到窗口位图")
        assert pixels is not None

        self.assertFalse(pixels.is_opaque, "NSWindow 仍是不透明的")
        self.assertEqual(
            pixels.background_alpha,
            0,
            f"整窗最常见的 alpha 不是 0（corner_rgba={pixels.corner_rgba}）"
            f"—— Tk {tk.TkVersion} 把 systemTransparent 画成了不透明色",
        )
        self.assertGreater(
            pixels.transparent_ratio,
            0.5,
            f"只有 {pixels.transparent_ratio:.0%} 的像素透明，说明整块背景被涂满了",
        )
        # 绿色文字必须仍然画出来（别把透明做过头把字也弄没了）
        self.assertLess(pixels.transparent_ratio, 0.99, "整窗全透明，文字没画出来")

    def test_three_lines(self):
        """窗口正好三行：主标题、副标题、时间点+偏移。"""
        app = self.app
        app.root.update()
        self.assertEqual(
            list(app.frame.winfo_children()),
            [app.main_label, app.sub_label, app.info_label],
        )
        for label in (app.main_label, app.sub_label, app.info_label):
            self.assertEqual(label.pack_info()["side"], "top")
            self.assertEqual(str(label.cget("bg")), app.bg, "三行都必须透明底")
        # 前两行是普通绿字；第三行交给原生叠层画（label 只占位，不自己画字）
        self.assertEqual(app.main_label.cget("fg"), "#00FF66")
        self.assertEqual(app.sub_label.cget("fg"), "#00FF66")

    def test_info_line_shows_timepoint_and_delta(self):
        """第三行显示目标时间点 + 偏移，且随配置变化、可关闭。"""
        app = self.app
        app._render(datetime(2026, 6, 1, 9, 29, 55))
        self.assertEqual(app.info_label.cget("text"), "09:30:00 · -00:05:00")

        app.cfg.display.info_template = "{datetime} ({delta_human}) → {mark}"
        app._render(datetime(2026, 6, 1, 9, 29, 55))
        self.assertEqual(
            app.info_label.cget("text"), "2026-06-01 09:30:00 (-5 分) → 09:25:00"
        )

        app.cfg.display.info_template = ""
        app._render(datetime(2026, 6, 1, 9, 29, 55))
        self.assertEqual(app.info_label.cget("text").strip(), "")

    def test_window_is_borderless_and_topmost(self):
        app = self.app
        app.root.update()
        self.assertTrue(bool(app.root.overrideredirect()))
        self.assertTrue(bool(app.root.wm_attributes("-topmost")))

    def test_no_title_bar(self):
        """回归测试：窗口必须在映射前配置好，否则 macOS 会把标题栏加回来。"""
        app = self.app
        app.root.update()
        geometry = app.root.wm_geometry()
        frame_x, frame_y = (int(v) for v in geometry.split("+", 1)[1].split("+"))
        title_bar = app.root.winfo_rooty() - frame_y
        self.assertLessEqual(title_bar, 1, f"窗口仍有 {title_bar}px 标题栏：{geometry}")
        self.assertEqual(app.root.winfo_rootx(), frame_x)

    # ------------------------------------------------------------------ 外观
    def test_green_bold_monospace(self):
        app = self.app
        display = app.cfg.display
        self.assertEqual(app.main_label.cget("fg"), "#00FF66")
        self.assertEqual(app.main_font.actual("weight"), "bold")
        self.assertEqual(app.main_font.actual("size"), display.main_size)
        self.assertEqual(app.sub_font.actual("size"), display.sub_size)

        family = app.main_font.actual("family")
        self.assertIn(family, ("TkFixedFont", "Menlo", "Monaco", "Courier New", "Courier"))
        self.assertEqual(app.family, family)

        # 等宽：数字与冒号宽度必须一致，否则逐秒跳动会左右抖
        reference = app.main_font.measure("0")
        for char in "0123456789:":
            self.assertEqual(app.main_font.measure(char), reference, f"{char!r} 不是等宽")

    # ------------------------------------------------------------------ 渲染
    def test_main_title_counts_down_to_offset_moment(self):
        app = self.app
        app._render(datetime(2026, 6, 1, 9, 20, 0))
        self.assertEqual(app.main_label.cget("text"), "T-00:05:00")  # 距离 09:25:00 还有 5 分
        self.assertEqual(app.sub_label.cget("text"), "T-00:10:00")  # 距离 09:30:00 还有 10 分

        app._render(datetime(2026, 6, 1, 9, 25, 0))
        self.assertEqual(app.main_label.cget("text"), "T-00:00:00")  # 偏移时刻正点
        self.assertEqual(app.sub_label.cget("text"), "T-00:05:00")

        app._render(datetime(2026, 6, 1, 9, 30, 0))
        self.assertEqual(app.main_label.cget("text"), "T+00:05:00")  # 偏移时刻已过去 5 分
        self.assertEqual(app.sub_label.cget("text"), "T-00:00:00")  # 目标时间点正点

        app._render(datetime(2026, 6, 1, 9, 30, 7))
        self.assertEqual(app.main_label.cget("text"), "T+00:05:07")
        self.assertEqual(app.sub_label.cget("text"), "T+00:00:07")

    def test_zero_padding_and_day_rollover(self):
        app = self.app
        app._render(datetime(2026, 5, 30, 8, 24, 57))
        self.assertEqual(app.sub_label.cget("text"), "T-02:01:05:03")  # DD:HH:MM:SS 全补零

        app.cfg.display.show_days = False
        app._render(datetime(2026, 5, 30, 8, 24, 57))
        self.assertEqual(app.sub_label.cget("text"), "T-49:05:03")  # 不用天数时小时不进位

    # ------------------------------------------------------------------ 拖动
    def test_drag_moves_window(self):
        app = self.app
        app._on_press(_Event(500, 500))
        before = (app.root.winfo_x(), app.root.winfo_y())
        app._on_drag(_Event(540, 533))
        app.root.update()
        self.assertEqual((app.root.winfo_x(), app.root.winfo_y()), (before[0] + 40, before[1] + 33))

    def test_drag_writes_position_back_to_config(self):
        app = self.app
        app._on_press(_Event(500, 500))
        app._on_drag(_Event(510, 520))
        app._on_release(_Event(510, 520))
        cfg = load_config(self.cfg_path)
        self.assertEqual((cfg.window.x, cfg.window.y), (app.root.winfo_x(), app.root.winfo_y()))

    # ------------------------------------------------------------------ 锁定
    def test_right_click_locks_then_unlocks(self):
        app = self.app

        app._on_right_click(_Event(0, 0))
        self.assertTrue(app.locked)
        self.assertTrue(load_config(self.cfg_path).window.locked)

        # 锁定后拖动无效
        app.root.update()
        before = (app.root.winfo_x(), app.root.winfo_y())
        app._on_press(_Event(300, 300))
        app._on_drag(_Event(400, 400))
        app.root.update()
        self.assertEqual((app.root.winfo_x(), app.root.winfo_y()), before)

        app._on_right_click(_Event(0, 0))
        self.assertFalse(app.locked)
        self.assertFalse(load_config(self.cfg_path).window.locked)

        # 解锁后拖动恢复
        app._on_press(_Event(300, 300))
        base = (app.root.winfo_x(), app.root.winfo_y())
        app._on_drag(_Event(330, 320))
        app.root.update()
        self.assertEqual((app.root.winfo_x(), app.root.winfo_y()), (base[0] + 30, base[1] + 20))

    def test_lock_indicator_is_border_not_text(self):
        """锁定状态只用外框线表示：解锁=虚线，锁定=实线，且不弹任何文字提示。"""
        app = self.app
        app.root.update()

        if app.locked:
            app.toggle_lock()
        app.root.update()
        app._redraw_border()
        unlocked = app.border_canvas.find_all()
        self.assertTrue(unlocked, "解锁时外框线没有画出来")
        self.assertNotEqual(
            app.border_canvas.itemcget(unlocked[0], "dash"), "", "解锁时应为虚线"
        )

        app.toggle_lock()
        app.root.update()
        app._redraw_border()
        locked = app.border_canvas.find_all()
        self.assertTrue(locked, "锁定时外框线没有画出来")
        self.assertEqual(app.border_canvas.itemcget(locked[0], "dash"), "", "锁定时应为实线")

        # 锁定后拖动无效，而且窗口上不该有任何文字提示
        app.root.update()
        before = (app.root.winfo_x(), app.root.winfo_y())
        app._on_press(_Event(300, 300))
        app._on_drag(_Event(400, 400))
        app.root.update()
        self.assertEqual((app.root.winfo_x(), app.root.winfo_y()), before)

    def test_lock_indicator_can_be_disabled(self):
        app = self.app
        app.cfg.display.lock_indicator = "none"
        app.root.update()
        app._redraw_border()
        self.assertEqual(app.border_canvas.find_all(), ())

    # ------------------------------------------------------------------ 热重载
    def test_reload_picks_up_config_change(self):
        app = self.app
        patch_toml(
            self.cfg_path,
            {"time.target": "2026-06-01T10:00:00", "time.offset": "1h"},
        )
        app.reload_config(force=True)
        self.assertEqual(app.target, datetime(2026, 6, 1, 10, 0))
        self.assertEqual(app.mark, datetime(2026, 6, 1, 11, 0))

        app._render(datetime(2026, 6, 1, 9, 59, 57))
        self.assertEqual(app.main_label.cget("text"), "T-01:00:03")  # 距离 11:00:00
        self.assertEqual(app.sub_label.cget("text"), "T-00:00:03")  # 距离 10:00:00

    def test_reload_keeps_running_on_bad_config(self):
        app = self.app
        good = self.cfg_path.read_text(encoding="utf-8")
        self.cfg_path.write_text(good.replace(BASE_TARGET, "明天早上"), encoding="utf-8")
        app.reload_config(force=True)  # 不该抛异常
        self.assertIn("配置有误", app.last_error)
        self.assertEqual(app.target, datetime(2026, 6, 1, 9, 30))  # 保持原值
        self.cfg_path.write_text(good, encoding="utf-8")
        app.reload_config(force=True)

    # -------------------------------------------------------- 第三行：绿底镂空
    @unittest.skipUnless(
        macos_probe is not None and macos_probe.AVAILABLE, "只在 macOS 上做像素检查"
    )
    def test_info_line_is_knockout(self):
        """第三行必须是「实心绿底 + 字镂空」，笔画处直接透出桌面。"""
        app = self.app
        app._render(datetime.now())   # 先把第三行文字填上
        app.root.update()
        self.assertIn(app.cfg.display.info_style, ("auto", "knockout"))
        self.assertTrue(app._knockout_by_style())
        self.assertIsNotNone(app._knockout, "原生叠层没挂上")
        self.assertEqual(app.info_label.cget("fg"), app.bg, "占位 label 不该自己画字")

        got = macos_probe.window_pixels(WINDOW_TITLE)
        self.assertIsNotNone(got, "拿不到窗口位图")
        pixel, width, _height = got
        scale = width / app.root.winfo_width()
        label = app.info_label
        x0 = int((label.winfo_rootx() - app.root.winfo_rootx()) * scale)
        y0 = int((label.winfo_rooty() - app.root.winfo_rooty()) * scale)
        w = int(label.winfo_width() * scale)
        h = int(label.winfo_height() * scale)

        green = hole = 0
        for y in range(y0, y0 + h):
            for x in range(x0, x0 + w):
                p = pixel(x, y)
                if p[3] > 200 and p[1] > 150 and p[0] < 120:
                    green += 1
                elif p[3] <= 8:
                    hole += 1
        total = w * h
        self.assertGreater(total, 100)
        self.assertGreater(green / total, 0.5, "绿底没有铺满整行")
        self.assertGreater(hole, total * 0.03, "没有镂空：字没从绿块里挖出来")
        self.assertLess(hole / total, 0.6, "镂空过多，字不成形")

    def test_info_style_text_fallback(self):
        """切回 text 样式时应该恢复成普通绿字、并收起原生叠层。"""
        app = self.app
        app.cfg.display.info_style = "text"
        app._update_info_style()
        self.assertEqual(app.info_label.cget("fg"), app.cfg.display.color)

    # ------------------------------------------------------------------ 提醒
    def test_notifier_armed_for_both_moments(self):
        app = self.app
        moments = dict(app.notifier._moments)
        self.assertEqual(moments["偏移时刻"], datetime(2026, 6, 1, 9, 25))
        self.assertEqual(moments["目标时间点"], datetime(2026, 6, 1, 9, 30))


class TestCrossPlatform(unittest.TestCase):
    """换掉 sys.platform 在子进程里跑一遍：非 macOS 上不能因为导入或调用就炸。"""

    _IMPORTS = "\n".join(
        (
            "import float_clock.timefmt",
            "import float_clock.config",
            "import float_clock.notifier",
            "import float_clock.knockout",
            "import float_clock.macos_overlay as mo",
            "import float_clock.tclenv",
            "import float_clock.notify as nf",
        )
    )

    # 标准库里有按 sys.platform 分支的模块（比如 subprocess 会 import _winapi），
    # 必须先在真实平台上把它们加载好，再去改 sys.platform
    _PRELOAD = (
        "import base64, ctypes, ctypes.util, os, shutil, subprocess, sys, threading, tomllib"
    )

    def _run(self, platform, extra=""):
        code = (
            self._PRELOAD
            + "\nsys.platform = %r\n" % platform
            + self._IMPORTS
            + "\n"
            + extra
        )
        return subprocess.run(
            [sys.executable, "-c", code], capture_output=True, text=True, timeout=90
        )

    def test_linux_falls_back_cleanly(self):
        result = self._run(
            "linux",
            "assert mo.available() is False\n"
            "v = mo.KnockoutView('x')\n"
            "assert v.attach() is False\n"
            "assert v.show_png(b'', 0, 0, 1, 1) is False\n"
            "print(nf.backend())\n",
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("notify-send", result.stdout)

    def test_windows_falls_back_cleanly(self):
        result = self._run(
            "win32",
            "assert mo.available() is False\n"
            "assert mo.KnockoutView('x').attach() is False\n"
            "print(nf.backend())\n",
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("Toast", result.stdout)

    @unittest.skipUnless(sys.platform == "darwin", "本机就是 macOS")
    def test_darwin_reports_a_real_backend(self):
        result = self._run("darwin", "print(nf.backend())\n")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue(
            "osascript" in result.stdout or "terminal-notifier" in result.stdout,
            result.stdout,
        )


if __name__ == "__main__":
    unittest.main()
