"""macOS 专用探针：直接读 NSWindow 的真实绘制像素，验证「背景是否真的透明」。

不依赖屏幕录制权限：用 `cacheDisplayInRect:toBitmapImageRep:` 把窗口 contentView
渲染进位图，再读 RGBA。屏幕截图被 TCC 挡住时这是唯一能拿到地面真相的办法。
"""

from __future__ import annotations

import ctypes
import ctypes.util
import sys
from dataclasses import dataclass

__all__ = ["AVAILABLE", "WindowPixels", "sample_window", "window_pixels"]

AVAILABLE = sys.platform == "darwin"

if AVAILABLE:
    import tkinter as tk

    _objc = ctypes.CDLL(ctypes.util.find_library("objc"))
    _objc.sel_registerName.restype = ctypes.c_void_p
    _objc.sel_registerName.argtypes = [ctypes.c_char_p]
    _objc.objc_getClass.restype = ctypes.c_void_p
    _objc.objc_getClass.argtypes = [ctypes.c_char_p]
    _MSG = _objc.objc_msgSend

    class _NSPoint(ctypes.Structure):
        _fields_ = [("x", ctypes.c_double), ("y", ctypes.c_double)]

    class _NSSize(ctypes.Structure):
        _fields_ = [("width", ctypes.c_double), ("height", ctypes.c_double)]

    class _NSRect(ctypes.Structure):
        _fields_ = [("origin", _NSPoint), ("size", _NSSize)]

    def _sel(name: str) -> int:
        return _objc.sel_registerName(name.encode())

    def _cls(name: str) -> int:
        return _objc.objc_getClass(name.encode())

    def _send_ptr(receiver: int, name: str, *extra):
        _MSG.restype = ctypes.c_void_p
        _MSG.argtypes = [ctypes.c_void_p, ctypes.c_void_p] + [type(a) for a in extra]
        return _MSG(ctypes.c_void_p(receiver), ctypes.c_void_p(_sel(name)), *extra)

    def _send_ulong(receiver: int, name: str) -> int:
        _MSG.restype = ctypes.c_ulong
        _MSG.argtypes = [ctypes.c_void_p, ctypes.c_void_p]
        return _MSG(ctypes.c_void_p(receiver), ctypes.c_void_p(_sel(name)))

    def _send_bool(receiver: int, name: str) -> bool:
        _MSG.restype = ctypes.c_bool
        _MSG.argtypes = [ctypes.c_void_p, ctypes.c_void_p]
        return _MSG(ctypes.c_void_p(receiver), ctypes.c_void_p(_sel(name)))

    def _send_str(receiver: int, name: str) -> str | None:
        obj = _send_ptr(receiver, name)
        if not obj:
            return None
        _MSG.restype = ctypes.c_char_p
        _MSG.argtypes = [ctypes.c_void_p, ctypes.c_void_p]
        raw = _MSG(ctypes.c_void_p(obj), ctypes.c_void_p(_sel("UTF8String")))
        return raw.decode("utf-8", "replace") if raw else None

    def _find_window(title_part: str):
        app = _send_ptr(_cls("NSApplication"), "sharedApplication")
        windows = _send_ptr(app, "windows")
        for index in range(_send_ulong(windows, "count")):
            window = _send_ptr(windows, "objectAtIndex:", ctypes.c_ulong(index))
            if not window:
                continue
            if title_part in (_send_str(window, "title") or ""):
                return window
        return None

    @dataclass
    class WindowPixels:
        width: int
        height: int
        corner_rgba: tuple[int, ...]
        transparent_ratio: float
        is_opaque: bool
        dominant_alpha: int

        @property
        def background_alpha(self) -> int:
            """整窗出现次数最多的 alpha —— 对于浮窗就该是 0（背景透明）。"""
            return self.dominant_alpha

    def window_pixels(title: str):
        """返回 (取值函数, 宽, 高)，用于逐像素检查。坐标是设备像素。"""
        window = _find_window(title)
        if not window:
            return None
        view = _send_ptr(window, "contentView")
        _MSG.restype = _NSRect
        _MSG.argtypes = [ctypes.c_void_p, ctypes.c_void_p]
        bounds = _MSG(ctypes.c_void_p(view), ctypes.c_void_p(_sel("bounds")))
        _MSG.restype = ctypes.c_void_p
        _MSG.argtypes = [ctypes.c_void_p, ctypes.c_void_p, _NSRect]
        rep = _MSG(
            ctypes.c_void_p(view),
            ctypes.c_void_p(_sel("bitmapImageRepForCachingDisplayInRect:")),
            bounds,
        )
        _MSG.restype = None
        _MSG.argtypes = [ctypes.c_void_p, ctypes.c_void_p, _NSRect, ctypes.c_void_p]
        _MSG(
            ctypes.c_void_p(view),
            ctypes.c_void_p(_sel("cacheDisplayInRect:toBitmapImageRep:")),
            bounds,
            ctypes.c_void_p(rep),
        )
        data = _send_ptr(rep, "bitmapData")
        row_bytes = _send_ulong(rep, "bytesPerRow")
        samples = _send_ulong(rep, "samplesPerPixel")
        width = _send_ulong(rep, "pixelsWide")
        height = _send_ulong(rep, "pixelsHigh")
        if not data or not width or not height:
            return None
        buffer = ctypes.cast(ctypes.c_void_p(data), ctypes.POINTER(ctypes.c_ubyte))

        def pixel(x: int, y: int) -> tuple[int, ...]:
            offset = y * row_bytes + x * samples
            return tuple(buffer[offset + i] for i in range(samples))

        return pixel, width, height

    def sample_window(title: str, *, stride: int = 4) -> WindowPixels | None:
        """把标题含 `title` 的窗口内容渲染成位图并统计透明度。"""
        window = _find_window(title)
        if not window:
            return None
        view = _send_ptr(window, "contentView")

        _MSG.restype = _NSRect
        _MSG.argtypes = [ctypes.c_void_p, ctypes.c_void_p]
        bounds = _MSG(ctypes.c_void_p(view), ctypes.c_void_p(_sel("bounds")))

        _MSG.restype = ctypes.c_void_p
        _MSG.argtypes = [ctypes.c_void_p, ctypes.c_void_p, _NSRect]
        rep = _MSG(
            ctypes.c_void_p(view),
            ctypes.c_void_p(_sel("bitmapImageRepForCachingDisplayInRect:")),
            bounds,
        )

        _MSG.restype = None
        _MSG.argtypes = [ctypes.c_void_p, ctypes.c_void_p, _NSRect, ctypes.c_void_p]
        _MSG(
            ctypes.c_void_p(view),
            ctypes.c_void_p(_sel("cacheDisplayInRect:toBitmapImageRep:")),
            bounds,
            ctypes.c_void_p(rep),
        )

        data = _send_ptr(rep, "bitmapData")
        row_bytes = _send_ulong(rep, "bytesPerRow")
        samples = _send_ulong(rep, "samplesPerPixel")
        width = _send_ulong(rep, "pixelsWide")
        height = _send_ulong(rep, "pixelsHigh")
        if not data or not width or not height:
            return None

        buffer = ctypes.cast(ctypes.c_void_p(data), ctypes.POINTER(ctypes.c_ubyte))
        total = transparent = 0
        histogram: dict[int, int] = {}
        for y in range(0, height, stride):
            for x in range(0, width, stride):
                offset = y * row_bytes + x * samples
                alpha = buffer[offset + 3]
                histogram[alpha] = histogram.get(alpha, 0) + 1
                total += 1
                if alpha <= 8:
                    transparent += 1
        offset = 2 * row_bytes + 2 * samples
        corner = tuple(buffer[offset + i] for i in range(samples))
        dominant = max(histogram, key=lambda a: histogram[a]) if histogram else 255
        return WindowPixels(
            width=width,
            height=height,
            corner_rgba=corner,
            transparent_ratio=transparent / total if total else 0.0,
            is_opaque=_send_bool(window, "isOpaque"),
            dominant_alpha=dominant,
        )

else:  # pragma: no cover - 非 macOS

    @dataclass
    class WindowPixels:  # type: ignore[no-redef]
        width: int = 0
        height: int = 0
        corner_rgba: tuple[int, ...] = ()
        transparent_ratio: float = 0.0
        is_opaque: bool = True
        dominant_alpha: int = 255

        @property
        def background_alpha(self) -> int:
            return self.dominant_alpha

    def window_pixels(title: str):
        """返回 (取值函数, 宽, 高)，用于逐像素检查。坐标是设备像素。"""
        window = _find_window(title)
        if not window:
            return None
        view = _send_ptr(window, "contentView")
        _MSG.restype = _NSRect
        _MSG.argtypes = [ctypes.c_void_p, ctypes.c_void_p]
        bounds = _MSG(ctypes.c_void_p(view), ctypes.c_void_p(_sel("bounds")))
        _MSG.restype = ctypes.c_void_p
        _MSG.argtypes = [ctypes.c_void_p, ctypes.c_void_p, _NSRect]
        rep = _MSG(
            ctypes.c_void_p(view),
            ctypes.c_void_p(_sel("bitmapImageRepForCachingDisplayInRect:")),
            bounds,
        )
        _MSG.restype = None
        _MSG.argtypes = [ctypes.c_void_p, ctypes.c_void_p, _NSRect, ctypes.c_void_p]
        _MSG(
            ctypes.c_void_p(view),
            ctypes.c_void_p(_sel("cacheDisplayInRect:toBitmapImageRep:")),
            bounds,
            ctypes.c_void_p(rep),
        )
        data = _send_ptr(rep, "bitmapData")
        row_bytes = _send_ulong(rep, "bytesPerRow")
        samples = _send_ulong(rep, "samplesPerPixel")
        width = _send_ulong(rep, "pixelsWide")
        height = _send_ulong(rep, "pixelsHigh")
        if not data or not width or not height:
            return None
        buffer = ctypes.cast(ctypes.c_void_p(data), ctypes.POINTER(ctypes.c_ubyte))

        def pixel(x: int, y: int) -> tuple[int, ...]:
            offset = y * row_bytes + x * samples
            return tuple(buffer[offset + i] for i in range(samples))

        return pixel, width, height

    def sample_window(title: str, *, stride: int = 4) -> WindowPixels | None:  # type: ignore[misc]
        return None
