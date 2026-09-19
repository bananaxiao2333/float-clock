"""Build the "green block + knocked-out glyphs" bitmap (pure PIL, no window system).

Tk cannot draw an image with an alpha channel on a transparent window, and it
cannot punch holes in one either, so the shape of this line is worked out here
and handed to `macos_overlay.KnockoutView` to be drawn with AppKit.
"""

from __future__ import annotations

from pathlib import Path

__all__ = ["find_font_file", "calibrate_px_size", "render_knockout", "available"]

# Monospace font file candidates: (lowercase Tk family name, regular file, bold file,
# face index inside the .ttc)
_FONT_TABLE: dict[str, tuple[tuple[str, int], tuple[str, int]]] = {
    "menlo": (
        ("/System/Library/Fonts/Menlo.ttc", 0),
        ("/System/Library/Fonts/Menlo.ttc", 1),
    ),
    "sf mono": (
        ("/System/Library/Fonts/SFNSMono.ttf", 0),
        ("/System/Library/Fonts/SFNSMono.ttf", 0),
    ),
    "monaco": (
        ("/System/Library/Fonts/Monaco.ttf", 0),
        ("/System/Library/Fonts/Monaco.ttf", 0),
    ),
    "courier new": (
        ("/System/Library/Fonts/Supplemental/Courier New.ttf", 0),
        ("/System/Library/Fonts/Supplemental/Courier New Bold.ttf", 0),
    ),
    "consolas": (
        ("C:/Windows/Fonts/consola.ttf", 0),
        ("C:/Windows/Fonts/consolab.ttf", 0),
    ),
    "cascadia mono": (
        ("C:/Windows/Fonts/CascadiaMono.ttf", 0),
        ("C:/Windows/Fonts/CascadiaMono-Bold.ttf", 0),
    ),
    "dejavu sans mono": (
        ("/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf", 0),
        ("/usr/share/fonts/truetype/dejavu/DejaVuSansMono-Bold.ttf", 0),
    ),
    "liberation mono": (
        ("/usr/share/fonts/truetype/liberation/LiberationMono-Regular.ttf", 0),
        ("/usr/share/fonts/truetype/liberation/LiberationMono-Bold.ttf", 0),
    ),
    "noto sans mono": (
        ("/usr/share/fonts/truetype/noto/NotoSansMono-Regular.ttf", 0),
        ("/usr/share/fonts/truetype/noto/NotoSansMono-Bold.ttf", 0),
    ),
    "ubuntu mono": (
        ("/usr/share/fonts/truetype/ubuntu/UbuntuMono-R.ttf", 0),
        ("/usr/share/fonts/truetype/ubuntu/UbuntuMono-B.ttf", 0),
    ),
}

_PREFERRED_ORDER = (
    "menlo",
    "sf mono",
    "monaco",
    "consolas",
    "cascadia mono",
    "dejavu sans mono",
    "liberation mono",
    "noto sans mono",
    "courier new",
)


def _pil():
    from PIL import Image  # noqa: F401

    return Image


def available() -> bool:
    """Whether Pillow is available."""
    try:
        _pil()
        return True
    except Exception:  # pragma: no cover
        return False


def find_font_file(family: str, bold: bool = True) -> tuple[str, int] | None:
    """Look up the font file for a Tk family name; with no match, take the first monospace
    font that exists, in preference order.
    """
    if not available():
        return None
    key = (family or "").strip().lower()
    candidates: list[str] = []
    if key in _FONT_TABLE:
        candidates.append(key)
    candidates += [name for name in _PREFERRED_ORDER if name != key]

    for name in candidates:
        regular, bold_face = _FONT_TABLE[name]
        path, index = bold_face if bold else regular
        if Path(path).is_file():
            return path, index
    return None


def calibrate_px_size(wanted_pixel_height: float, font_path: str, font_index: int) -> int:
    """Convert the wanted pixel height into a PIL font size."""
    from PIL import ImageFont

    reference = ImageFont.truetype(font_path, 100, index=font_index)
    ascent, descent = reference.getmetrics()
    if ascent + descent <= 0:
        return max(6, int(round(wanted_pixel_height)))
    return max(6, int(round(wanted_pixel_height * 100.0 / (ascent + descent))))


def render_knockout(
    text: str,
    font_path: str,
    font_index: int,
    px_size: int,
    size_px: tuple[int, int],
    color: tuple[int, int, int],
):
    """Green block with knocked-out glyphs: fill the whole block, then set alpha=0
    wherever the glyph strokes are.
    """
    from PIL import Image, ImageDraw, ImageFont

    width, height = max(1, int(size_px[0])), max(1, int(size_px[1]))
    image = Image.new("RGBA", (width, height), (*color, 255))
    mask = Image.new("L", (width, height), 0)
    font = ImageFont.truetype(font_path, px_size, index=font_index)
    ImageDraw.Draw(mask).text(
        (width / 2.0, height / 2.0), text, font=font, fill=255, anchor="mm"
    )
    image.putalpha(Image.eval(mask, lambda value: 255 - value))
    return image


def hex_to_rgb(value: str) -> tuple[int, int, int]:
    text = (value or "").strip().lstrip("#")
    if len(text) == 3:
        text = "".join(char * 2 for char in text)
    if len(text) != 6:
        return (0, 255, 102)
    try:
        return tuple(int(text[i : i + 2], 16) for i in (0, 2, 4))  # type: ignore[return-value]
    except ValueError:
        return (0, 255, 102)
