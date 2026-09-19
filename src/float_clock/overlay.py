"""无背景悬浮 T± 倒计时窗口。

交互：
* 左键拖动移动
* 双击 打开设置窗口
* 右键 锁定 / 解锁（外框线：实线=锁定，虚线=可拖动）
* 中键 / Ctrl+右键 / Cmd+右键 打开菜单
* Ctrl+L 锁定，Ctrl+, 设置，Ctrl+R 重载配置，Ctrl+Q 退出

macOS 注意：`overrideredirect` / `-transparent` 必须在窗口**映射之前**设置，
并且最后用 `deiconify()` 一次性映射，否则系统会重新套上标题栏、透明也会失效。
所以本模块统一走 withdraw → 配置 → 构建 → geometry → deiconify 的顺序。
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

# venv 里 Tcl 找不到 tcl8.6/tk8.6 数据目录，必须在 Tk() 之前修好
ensure_tcl_library()

# Tk 9 的 macOS 回归：systemTransparent 被画成不透明黑，浮窗会变成黑底 + 拖影
def macos_transparency_supported() -> bool:
    """macOS 上 `systemTransparent` 能否真的透明（Tk 9.0 不行）。"""
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

_MOMENT_LABELS = {"main": "偏移时刻", "sub": "目标时间点"}


def pick_mono_family(root: tk.Misc) -> str:
    """挑一个系统里真实存在的等宽字体。"""
    try:
        available = set(tkfont.families(root))
    except tk.TclError:  # pragma: no cover - 平台相关
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

        # 关键：窗口先藏起来，等一切配置好再一次性映射
        self.root = tk.Tk()
        self.root.withdraw()
        self.root.title("FloatClock 悬浮倒计时")

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

    # ------------------------------------------------------------------ 窗口
    def _configure_window(self) -> str:
        """建立无边框置顶透明窗口。必须在窗口映射前调用。"""
        window = self.cfg.window

        if window.borderless:
            try:
                self.root.overrideredirect(True)
                self.diagnostics["无边框"] = "已设置"
            except tk.TclError as exc:  # pragma: no cover
                self.diagnostics["无边框"] = f"设置失败：{exc}"
        else:
            self.diagnostics["无边框"] = "配置里关闭了"

        back_color = ""
        if sys.platform == "darwin":
            try:
                self.root.configure(bg="systemTransparent")
                self.root.wm_attributes("-transparent", True)
                back_color = "systemTransparent"
                self.diagnostics["透明背景"] = "macOS -transparent + systemTransparent"
                if not macos_transparency_supported():
                    self.diagnostics["透明背景"] += (
                        f"（⚠︎ Tk {tk.TkVersion} 的透明有 bug，会变成黑底）"
                    )
            except tk.TclError as exc:
                self.diagnostics["透明背景"] = f"失败：{exc}"
        elif sys.platform.startswith("win"):
            key = "#010203"
            try:
                self.root.configure(bg=key)
                self.root.wm_attributes("-transparentcolor", key)
                back_color = key
                self.diagnostics["透明背景"] = "Windows -transparentcolor 抠色"
            except tk.TclError as exc:
                self.diagnostics["透明背景"] = f"失败：{exc}"
        else:
            self.diagnostics["透明背景"] = "当前平台不支持，用底色代替"

        if not back_color:
            back_color = self.cfg.display.x11_background
            self.diagnostics["透明背景"] = "回退为实心底色 " + back_color

        self.root.configure(bg=back_color)
        try:
            self.root.wm_attributes("-topmost", bool(window.topmost))
            self.diagnostics["置顶"] = "开" if window.topmost else "关"
        except tk.TclError as exc:  # pragma: no cover
            self.diagnostics["置顶"] = f"失败：{exc}"
        if window.opacity < 1.0:
            try:
                self.root.wm_attributes("-alpha", max(0.05, float(window.opacity)))
                self.diagnostics["不透明度"] = f"{window.opacity}"
            except tk.TclError:  # pragma: no cover
                pass
        return back_color

    def _show(self) -> None:
        """映射窗口，并再确认一次样式（有些系统的 deiconify 会把标题栏带回来）。"""
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
        """给 --diagnose / 排障用的窗口状态报告。"""
        root = self.root
        geom = root.wm_geometry()
        frame_x, frame_y = (int(v) for v in geom.split("+", 1)[1].split("+"))
        titlebar = root.winfo_rooty() - frame_y
        lines = [
            f"平台            : {sys.platform}  Tk {tk.TkVersion} / Tcl {tk.TclVersion}",
            f"窗口 geometry   : {geom}   内容原点 : {root.winfo_rootx()},{root.winfo_rooty()}",
            f"标题栏高度      : {titlebar} px  {'✅ 无边框生效' if titlebar <= 1 else '❌ 仍有标题栏'}",
            f"overrideredirect: {root.overrideredirect()}",
            f"透明背景        : {self.bg}"
            f"（-transparent = {root.wm_attributes('-transparent')}）",
            f"置顶            : {root.wm_attributes('-topmost')}",
            f"字体            : {self.family}  主 {self.main_font.cget('size')}"
            f" / 副 {self.sub_font.cget('size')} / 三 {self.info_font.cget('size')}"
            f"  weight={self.main_font.actual('weight')}",
            f"主标题          : {self.main_label.cget('text')}",
            f"副标题          : {self.sub_label.cget('text')}",
            f"第三行          : {self.info_label.cget('text')}",
            f"锁定状态        : {'锁定（实线框）' if self.locked else '解锁（虚线框）'}",
            f"通知后端        : {notify.backend()}",
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

        # 外框线画布在最底层，内容 Frame 叠在它上面
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

        # 第三行：目标时间点 + 偏移（绿字、透明底，和前两行同款）
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
        self.menu.add_command(label="锁定 / 解锁", command=self.toggle_lock)
        self.menu.add_command(label="设置…", command=self.open_settings)
        self.menu.add_command(label="重载配置", command=lambda: self.reload_config(force=True))
        self.menu.add_separator()
        self.menu.add_command(label="退出", command=self.quit)

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
                except tk.TclError:  # pragma: no cover - 该平台不支持这个修饰键
                    pass

        self.root.bind("<Control-l>", lambda _e: self.toggle_lock())
        self.root.bind("<Control-L>", lambda _e: self.toggle_lock())
        self.root.bind("<Control-comma>", lambda _e: self.open_settings())
        self.root.bind("<Control-r>", lambda _e: self.reload_config(force=True))
        self.root.bind("<Control-q>", lambda _e: self.quit())
        self.root.protocol("WM_DELETE_WINDOW", self.quit)

    # ------------------------------------------------------------ 外框锁定指示
    def _border_inset(self) -> int:
        display = self.cfg.display
        if display.lock_indicator != "border":
            return 0
        return max(0, int(display.border_width)) + 2

    def _redraw_border(self) -> None:
        """锁定状态不用文字提示，改用外框线的实线 / 虚线表示。"""
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

    # -------------------------------------------------------------- 鼠标交互
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
            return  # 锁定状态由外框实线表示，不再弹文字
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
        self.menu.entryconfigure(0, label="解锁" if self.locked else "锁定")
        try:
            self.menu.tk_popup(event.x_root, event.y_root)
        finally:
            self.menu.grab_release()

    # ------------------------------------------------------------ 配置与状态
    def _persist_position(self) -> None:
        x = self.root.winfo_x()
        y = self.root.winfo_y()
        if (x, y) == (self.cfg.window.x, self.cfg.window.y):
            return
        try:
            patch_toml(self.cfg.path, {"window.x": x, "window.y": y})
        except OSError as exc:
            self._report_error(f"位置保存失败：{exc}")
            return
        self.cfg.window.x = x
        self.cfg.window.y = y
        self._config_mtime = self._current_mtime()

    def _persist_lock(self) -> None:
        try:
            patch_toml(self.cfg.path, {"window.locked": self.locked})
        except OSError as exc:
            self._report_error(f"锁定状态保存失败：{exc}")
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
        except Exception as exc:  # noqa: BLE001 - 用户输入问题，直接提示
            self._report_error(f"配置有误：{exc}")
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

    # -------------------------------------------------------------- 锁定状态
    def toggle_lock(self) -> None:
        self.locked = not self.locked
        self._apply_lock_visual()
        self._persist_lock()

    def _apply_lock_visual(self) -> None:
        """锁定只用外框线区分：实线=锁定，虚线=可拖动。"""
        self._redraw_border()

    # ------------------------------------------------------------------ 提醒
    def _arm(self, now: datetime) -> None:
        self.notifier.config = self.cfg.notify
        self.notifier.arm(
            [
                (_MOMENT_LABELS["main"], self.mark),
                (_MOMENT_LABELS["sub"], self.target),
            ],
            now,
        )

    # ------------------------------------------------------------------ 渲染
    def _report_error(self, message: str) -> None:
        """窗口上不再有第三行，出错走 stderr + 系统通知，同一条只提醒一次。"""
        self.last_error = message
        print(f"[float-clock] {message}", file=sys.stderr)
        if message != self._notified_error:
            self._notified_error = message
            notify.send_async("FloatClock 出错", message)

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

    # -------------------------------------------------------------- 第三行内容
    def info_line_text(self) -> str:
        """第三行：目标时间点 + 偏移。留空模板则整行不占位。"""
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

    # ------------------------------------------------- 第三行：绿底 + 镂空字
    def _knockout_wanted(self) -> bool:
        """要不要用原生叠层画第三行（auto 时只在支持的平台上启用）。"""
        style = self.cfg.display.info_style
        if style == "text":
            return False
        if not macos_overlay.available() or not knockout.available():
            return False
        return bool(self.cfg.display.info_template.strip())

    def _knockout_by_style(self) -> bool:
        return self.cfg.display.info_style != "text" and self._knockout is not None

    def _setup_knockout(self) -> None:
        """挂载原生叠层。幂等：已经挂好且字体没变就直接返回。"""
        # 窗口还没映射时 NSWindow 可能还不存在，等 _show() 之后再挂
        if not self._window_shown or not self._knockout_wanted():
            return
        bold = bool(self.cfg.display.bold)
        if self._knockout is not None and self._font_face is not None:
            family = self.cfg.display.font_family or self.family
            if self._font_face == knockout.find_font_file(family, bold):
                return
        face = knockout.find_font_file(self.cfg.display.font_family or self.family, bold)
        if face is None:
            self._report_error("找不到可用的等宽字体文件，第三行回退为普通绿字")
            return
        if self._knockout is None:
            view = macos_overlay.KnockoutView(self.root.title())
            if not view.attach():
                self._report_error(
                    f"无法在窗口上挂载原生叠层（{view.last_error or '未知原因'}），"
                    "第三行回退为普通绿字"
                )
                return
            self._knockout = view
        self._font_face = face
        self._knockout_state = None

    def _update_info_style(self) -> None:
        """决定第三行由谁画：原生叠层（绿底镂空）还是普通绿字。"""
        by_knockout = self._knockout_by_style() and self._font_face is not None
        if by_knockout:
            self.info_label.configure(fg=self.bg)  # 只占位，真正画的是原生叠层
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

    # ------------------------------------------------------------------ 主循环
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

    # ------------------------------------------------------------------ 设置
    def open_settings(self) -> None:
        if self._settings is not None and self._settings.winfo_exists():
            self._settings.lift()
            self._settings.focus_force()
            return

        win = tk.Toplevel(self.root)
        self._settings = win
        win.title("FloatClock 设置")
        win.configure(bg="#1c1c1e", padx=16, pady=14)
        win.resizable(False, False)
        win.attributes("-topmost", True)

        rows = [
            ("target", "目标时间点", self.cfg.time.target),
            ("offset", "偏移", self.cfg.time.offset),
            ("color", "绿色", self.cfg.display.color),
            ("main_size", "主标题字号", str(self.cfg.display.main_size)),
            ("sub_size", "副标题字号", str(self.cfg.display.sub_size)),
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
            text="改动会写回 config.toml（保留注释），立刻生效",
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
                messagebox.showerror("输入有误", str(exc), parent=win)
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
                messagebox.showerror("无法写入配置", str(exc), parent=win)
                return
            self.reload_config(force=True)
            win.destroy()
            self._settings = None

        buttons = tk.Frame(win, bg="#1c1c1e")
        buttons.grid(row=len(rows) + 1, column=0, columnspan=2, sticky="e")
        tk.Button(buttons, text="应用", command=apply, width=8).pack(side="left", padx=4)
        tk.Button(buttons, text="取消", command=win.destroy, width=8).pack(side="left")

        win.bind("<Return>", lambda _e: apply())
        win.bind("<Escape>", lambda _e: win.destroy())
        win.focus_force()
