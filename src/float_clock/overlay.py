"""The borderless, transparent T± countdown window.

Interaction:
* the left mouse button drags the window around
* a double-click opens the settings window
* the right mouse button locks / unlocks it (frame: solid = locked, dashed = draggable)
* the middle button / Ctrl+right-click / Cmd+right-click opens the menu
* Ctrl+L locks, Ctrl+, opens the settings, Ctrl+R reloads the config, Ctrl+Q quits

macOS note: `overrideredirect` / `-transparent` must be set **before the window is
mapped**, and the window must then be mapped in one go with `deiconify()`,
otherwise the system puts the title bar back and the transparency stops working.
This module therefore always follows the order withdraw -> configure -> build ->
geometry -> deiconify.
"""

from __future__ import annotations

import sys
import tkinter as tk
from datetime import datetime, timedelta
from tkinter import font as tkfont
from tkinter import messagebox

from . import knockout, macos_overlay, notify
from .config import Config, load_config, patch_toml
from .notifier import MomentNotifier
from .tclenv import ensure_tcl_library
from .timefmt import format_hms, parse_duration, parse_target, render_info, split_delta

__all__ = ["FloatingClock", "pick_mono_family"]

# Tcl inside a venv cannot find the tcl8.6/tk8.6 data directories, so fix that before Tk()
ensure_tcl_library()

# A macOS regression in Tk 9: systemTransparent is painted as opaque black, which
# turns the overlay into a black box with smeared text
def macos_transparency_supported() -> bool:
    """Whether `systemTransparent` is really transparent on macOS (it is not in Tk 9.0)."""
    if sys.platform != "darwin":
        return True
    return float(tk.TkVersion) < 9.0

MONO_CANDIDATES = (
    "Menlo",
    "SF Mono",
    "Monaco",
    "JetBrains Mono",
    "Fira Code",
    "Cascadia Mono",
    "Consolas",
    "DejaVu Sans Mono",
    "Liberation Mono",
    "Noto Sans Mono",
    "Ubuntu Mono",
    "Courier New",
)

_MOMENT_LABELS = {"main": "Offset moment", "sub": "Target time"}


def pick_mono_family(root: tk.Misc) -> str:
    """Pick a monospace font that really exists on this system."""
    try:
        available = set(tkfont.families(root))
    except tk.TclError:  # pragma: no cover - platform dependent
        available = set()
    for name in MONO_CANDIDATES:
        if name in available:
            return name
    return "TkFixedFont"


class FloatingClock:
    def __init__(self, config: Config, *, open_settings: bool = False) -> None:
        self.cfg = config
        self.locked = bool(config.window.locked)

        now = datetime.now()
        self.target = parse_target(config.time.target, now)
        self.offset = parse_duration(config.time.offset)
        self.mark = self.target + timedelta(seconds=self.offset)

        self.notifier = MomentNotifier(config.notify)
        self.diagnostics: dict[str, str] = {}

        self._main_text = ""
        self._sub_text = ""
        self._info_text = ""
        self.last_error = ""
        self._notified_error = ""
        self._font_face: tuple[str, int] | None = None
        self._knockout = None
        self._knockout_state = None
        self._knockout_png = b""
        self._window_shown = False
        self._drag_origin: tuple[int, int, int, int] | None = None
        self._settings: tk.Toplevel | None = None
        self._tick_count = 0
        self._config_mtime = self._current_mtime()
        self._closed = False

        # The key step: keep the window hidden and map it in one go once everything is configured
        self.root = tk.Tk()
        self.root.withdraw()
        self.root.title("FloatClock")

        self.bg = self._configure_window()
        self._build_widgets()
        self._bind_events()

        self._apply_display()
        self.root.update_idletasks()
        self._show()
        self._setup_knockout()

        self._arm(now)
        self.root.after(int(self.cfg.display.interval_ms), self._tick)
        if open_settings:
            self.root.after(300, self.open_settings)

    # ---------------------------------------------------------------- window
    def _configure_window(self) -> str:
        """Create the borderless, always-on-top, transparent window. Must be called before
        the window is mapped.
        """
        window = self.cfg.window

        if window.borderless:
            try:
                self.root.overrideredirect(True)
                self.diagnostics["borderless"] = "set"
            except tk.TclError as exc:  # pragma: no cover
                self.diagnostics["borderless"] = f"failed to set it: {exc}"
        else:
            self.diagnostics["borderless"] = "switched off in the config"

        back_color = ""
        if sys.platform == "darwin":
            try:
                self.root.configure(bg="systemTransparent")
                self.root.wm_attributes("-transparent", True)
                back_color = "systemTransparent"
                self.diagnostics["transparent background"] = (
                    "macOS -transparent + systemTransparent"
                )
                if not macos_transparency_supported():
                    self.diagnostics["transparent background"] += (
                        f" (warning: transparency is broken in Tk {tk.TkVersion}"
                        " and turns into a black box)"
                    )
            except tk.TclError as exc:
                self.diagnostics["transparent background"] = f"failed: {exc}"
        elif sys.platform.startswith("win"):
            key = "#010203"
            try:
                self.root.configure(bg=key)
                self.root.wm_attributes("-transparentcolor", key)
                back_color = key
                self.diagnostics["transparent background"] = (
                    "Windows -transparentcolor chroma key"
                )
            except tk.TclError as exc:
                self.diagnostics["transparent background"] = f"failed: {exc}"
        else:
            self.diagnostics["transparent background"] = (
                "not supported on this platform, using a solid background instead"
            )

        if not back_color:
            back_color = self.cfg.display.x11_background
            self.diagnostics["transparent background"] = (
                "falling back to the solid background " + back_color
            )

        self.root.configure(bg=back_color)
        try:
            self.root.wm_attributes("-topmost", bool(window.topmost))
            self.diagnostics["topmost"] = "on" if window.topmost else "off"
        except tk.TclError as exc:  # pragma: no cover
            self.diagnostics["topmost"] = f"failed: {exc}"
        if window.opacity < 1.0:
            try:
                self.root.wm_attributes("-alpha", max(0.05, float(window.opacity)))
                self.diagnostics["opacity"] = f"{window.opacity}"
            except tk.TclError:  # pragma: no cover
                pass
        return back_color

    def _show(self) -> None:
        """Map the window and then assert the styling once more (on some systems deiconify
        brings the title bar back).
        """
        self.root.geometry(f"+{int(self.cfg.window.x)}+{int(self.cfg.window.y)}")
        self.root.update_idletasks()
        self.root.deiconify()
        self.root.lift()
        if self.cfg.window.borderless:
            try:
                self.root.overrideredirect(True)
            except tk.TclError:  # pragma: no cover
                pass
        if self.bg == "systemTransparent":
            try:
                self.root.wm_attributes("-transparent", True)
            except tk.TclError:  # pragma: no cover
                pass
        if self.cfg.window.topmost:
            try:
                self.root.wm_attributes("-topmost", True)
            except tk.TclError:  # pragma: no cover
                pass
        self.root.geometry(f"+{int(self.cfg.window.x)}+{int(self.cfg.window.y)}")
        self.root.update_idletasks()
        self._window_shown = True
        self._redraw_border()

    def window_report(self) -> str:
        """Window state report used by --diagnose and for troubleshooting."""
        root = self.root
        geom = root.wm_geometry()
        frame_x, frame_y = (int(v) for v in geom.split("+", 1)[1].split("+"))
        titlebar = root.winfo_rooty() - frame_y
        lines = [
            f"platform        : {sys.platform}  Tk {tk.TkVersion} / Tcl {tk.TclVersion}",
            f"window geometry : {geom}   content origin : "
            f"{root.winfo_rootx()},{root.winfo_rooty()}",
            f"title bar height: {titlebar} px  "
            f"{'✅ borderless is in effect' if titlebar <= 1 else '❌ the title bar is still there'}",
            f"overrideredirect: {root.overrideredirect()}",
            f"transparent bg  : {self.bg}"
            f" (-transparent = {root.wm_attributes('-transparent')})",
            f"topmost         : {root.wm_attributes('-topmost')}",
            f"font            : {self.family}  main {self.main_font.cget('size')}"
            f" / sub {self.sub_font.cget('size')} / info {self.info_font.cget('size')}"
            f"  weight={self.main_font.actual('weight')}",
            f"main title      : {self.main_label.cget('text')}",
            f"subtitle        : {self.sub_label.cget('text')}",
            f"third line      : {self.info_label.cget('text')}",
            f"lock state      : "
            f"{'locked (solid frame)' if self.locked else 'unlocked (dashed frame)'}",
            f"notify via      : {notify.backend()}",
        ]
        for key, value in self.diagnostics.items():
            lines.append(f"{key:<16s}: {value}")
        return "\n".join(lines)

    def _build_widgets(self) -> None:
        display = self.cfg.display
        self.family = display.font_family or pick_mono_family(self.root)
        weight = "bold" if display.bold else "normal"
        self.main_font = tkfont.Font(
            root=self.root, family=self.family, size=int(display.main_size), weight=weight
        )
        self.sub_font = tkfont.Font(
            root=self.root, family=self.family, size=int(display.sub_size), weight=weight
        )
        self.info_font = tkfont.Font(
            root=self.root, family=self.family, size=int(display.info_size), weight=weight
        )

        # The frame canvas sits at the very bottom, with the content Frame stacked on top
        self.border_canvas = tk.Canvas(
            self.root, bg=self.bg, highlightthickness=0, bd=0, takefocus=0
        )
        self.border_canvas.place(x=0, y=0, relwidth=1, relheight=1)

        self.frame = tk.Frame(self.root, bg=self.bg, bd=0, highlightthickness=0)
        self.frame.pack(padx=self._border_inset(), pady=self._border_inset())

        self.main_label = tk.Label(
            self.frame,
            text="T-00:00:00",
            font=self.main_font,
            fg=display.color,
            bg=self.bg,
            bd=0,
            highlightthickness=0,
        )
        self.main_label.pack(anchor="center")

        self.sub_label = tk.Label(
            self.frame,
            text="T-00:00:00",
            font=self.sub_font,
            fg=display.sub_color or display.color,
            bg=self.bg,
            bd=0,
            highlightthickness=0,
        )
        self.sub_label.pack(anchor="center", pady=(int(display.gap), 0))

        # Third line: target time plus offset (green text on a transparent background,
        # exactly like the two lines above)
        self.info_label = tk.Label(
            self.frame,
            text=" ",
            font=self.info_font,
            fg=display.info_color or display.color,
            bg=self.bg,
            bd=0,
            highlightthickness=0,
        )
        self.info_label.pack(anchor="center", pady=(int(display.gap), 0))

        self.menu = tk.Menu(self.root, tearoff=0)
        self.menu.add_command(label="Lock / Unlock", command=self.toggle_lock)
        self.menu.add_command(label="Settings…", command=self.open_settings)
        self.menu.add_command(
            label="Reload config", command=lambda: self.reload_config(force=True)
        )
        self.menu.add_separator()
        self.menu.add_command(label="Quit", command=self.quit)

        self.frame.bind("<Configure>", lambda _e: self._redraw_border())

    def _bind_events(self) -> None:
        widgets = (
            self.root,
            self.border_canvas,
            self.frame,
            self.main_label,
            self.sub_label,
            self.info_label,
        )
        for widget in widgets:
            widget.bind("<Button-1>", self._on_press)
            widget.bind("<B1-Motion>", self._on_drag)
            widget.bind("<ButtonRelease-1>", self._on_release)
            widget.bind("<Double-Button-1>", self._on_double_click)
            widget.bind("<Button-3>", self._on_right_click)
            widget.bind("<Button-2>", self._on_menu)
            widget.bind("<Control-Button-3>", self._on_menu)
            for sequence in ("<Command-Button-3>", "<Mod1-Button-3>"):
                try:
                    widget.bind(sequence, self._on_menu)
                except tk.TclError:  # pragma: no cover - this platform has no such modifier
                    pass

        self.root.bind("<Control-l>", lambda _e: self.toggle_lock())
        self.root.bind("<Control-L>", lambda _e: self.toggle_lock())
        self.root.bind("<Control-comma>", lambda _e: self.open_settings())
        self.root.bind("<Control-r>", lambda _e: self.reload_config(force=True))
        self.root.bind("<Control-q>", lambda _e: self.quit())
        self.root.protocol("WM_DELETE_WINDOW", self.quit)

    # ------------------------------------------------------- lock indicator frame
    def _border_inset(self) -> int:
        display = self.cfg.display
        if display.lock_indicator != "border":
            return 0
        return max(0, int(display.border_width)) + 2

    def _redraw_border(self) -> None:
        """The lock state is not announced in text; the solid / dashed frame shows it instead."""
        display = self.cfg.display
        canvas = self.border_canvas
        canvas.delete("all")
        width = int(display.border_width)
        if display.lock_indicator != "border" or width <= 0:
            return

        w = self.root.winfo_width()
        h = self.root.winfo_height()
        if w <= 1 or h <= 1:
            self.root.update_idletasks()
            w, h = self.root.winfo_width(), self.root.winfo_height()
        if w <= 1 or h <= 1:
            return

        solid = self.locked == bool(display.solid_when_locked)
        inset = width / 2.0
        canvas.create_rectangle(
            inset,
            inset,
            w - inset,
            h - inset,
            outline=display.border_color or display.color,
            width=width,
            dash=() if solid else (4, 3),
        )

    # ------------------------------------------------------------- mouse input
    def _on_press(self, event: tk.Event) -> None:
        self.root.update_idletasks()
        self._drag_origin = (
            event.x_root,
            event.y_root,
            self.root.winfo_x(),
            self.root.winfo_y(),
        )

    def _on_drag(self, event: tk.Event) -> None:
        if self._drag_origin is None:
            return
        if self.locked:
            return  # the solid frame already says "locked", so no text popup
        start_x, start_y, win_x, win_y = self._drag_origin
        x = win_x + (event.x_root - start_x)
        y = win_y + (event.y_root - start_y)
        screen_w = self.root.winfo_screenwidth()
        screen_h = self.root.winfo_screenheight()
        x = max(-self.root.winfo_width() + 48, min(x, screen_w - 48))
        y = max(0, min(y, screen_h - 40))
        self.root.geometry(f"+{int(x)}+{int(y)}")

    def _on_release(self, _event: tk.Event) -> None:
        self._drag_origin = None
        self._persist_position()

    def _on_right_click(self, _event: tk.Event) -> None:
        self.toggle_lock()

    def _on_double_click(self, _event: tk.Event) -> None:
        self.open_settings()

    def _on_menu(self, event: tk.Event) -> None:
        self.menu.entryconfigure(0, label="Unlock" if self.locked else "Lock")
        try:
            self.menu.tk_popup(event.x_root, event.y_root)
        finally:
            self.menu.grab_release()

    # ------------------------------------------------------- config and state
    def _persist_position(self) -> None:
        x = self.root.winfo_x()
        y = self.root.winfo_y()
        if (x, y) == (self.cfg.window.x, self.cfg.window.y):
            return
        try:
            patch_toml(self.cfg.path, {"window.x": x, "window.y": y})
        except OSError as exc:
            self._report_error(f"could not save the position: {exc}")
            return
        self.cfg.window.x = x
        self.cfg.window.y = y
        self._config_mtime = self._current_mtime()

    def _persist_lock(self) -> None:
        try:
            patch_toml(self.cfg.path, {"window.locked": self.locked})
        except OSError as exc:
            self._report_error(f"could not save the lock state: {exc}")
            return
        self.cfg.window.locked = self.locked
        self._config_mtime = self._current_mtime()

    def _current_mtime(self) -> int:
        try:
            return self.cfg.path.stat().st_mtime_ns
        except OSError:
            return 0

    def reload_config(self, *, force: bool = False) -> None:
        path = self.cfg.path
        if not force and not path.exists():
            return
        try:
            new = load_config(path)
            target = parse_target(new.time.target, datetime.now())
            offset = parse_duration(new.time.offset)
        except Exception as exc:  # noqa: BLE001 - a user input problem: just report it
            self._report_error(f"bad configuration: {exc}")
            self._config_mtime = self._current_mtime()
            return

        mtime = self._current_mtime()
        self.last_error = ""
        self._notified_error = ""
        old_cfg, self.cfg = self.cfg, new
        self._config_mtime = mtime

        self.target = target
        self.offset = offset
        self.mark = target + timedelta(seconds=offset)
        self.locked = bool(new.window.locked)

        if (new.window.x, new.window.y) != (old_cfg.window.x, old_cfg.window.y):
            self.root.geometry(f"+{int(new.window.x)}+{int(new.window.y)}")

        self._apply_display()
        try:
            self.root.wm_attributes("-topmost", bool(new.window.topmost))
        except tk.TclError:  # pragma: no cover
            pass
        self._apply_lock_visual()
        self._arm(datetime.now())

    def _apply_display(self) -> None:
        display = self.cfg.display
        family = display.font_family or pick_mono_family(self.root)
        weight = "bold" if display.bold else "normal"
        self.main_font.configure(family=family, size=int(display.main_size), weight=weight)
        self.sub_font.configure(family=family, size=int(display.sub_size), weight=weight)
        self.info_font.configure(family=family, size=int(display.info_size), weight=weight)
        self.main_label.configure(fg=display.color)
        self.sub_label.configure(fg=display.sub_color or display.color)
        self.sub_label.pack_configure(pady=(int(display.gap), 0))
        self.frame.pack_configure(padx=self._border_inset(), pady=self._border_inset())
        self._main_text = ""
        self._sub_text = ""
        self._info_text = ""
        self._knockout_state = None
        if display.info_style != "text":
            self._setup_knockout()
        self._update_info_style()
        self._apply_lock_visual()

    # ------------------------------------------------------------ lock state
    def toggle_lock(self) -> None:
        self.locked = not self.locked
        self._apply_lock_visual()
        self._persist_lock()

    def _apply_lock_visual(self) -> None:
        """The frame alone distinguishes the lock state: solid = locked, dashed = draggable."""
        self._redraw_border()

    # ------------------------------------------------------------- reminders
    def _arm(self, now: datetime) -> None:
        self.notifier.config = self.cfg.notify
        self.notifier.arm(
            [
                (_MOMENT_LABELS["main"], self.mark),
                (_MOMENT_LABELS["sub"], self.target),
            ],
            now,
        )

    # ------------------------------------------------------------- rendering
    def _report_error(self, message: str) -> None:
        """The window no longer has a line for errors, so one goes to stderr plus a system
        notification, and every distinct message is announced only once.
        """
        self.last_error = message
        print(f"[float-clock] {message}", file=sys.stderr)
        if message != self._notified_error:
            self._notified_error = message
            notify.send_async("FloatClock error", message)

    @staticmethod
    def _compose(template: str, sign: str, clock: str) -> str:
        return template.replace("{sign}", sign).replace("{clock}", clock)

    def _render(self, now: datetime) -> None:
        display = self.cfg.display
        show_days = bool(display.show_days)

        sign, secs = split_delta((self.mark - now).total_seconds())
        main_text = self._compose(display.main_template, sign, format_hms(secs, show_days))
        if main_text != self._main_text:
            self.main_label.configure(text=main_text)
            self._main_text = main_text

        sign, secs = split_delta((self.target - now).total_seconds())
        sub_text = self._compose(display.sub_template, sign, format_hms(secs, show_days))
        if sub_text != self._sub_text:
            self.sub_label.configure(text=sub_text)
            self._sub_text = sub_text

        info_text = self.info_line_text()
        if info_text != self._info_text:
            self.info_label.configure(text=info_text)
            self._info_text = info_text
            self._knockout_state = None
        self._update_info_style()

    # -------------------------------------------------------- third line content
    def info_line_text(self) -> str:
        """Third line: target time plus offset. An empty template makes the whole line take
        up no space at all.
        """
        template = self.cfg.display.info_template
        if not template.strip():
            return " "
        return render_info(
            template,
            self.target,
            self.mark,
            self.offset,
            self.cfg.display.show_days,
        )

    # --------------------------------------------- third line: green knockout block
    def _knockout_wanted(self) -> bool:
        """Whether the third line should be drawn by the native overlay (in auto mode it is
        only enabled on platforms that support it).
        """
        style = self.cfg.display.info_style
        if style == "text":
            return False
        if not macos_overlay.available() or not knockout.available():
            return False
        return bool(self.cfg.display.info_template.strip())

    def _knockout_by_style(self) -> bool:
        return self.cfg.display.info_style != "text" and self._knockout is not None

    def _setup_knockout(self) -> None:
        """Attach the native overlay. Idempotent: if it is already attached and the font has
        not changed, return straight away.
        """
        # Before the window is mapped the NSWindow may not exist yet, so attach after _show()
        if not self._window_shown or not self._knockout_wanted():
            return
        bold = bool(self.cfg.display.bold)
        if self._knockout is not None and self._font_face is not None:
            family = self.cfg.display.font_family or self.family
            if self._font_face == knockout.find_font_file(family, bold):
                return
        face = knockout.find_font_file(self.cfg.display.font_family or self.family, bold)
        if face is None:
            self._report_error(
                "no usable monospace font file found, falling back to plain green text"
                " for the third line"
            )
            return
        if self._knockout is None:
            view = macos_overlay.KnockoutView(self.root.title())
            if not view.attach():
                self._report_error(
                    "could not attach the native overlay to the window"
                    f" ({view.last_error or 'unknown reason'}); the third line falls"
                    " back to plain green text"
                )
                return
            self._knockout = view
        self._font_face = face
        self._knockout_state = None

    def _update_info_style(self) -> None:
        """Decide who draws the third line: the native overlay (green block, knocked out) or
        plain green text.
        """
        by_knockout = self._knockout_by_style() and self._font_face is not None
        if by_knockout:
            self.info_label.configure(fg=self.bg)  # placeholder: the native overlay paints
            if self._info_text.strip():
                self._place_knockout(self._info_text)
            else:
                self._knockout.hide()
            return
        self.info_label.configure(
            fg=self.cfg.display.info_color or self.cfg.display.color
        )
        if self._knockout is not None:
            self._knockout.hide()

    def _place_knockout(self, text: str) -> None:
        label = self.info_label
        self.root.update_idletasks()
        width, height = label.winfo_width(), label.winfo_height()
        if width <= 1 or height <= 1:
            return
        x = label.winfo_rootx() - self.root.winfo_rootx()
        y = label.winfo_rooty() - self.root.winfo_rooty()
        color = knockout.hex_to_rgb(
            self.cfg.display.info_color or self.cfg.display.color
        )
        scale = self._knockout.scale
        state = (text, width, height, x, y, color, scale)
        if state == self._knockout_state and not self._knockout.hidden:
            return
        face_path, face_index = self._font_face
        px_size = knockout.calibrate_px_size(
            self.info_font.metrics("linespace") * scale, face_path, face_index
        )
        image = knockout.render_knockout(
            text,
            face_path,
            face_index,
            px_size,
            (int(round(width * scale)), int(round(height * scale))),
            color,
        )
        self._knockout_png = macos_overlay.encode_png(image)
        if self._knockout.show_png(self._knockout_png, x, y, width, height):
            self._knockout_state = state

    # --------------------------------------------------------------- main loop
    def _tick(self) -> None:
        if self._closed:
            return
        now = datetime.now()
        self._render(now)
        self.notifier.tick(now)

        self._tick_count += 1
        if self._tick_count % 5 == 0 and self._current_mtime() != self._config_mtime:
            self.reload_config()

        self.root.after(max(50, int(self.cfg.display.interval_ms)), self._tick)

    def run(self) -> int:
        self.root.mainloop()
        return 0

    def quit(self) -> None:
        self._closed = True
        try:
            self.root.destroy()
        except tk.TclError:  # pragma: no cover
            pass

    # --------------------------------------------------------------- settings
    def open_settings(self) -> None:
        if self._settings is not None and self._settings.winfo_exists():
            self._settings.lift()
            self._settings.focus_force()
            return

        win = tk.Toplevel(self.root)
        self._settings = win
        win.title("FloatClock settings")
        win.configure(bg="#1c1c1e", padx=16, pady=14)
        win.resizable(False, False)
        win.attributes("-topmost", True)

        rows = [
            ("target", "Target time", self.cfg.time.target),
            ("offset", "Offset", self.cfg.time.offset),
            ("color", "Colour", self.cfg.display.color),
            ("main_size", "Title size", str(self.cfg.display.main_size)),
            ("sub_size", "Subtitle size", str(self.cfg.display.sub_size)),
        ]
        entries: dict[str, tk.Entry] = {}
        for index, (key, label, value) in enumerate(rows):
            tk.Label(win, text=label, bg="#1c1c1e", fg="#f2f2f7").grid(
                row=index, column=0, sticky="w", pady=4
            )
            entry = tk.Entry(
                win,
                width=30,
                bg="#2c2c2e",
                fg="#ffffff",
                insertbackground="#ffffff",
                relief="flat",
            )
            entry.insert(0, value)
            entry.grid(row=index, column=1, padx=(12, 0), pady=4)
            entries[key] = entry

        tk.Label(
            win,
            text="Changes are written back into config.toml (comments kept), effective at once",
            bg="#1c1c1e",
            fg="#8e8e93",
        ).grid(row=len(rows), column=0, columnspan=2, sticky="w", pady=(8, 10))

        def apply() -> None:
            try:
                parse_target(entries["target"].get(), datetime.now())
                parse_duration(entries["offset"].get())
                main_size = int(entries["main_size"].get())
                sub_size = int(entries["sub_size"].get())
                color = entries["color"].get().strip() or "#00FF66"
            except Exception as exc:  # noqa: BLE001
                messagebox.showerror("Invalid input", str(exc), parent=win)
                return
            updates = {
                "time.target": entries["target"].get().strip(),
                "time.offset": entries["offset"].get().strip(),
                "display.color": color,
                "display.main_size": main_size,
                "display.sub_size": sub_size,
            }
            try:
                patch_toml(self.cfg.path, updates)
            except OSError as exc:
                messagebox.showerror("Cannot write the config", str(exc), parent=win)
                return
            self.reload_config(force=True)
            win.destroy()
            self._settings = None

        buttons = tk.Frame(win, bg="#1c1c1e")
        buttons.grid(row=len(rows) + 1, column=0, columnspan=2, sticky="e")
        tk.Button(buttons, text="Apply", command=apply, width=8).pack(side="left", padx=4)
        tk.Button(buttons, text="Cancel", command=win.destroy, width=8).pack(side="left")

        win.bind("<Return>", lambda _e: apply())
        win.bind("<Escape>", lambda _e: win.destroy())
        win.focus_force()
