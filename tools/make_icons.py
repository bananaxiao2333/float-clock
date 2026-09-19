#!/usr/bin/env python3
"""Generate the FloatClock artwork (PNG / ICNS / ICO plus the two tray icons).

The design is deliberately the same language as the overlay itself: a near-black
rounded square with a bright green (#00FF66) countdown dial. The dial has its
notch at the top right, as if it were still counting down, and the monospace
`T-` in the middle is the prefix the main title starts with.

Usage (needs Pillow):

    python3 tools/make_icons.py

Everything lands in assets/:

    icon.png         1024x1024, for Linux and documentation
    icon-512.png     512x512, for the README
    icon.ico         multi-resolution, for Windows
    icon.icns        for macOS (only produced on macOS, via the system iconutil)
    tray-macos.png   44x44 template image (black + alpha) for the menu bar
    tray-windows.png 64x64 colour icon for the notification area
"""

from __future__ import annotations

import math
import shutil
import subprocess
import sys
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

# Matches [display] color
GREEN = (0, 255, 102, 255)
# Matches [display] background, nudged very slightly green
DARK = (13, 16, 13, 255)
CANVAS = 1024
# Apple's post-Big-Sur icon grid: content occupies 824x824, centred
MARGIN = 100
RADIUS = 186

MONO_FONTS = [
    "/System/Library/Fonts/Menlo.ttc",
    "/System/Library/Fonts/SFNSMono.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSansMono-Bold.ttf",
    "C:/Windows/Fonts/consola.ttf",
]


def load_mono(size: int) -> ImageFont.FreeTypeFont:
    for path in MONO_FONTS:
        if Path(path).exists():
            try:
                return ImageFont.truetype(path, size, index=1)  # index 1 = Bold
            except OSError:
                try:
                    return ImageFont.truetype(path, size)
                except OSError:
                    continue
    return ImageFont.load_default(size)


def rounded_mask(size: int, margin: int, radius: int) -> Image.Image:
    mask = Image.new("L", (size, size), 0)
    ImageDraw.Draw(mask).rounded_rectangle(
        (margin, margin, size - margin - 1, size - margin - 1),
        radius=radius,
        fill=255,
    )
    return mask


def draw_glyph(
    draw: ImageDraw.ImageDraw,
    center: float,
    size: int,
    colour: tuple[int, int, int, int],
    text: str = "T\u2212",
) -> None:
    """Draw the monospace `T-` mark centred on `center`."""
    font = load_mono(size)
    left, top, right, bottom = draw.textbbox((0, 0), text, font=font)
    draw.text(
        (center - (right - left) / 2 - left, center - (bottom - top) / 2 - top),
        text,
        font=font,
        fill=colour,
    )


def draw_icon(size: int = CANVAS) -> Image.Image:
    # Draw at 4x and scale back down, the usual PIL supersampling trick, so the
    # antialiasing comes out clean.
    ss = 4
    big = size * ss
    margin = MARGIN * ss
    radius = RADIUS * ss

    image = Image.new("RGBA", (big, big), (0, 0, 0, 0))
    draw = ImageDraw.Draw(image)

    # Base plate: near-black rounded square
    draw.rounded_rectangle(
        (margin, margin, big - margin - 1, big - margin - 1),
        radius=radius,
        fill=DARK,
    )
    # Inner hairline so the plate still reads against a dark wallpaper
    stroke = max(1, int(2.5 * ss))
    draw.rounded_rectangle(
        (margin, margin, big - margin - 1, big - margin - 1),
        radius=radius,
        outline=(0, 255, 102, 38),
        width=stroke,
    )

    # Dial: a quarter of it is missing at the top right, as if still counting
    center = big / 2
    ring_r = 268 * ss
    ring_w = 58 * ss
    box = (center - ring_r, center - ring_r, center + ring_r, center + ring_r)
    draw.arc(box, start=-58, end=270, fill=GREEN, width=ring_w)

    # Round caps on both ends of the arc, otherwise the cut looks blunt
    for angle in (-58, 270):
        x = center + ring_r * math.cos(math.radians(angle))
        y = center + ring_r * math.sin(math.radians(angle))
        draw.ellipse(
            (x - ring_w / 2, y - ring_w / 2, x + ring_w / 2, y + ring_w / 2),
            fill=GREEN,
        )

    draw_glyph(draw, center, int(300 * ss), GREEN)

    image = image.resize((size, size), Image.LANCZOS)
    image.putalpha(rounded_mask(size, MARGIN, RADIUS))
    return image


def draw_tray_macos() -> Image.Image:
    """44x44 (22pt @2x) macOS status-bar template image.

    A template image must be black plus an alpha channel; AppKit inverts and
    tints it automatically for light and dark menu bars. That is also why there
    is no ring here - at 22pt the dial would collapse into a smudge, so the mark
    is just the `T-` glyph, sized to the ~18pt cap height AppKit expects.
    """
    size = 44
    ss = 8
    big = size * ss
    image = Image.new("RGBA", (big, big), (0, 0, 0, 0))
    draw = ImageDraw.Draw(image)
    draw_glyph(draw, big / 2, int(big * 0.62), (0, 0, 0, 255))
    image = image.resize((size, size), Image.LANCZOS)
    return fit_to_canvas(image, size, pad_ratio=0.06)


def draw_tray_windows() -> Image.Image:
    """64x64 colour icon for the Windows notification area.

    Simplified on purpose: the dial is dropped, because at the 16px the tray
    actually renders, a ring plus a glyph turns into mush.
    """
    size = 64
    ss = 8
    big = size * ss
    image = Image.new("RGBA", (big, big), (0, 0, 0, 0))
    draw = ImageDraw.Draw(image)
    margin = int(big * 0.04)
    draw.rounded_rectangle(
        (margin, margin, big - margin - 1, big - margin - 1),
        radius=int(big * 0.22),
        fill=DARK,
    )
    draw_glyph(draw, big / 2, int(big * 0.62), GREEN)
    image = image.resize((size, size), Image.LANCZOS)
    image.putalpha(rounded_mask(size, int(size * 0.04), int(size * 0.22)))
    return image


def fit_to_canvas(image: Image.Image, size: int, pad_ratio: float) -> Image.Image:
    """Trim transparent margins, then re-centre inside a square with padding."""
    bbox = image.getbbox()
    if bbox is None:
        return image
    cropped = image.crop(bbox)
    inner = int(size * (1 - 2 * pad_ratio))
    scale = min(inner / cropped.width, inner / cropped.height)
    resized = cropped.resize(
        (max(1, round(cropped.width * scale)), max(1, round(cropped.height * scale))),
        Image.LANCZOS,
    )
    canvas = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    canvas.paste(
        resized,
        ((size - resized.width) // 2, (size - resized.height) // 2),
    )
    return canvas


def write_ico(image: Image.Image, path: Path) -> None:
    sizes = [(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)]
    image.resize((256, 256), Image.LANCZOS).save(path, format="ICO", sizes=sizes)


def write_icns(image: Image.Image, path: Path, tmp: Path) -> bool:
    if not shutil.which("iconutil"):
        return False
    iconset = tmp / "FloatClock.iconset"
    if iconset.exists():
        shutil.rmtree(iconset)
    iconset.mkdir(parents=True)
    for points in (16, 32, 128, 256, 512):
        for scale in (1, 2):
            pixels = points * scale
            if pixels > image.width:
                continue
            suffix = "" if scale == 1 else "@2x"
            name = f"icon_{points}x{points}{suffix}.png"
            image.resize((pixels, pixels), Image.LANCZOS).save(iconset / name)
    subprocess.run(
        ["iconutil", "-c", "icns", str(iconset), "-o", str(path)],
        check=True,
    )
    shutil.rmtree(iconset)
    return True


def main() -> int:
    root = Path(__file__).resolve().parent.parent
    assets = root / "assets"
    assets.mkdir(exist_ok=True)
    tmp = root / "target" / "icons"
    tmp.mkdir(parents=True, exist_ok=True)

    image = draw_icon()
    image.save(assets / "icon.png")
    print(f"  -> {assets / 'icon.png'}")

    image.resize((512, 512), Image.LANCZOS).save(assets / "icon-512.png")
    print(f"  -> {assets / 'icon-512.png'}")

    write_ico(image, assets / "icon.ico")
    print(f"  -> {assets / 'icon.ico'}")

    if write_icns(image, assets / "icon.icns", tmp):
        print(f"  -> {assets / 'icon.icns'}")
    else:
        print("  (no iconutil on this machine, skipping .icns)", file=sys.stderr)

    draw_tray_macos().save(assets / "tray-macos.png")
    print(f"  -> {assets / 'tray-macos.png'}")

    draw_tray_windows().save(assets / "tray-windows.png")
    print(f"  -> {assets / 'tray-windows.png'}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
