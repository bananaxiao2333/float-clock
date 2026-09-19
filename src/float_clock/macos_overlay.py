"""在 Tk 窗口上叠一个原生 AppKit 子视图，用来画「绿底 + 镂空字」。

为什么需要这个：Tk 8.6 在 `-transparent` 窗口上**不会绘制任何图片**——`tk.Label(image=...)`
和 `canvas.create_image(...)` 都实测为整片透明（同一张带 alpha 的 PNG 在不透明窗口里
完全正常）。而「背景绿、字镂空」必须在位图里带 alpha 通道，纯 Tk 的 `fg=systemTransparent`
也挖不出洞（实测像素数与 `fg=黑色` 逐点相同，即空操作）。

于是把这一行的位图交给 AppKit：`NSWindow.contentView` 上加一个 `NSImageView` 子视图，
由它绘制带 alpha 的 PNG；子视图会比 Tk 自己的绘制更靠上层，镂空处直接透出桌面。

非 macOS 或任何一步失败时 `available()` 返回 False，调用方回退成普通绿字。
"""

from __future__ import annotations

import ctypes
import ctypes.util
import sys

__all__ = ["available", "KnockoutView"]

_AVAILABLE = sys.platform == "darwin"


class _NSPoint(ctypes.Structure):
    _fields_ = [("x", ctypes.c_double), ("y", ctypes.c_double)]


class _NSSize(ctypes.Structure):
    _fields_ = [("width", ctypes.c_double), ("height", ctypes.c_double)]


class _NSRect(ctypes.Structure):
    _fields_ = [("origin", _NSPoint), ("size", _NSSize)]


if _AVAILABLE:
    try:
        _objc = ctypes.CDLL(ctypes.util.find_library("objc"))
        _objc.sel_registerName.restype = ctypes.c_void_p
        _objc.sel_registerName.argtypes = [ctypes.c_char_p]
        _objc.objc_getClass.restype = ctypes.c_void_p
        _objc.objc_getClass.argtypes = [ctypes.c_char_p]
        _objc.objc_allocateClassPair.restype = ctypes.c_void_p
        _objc.objc_allocateClassPair.argtypes = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_size_t]
        _objc.objc_registerClassPair.argtypes = [ctypes.c_void_p]
        _objc.class_addMethod.restype = ctypes.c_bool
        _objc.class_addMethod.argtypes = [
            ctypes.c_void_p,
            ctypes.c_void_p,
            ctypes.c_void_p,
            ctypes.c_char_p,
        ]
        _MSG = _objc.objc_msgSend
    except Exception:  # pragma: no cover - 没有 ObjC 运行时
        _AVAILABLE = False


def _sel(name: str) -> int:
    return _objc.sel_registerName(name.encode())


def _cls(name: str) -> int:
    return _objc.objc_getClass(name.encode())


def _ptr(receiver, name: str, *extra, argtypes=()):
    _MSG.restype = ctypes.c_void_p
    _MSG.argtypes = [ctypes.c_void_p, ctypes.c_void_p, *argtypes]
    return _MSG(ctypes.c_void_p(receiver), ctypes.c_void_p(_sel(name)), *extra)


def _void(receiver, name: str, *extra, argtypes=()):
    _MSG.restype = None
    _MSG.argtypes = [ctypes.c_void_p, ctypes.c_void_p, *argtypes]
    _MSG(ctypes.c_void_p(receiver), ctypes.c_void_p(_sel(name)), *extra)


def _double(receiver, name: str) -> float:
    _MSG.restype = ctypes.c_double
    _MSG.argtypes = [ctypes.c_void_p, ctypes.c_void_p]
    return _MSG(ctypes.c_void_p(receiver), ctypes.c_void_p(_sel(name)))


def _bool(receiver, name: str) -> bool:
    _MSG.restype = ctypes.c_bool
    _MSG.argtypes = [ctypes.c_void_p, ctypes.c_void_p]
    return _MSG(ctypes.c_void_p(receiver), ctypes.c_void_p(_sel(name)))


def _ulong(receiver, name: str) -> int:
    _MSG.restype = ctypes.c_ulong
    _MSG.argtypes = [ctypes.c_void_p, ctypes.c_void_p]
    return _MSG(ctypes.c_void_p(receiver), ctypes.c_void_p(_sel(name)))


def _string(receiver, name: str) -> str:
    obj = _ptr(receiver, name)
    if not obj:
        return ""
    _MSG.restype = ctypes.c_char_p
    _MSG.argtypes = [ctypes.c_void_p, ctypes.c_void_p]
    raw = _MSG(ctypes.c_void_p(obj), ctypes.c_void_p(_sel("UTF8String")))
    return raw.decode("utf-8", "replace") if raw else ""


# 让子视图对鼠标完全透明，否则会挡住这一行的拖动
_HIT_TEST_IMP = ctypes.CFUNCTYPE(ctypes.c_void_p, ctypes.c_void_p, ctypes.c_void_p, _NSPoint)


def _passthrough_class():
    """NSImageView 子类，hitTest: 永远返回 nil（鼠标事件穿透给 Tk）。"""
    subclass = _objc.objc_allocateClassPair(_cls("NSImageView"), b"FloatClockImageView", 0)
    if not subclass:
        return _cls("NSImageView")
    imp = _HIT_TEST_IMP(lambda _self, _cmd, _point: None)
    _objc.class_addMethod(
        ctypes.c_void_p(subclass),
        ctypes.c_void_p(_sel("hitTest:")),
        ctypes.cast(imp, ctypes.c_void_p),
        b"@:@",
    )
    _objc.objc_registerClassPair(ctypes.c_void_p(subclass))
    _PASSTHROUGH_IMP.append(imp)  # 必须保活，否则回调被回收会崩
    return subclass


_PASSTHROUGH_IMP: list = []


def available() -> bool:
    return _AVAILABLE


class KnockoutView:
    """一个原生图片子视图；只在 macOS 上真正干活，其它平台是空实现。"""

    def __init__(self, window_title: str) -> None:
        self.window_title = window_title
        self._view = None
        self._window = None
        self._content = None
        self._flipped = True
        self.scale = 1.0
        self.ok = False
        self.hidden = True
        self.last_error = ""

    # ------------------------------------------------------------------ 查找
    def _find_window(self):
        app = _ptr(_cls("NSApplication"), "sharedApplication")
        windows = _ptr(app, "windows")
        for index in range(_ulong(windows, "count")):
            window = _ptr(windows, "objectAtIndex:", ctypes.c_ulong(index), argtypes=[ctypes.c_ulong])
            if window and _string(window, "title") == self.window_title:
                return window
        return None

    def attach(self) -> bool:
        """绑定到 Tk 窗口，建好子视图。"""
        if not _AVAILABLE:
            return False
        try:
            window = self._find_window()
            if not window:
                self.last_error = "找不到标题匹配的 NSWindow"
                return False
            content = _ptr(window, "contentView")
            if not content:
                self.last_error = "contentView 为空"
                return False
            self._window = window
            self._content = content
            self._flipped = _bool(content, "isFlipped")
            try:
                self.scale = float(_double(window, "backingScaleFactor")) or 1.0
            except Exception:
                self.scale = 1.0

            rect = _NSRect(_NSPoint(0.0, 0.0), _NSSize(1.0, 1.0))
            view = _ptr(
                _ptr(_passthrough_class(), "alloc"),
                "initWithFrame:",
                rect,
                argtypes=[_NSRect],
            )
            if not view:
                return False
            _void(view, "setImageFrameStyle:", ctypes.c_ulong(0), argtypes=[ctypes.c_ulong])
            _void(view, "setImageScaling:", ctypes.c_ulong(3), argtypes=[ctypes.c_ulong])
            _void(view, "setEditable:", False, argtypes=[ctypes.c_bool])
            _void(content, "addSubview:", ctypes.c_void_p(view), argtypes=[ctypes.c_void_p])
            self._view = view
            self.ok = True
            return True
        except Exception as exc:  # pragma: no cover - 任何 ObjC 异常都退回纯 Tk
            self.ok = False
            self.last_error = f"{type(exc).__name__}: {exc}"
            return False

    # ------------------------------------------------------------------ 绘制
    def hide(self) -> None:
        self.hidden = True
        if self._view:
            try:
                _void(self._view, "setHidden:", True, argtypes=[ctypes.c_bool])
            except Exception:
                pass

    def show_png(self, png: bytes, x: float, y: float, width: float, height: float) -> bool:
        """把 PNG 贴到窗口坐标 (x, y)，尺寸按点给（位图按 scale 预渲染）。"""
        if not self.ok or not self._view:
            return False
        try:
            data = _ptr(
                _cls("NSData"),
                "dataWithBytes:length:",
                ctypes.c_char_p(png),
                ctypes.c_ulong(len(png)),
                argtypes=[ctypes.c_char_p, ctypes.c_ulong],
            )
            if not data:
                return False
            image = _ptr(
                _ptr(_cls("NSImage"), "alloc"),
                "initWithData:",
                ctypes.c_void_p(data),
                argtypes=[ctypes.c_void_p],
            )
            if not image:
                return False
            _void(
                image,
                "setSize:",
                _NSSize(max(1.0, width), max(1.0, height)),
                argtypes=[_NSSize],
            )
            _void(self._view, "setImage:", ctypes.c_void_p(image), argtypes=[ctypes.c_void_p])

            # NSView 的 frame 用的是父视图坐标系；Tk 的内容视图是翻转的（原点在左上）
            top = y if self._flipped else (self._content_height() - y - height)
            _void(
                self._view,
                "setFrame:",
                _NSRect(_NSPoint(x, top), _NSSize(width, height)),
                argtypes=[_NSRect],
            )
            _void(self._view, "setHidden:", False, argtypes=[ctypes.c_bool])
            self.hidden = False
            return True
        except Exception:  # pragma: no cover
            return False

    def _content_height(self) -> float:
        _MSG.restype = _NSRect
        _MSG.argtypes = [ctypes.c_void_p, ctypes.c_void_p]
        bounds = _MSG(ctypes.c_void_p(self._content), ctypes.c_void_p(_sel("bounds")))
        return float(bounds.size.height)


def encode_png(image) -> bytes:
    """PIL 图像 → PNG 字节。"""
    import io

    buffer = io.BytesIO()
    image.save(buffer, "PNG")
    return buffer.getvalue()
