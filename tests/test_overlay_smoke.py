"""GUI smoke test: really create a Tk window and check the transparent background,
dragging, the lock write-back and zero-padded rendering.

The whole group is skipped when there is no graphics environment (no DISPLAY, no
window server).
Run: ``uv run python -m unittest discover -s tests -v``
"""

from __future__ import annotations

import subprocess
import sys
import tempfile
import unittest
from datetime import datetime
from pathlib import Path

from float_clock.tclenv import ensure_tcl_library

# Inside a venv the Tcl/Tk data directories have to be pointed at first, or Tk() will not start
ensure_tcl_library()

try:
    import tkinter as tk

    _root_probe = tk.Tk()
    _root_probe.withdraw()
    _root_probe.destroy()
    TK_OK = True
    TK_REASON = ""
except Exception as exc:  # pragma: no cover - no graphics environment
    TK_OK = False
    TK_REASON = f"no usable Tk graphics environment: {exc}"

if TK_OK:
    from float_clock import notify as _notify

    # Do not pop up real system notifications during the tests
    _notify.send_async = lambda *args, **kwargs: None

    from float_clock.config import load_config, patch_toml, write_default_config
    from float_clock.overlay import FloatingClock

    try:
        import macos_probe
    except Exception:  # pragma: no cover
        macos_probe = None

# Baseline: target time 2026-06-01 09:30:00, offset -5 minutes -> offset moment 09:25:00
BASE_TARGET = "2026-06-01T09:30:00"
BASE_OFFSET = "-00:05:00"
WINDOW_TITLE = "FloatClock"


class _Event:
    """A fake Tk mouse event."""

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
    def setUpClass_extra(cls):  # pragma: no cover - placeholder
        pass

    def setUp(self) -> None:
        """Every test starts from the baseline so they cannot contaminate each other."""
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

    # ----------------------------------------------------------- transparency
    def test_background_is_transparent(self):
        app = self.app
        if sys.platform == "darwin":
            self.assertEqual(app.bg, "systemTransparent")
            self.assertTrue(bool(app.root.wm_attributes("-transparent")))
        # Every widget uses the same "transparent background" colour; no solid block may appear
        for widget in (app.frame, app.main_label, app.sub_label, app.info_label):
            self.assertEqual(str(widget.cget("bg")), app.bg)
        self.assertEqual(str(app.root.cget("bg")), app.bg)

    @unittest.skipUnless(
        macos_probe is not None and macos_probe.AVAILABLE,
        "pixel-level transparency is only checked on macOS",
    )
    def test_background_pixels_are_really_transparent(self):
        """The core regression test.

        The macOS backend of Tk 9.0 paints `systemTransparent` as opaque black (the
        overlay turns into a black box with smeared text), so the alpha of the pixels
        the window actually rendered is read back here and a black background is
        caught immediately.
        """
        app = self.app
        app.root.update()
        pixels = macos_probe.sample_window(WINDOW_TITLE)
        self.assertIsNotNone(pixels, "could not find the window bitmap")
        assert pixels is not None

        self.assertFalse(pixels.is_opaque, "the NSWindow is still opaque")
        self.assertEqual(
            pixels.background_alpha,
            0,
            f"the most common alpha in the window is not 0"
            f" (corner_rgba={pixels.corner_rgba})"
            f" - Tk {tk.TkVersion} painted systemTransparent as an opaque colour",
        )
        self.assertGreater(
            pixels.transparent_ratio,
            0.5,
            f"only {pixels.transparent_ratio:.0%} of the pixels are transparent, so the whole background got painted over",
        )
        # The green text must still be drawn (do not overdo the transparency and lose it)
        self.assertLess(pixels.transparent_ratio, 0.99, "the window is fully transparent: no text was drawn")

    def test_three_lines(self):
        """The window is exactly three lines: main title, subtitle, target time + offset."""
        app = self.app
        app.root.update()
        self.assertEqual(
            list(app.frame.winfo_children()),
            [app.main_label, app.sub_label, app.info_label],
        )
        for label in (app.main_label, app.sub_label, app.info_label):
            self.assertEqual(label.pack_info()["side"], "top")
            self.assertEqual(str(label.cget("bg")), app.bg, "all three lines need a transparent background")
        # The first two lines are plain green text; the third line is drawn by the native
        # overlay (the label only reserves the space and draws nothing itself)
        self.assertEqual(app.main_label.cget("fg"), "#00FF66")
        self.assertEqual(app.sub_label.cget("fg"), "#00FF66")

    def test_info_line_shows_timepoint_and_delta(self):
        """The third line shows the target time plus the offset, follows the config, and
        can be switched off.
        """
        app = self.app
        app._render(datetime(2026, 6, 1, 9, 29, 55))
        self.assertEqual(app.info_label.cget("text"), "09:30:00 | -00:05:00")

        app.cfg.display.info_template = "{datetime} ({delta_human}) -> {mark}"
        app._render(datetime(2026, 6, 1, 9, 29, 55))
        self.assertEqual(
            app.info_label.cget("text"), "2026-06-01 09:30:00 (-5m) -> 09:25:00"
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
        """Regression test: the window must be configured before it is mapped, otherwise
        macOS puts the title bar back.
        """
        app = self.app
        app.root.update()
        geometry = app.root.wm_geometry()
        frame_x, frame_y = (int(v) for v in geometry.split("+", 1)[1].split("+"))
        title_bar = app.root.winfo_rooty() - frame_y
        self.assertLessEqual(title_bar, 1, f"the window still has a {title_bar}px title bar: {geometry}")
        self.assertEqual(app.root.winfo_rootx(), frame_x)

    # ----------------------------------------------------------- appearance
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

        # Monospace: the digits and the colon must all be the same width, otherwise the
        # text jitters left and right with every second that ticks by
        reference = app.main_font.measure("0")
        for char in "0123456789:":
            self.assertEqual(app.main_font.measure(char), reference, f"{char!r} is not monospaced")

    # ------------------------------------------------------------ rendering
    def test_main_title_counts_down_to_offset_moment(self):
        app = self.app
        app._render(datetime(2026, 6, 1, 9, 20, 0))
        self.assertEqual(app.main_label.cget("text"), "T-00:05:00")  # 5 minutes to 09:25:00
        self.assertEqual(app.sub_label.cget("text"), "T-00:10:00")  # 10 minutes to 09:30:00

        app._render(datetime(2026, 6, 1, 9, 25, 0))
        self.assertEqual(app.main_label.cget("text"), "T-00:00:00")  # exactly on the offset moment
        self.assertEqual(app.sub_label.cget("text"), "T-00:05:00")

        app._render(datetime(2026, 6, 1, 9, 30, 0))
        self.assertEqual(app.main_label.cget("text"), "T+00:05:00")  # offset moment 5 minutes ago
        self.assertEqual(app.sub_label.cget("text"), "T-00:00:00")  # exactly on the target time

        app._render(datetime(2026, 6, 1, 9, 30, 7))
        self.assertEqual(app.main_label.cget("text"), "T+00:05:07")
        self.assertEqual(app.sub_label.cget("text"), "T+00:00:07")

    def test_zero_padding_and_day_rollover(self):
        app = self.app
        app._render(datetime(2026, 5, 30, 8, 24, 57))
        self.assertEqual(app.sub_label.cget("text"), "T-02:01:05:03")  # DD:HH:MM:SS, all zero-padded

        app.cfg.display.show_days = False
        app._render(datetime(2026, 5, 30, 8, 24, 57))
        self.assertEqual(app.sub_label.cget("text"), "T-49:05:03")  # hours do not carry into days

    # ------------------------------------------------------------- dragging
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

    # -------------------------------------------------------------- locking
    def test_right_click_locks_then_unlocks(self):
        app = self.app

        app._on_right_click(_Event(0, 0))
        self.assertTrue(app.locked)
        self.assertTrue(load_config(self.cfg_path).window.locked)

        # Dragging does nothing while locked
        app.root.update()
        before = (app.root.winfo_x(), app.root.winfo_y())
        app._on_press(_Event(300, 300))
        app._on_drag(_Event(400, 400))
        app.root.update()
        self.assertEqual((app.root.winfo_x(), app.root.winfo_y()), before)

        app._on_right_click(_Event(0, 0))
        self.assertFalse(app.locked)
        self.assertFalse(load_config(self.cfg_path).window.locked)

        # Dragging works again after unlocking
        app._on_press(_Event(300, 300))
        base = (app.root.winfo_x(), app.root.winfo_y())
        app._on_drag(_Event(330, 320))
        app.root.update()
        self.assertEqual((app.root.winfo_x(), app.root.winfo_y()), (base[0] + 30, base[1] + 20))

    def test_lock_indicator_is_border_not_text(self):
        """The lock state is shown by the frame alone: unlocked = dashed, locked = solid, and
        no text hint pops up.
        """
        app = self.app
        app.root.update()

        if app.locked:
            app.toggle_lock()
        app.root.update()
        app._redraw_border()
        unlocked = app.border_canvas.find_all()
        self.assertTrue(unlocked, "no frame was drawn while unlocked")
        self.assertNotEqual(
            app.border_canvas.itemcget(unlocked[0], "dash"), "", "it should be dashed while unlocked"
        )

        app.toggle_lock()
        app.root.update()
        app._redraw_border()
        locked = app.border_canvas.find_all()
        self.assertTrue(locked, "no frame was drawn while locked")
        self.assertEqual(app.border_canvas.itemcget(locked[0], "dash"), "", "it should be solid while locked")

        # Dragging does nothing while locked, and no text hint may appear in the window
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

    # ----------------------------------------------------------- hot reload
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
        self.assertEqual(app.main_label.cget("text"), "T-01:00:03")  # to the offset moment 11:00:00
        self.assertEqual(app.sub_label.cget("text"), "T-00:00:03")  # to the target time 10:00:00

    def test_reload_keeps_running_on_bad_config(self):
        app = self.app
        good = self.cfg_path.read_text(encoding="utf-8")
        self.cfg_path.write_text(good.replace(BASE_TARGET, "tomorrow morning"), encoding="utf-8")
        app.reload_config(force=True)  # must not raise
        self.assertIn("bad configuration", app.last_error)
        self.assertEqual(app.target, datetime(2026, 6, 1, 9, 30))  # keeps the previous value
        self.cfg_path.write_text(good, encoding="utf-8")
        app.reload_config(force=True)

    # --------------------------------------- third line: green knockout block
    @unittest.skipUnless(
        macos_probe is not None and macos_probe.AVAILABLE, "pixel checks only run on macOS"
    )
    def test_info_line_is_knockout(self):
        """The third line must be a solid green block with the glyphs knocked out, so the
        desktop shows through the strokes.
        """
        app = self.app
        app._render(datetime.now())   # fill the third line with text first
        app.root.update()
        self.assertIn(app.cfg.display.info_style, ("auto", "knockout"))
        self.assertTrue(app._knockout_by_style())
        self.assertIsNotNone(app._knockout, "the native overlay was not attached")
        self.assertEqual(app.info_label.cget("fg"), app.bg, "the placeholder label must not draw text itself")

        got = macos_probe.window_pixels(WINDOW_TITLE)
        self.assertIsNotNone(got, "could not get the window bitmap")
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
        self.assertGreater(green / total, 0.5, "the green block does not fill the whole line")
        self.assertGreater(hole, total * 0.03, "no knockout: the glyphs were not punched out of the block")
        self.assertLess(hole / total, 0.6, "too much knockout: the glyphs are not legible any more")

    def test_info_style_text_fallback(self):
        """Switching back to the text style must restore plain green text and put the native
        overlay away.
        """
        app = self.app
        app.cfg.display.info_style = "text"
        app._update_info_style()
        self.assertEqual(app.info_label.cget("fg"), app.cfg.display.color)

    # ------------------------------------------------------------ reminders
    def test_notifier_armed_for_both_moments(self):
        app = self.app
        moments = dict(app.notifier._moments)
        self.assertEqual(moments["Offset moment"], datetime(2026, 6, 1, 9, 25))
        self.assertEqual(moments["Target time"], datetime(2026, 6, 1, 9, 30))


class TestCrossPlatform(unittest.TestCase):
    """Run everything once in a subprocess with sys.platform swapped: off macOS nothing may
    blow up just from importing or calling into it.
    """

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

    # Some standard library modules branch on sys.platform (subprocess imports _winapi,
    # for instance), so they have to be loaded on the real platform first and only then
    # may sys.platform be changed
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

    @unittest.skipUnless(sys.platform == "darwin", "this machine is macOS")
    def test_darwin_reports_a_real_backend(self):
        result = self._run("darwin", "print(nf.backend())\n")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue(
            "osascript" in result.stdout or "terminal-notifier" in result.stdout,
            result.stdout,
        )


if __name__ == "__main__":
    unittest.main()
