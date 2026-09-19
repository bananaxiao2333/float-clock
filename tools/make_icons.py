#!/usr/bin/env python3
"""生成 FloatClock 的图标（PNG / ICNS / ICO）。

设计上刻意和浮窗本身对齐：近黑的圆角方块 + 亮绿(#00FF66)的倒计时表盘。
表盘缺口留在右上角，暗示「正在倒数」；中间是等宽的 `T−`，也就是主标题的前缀。

用法（需要 Pillow）：

    python3 tools/make_icons.py

产物都落在 assets/ 下：

    icon.png      1024×1024，Linux / 文档用
    icon.ico      Windows 多尺寸
    icon.icns     macOS（只有 macOS 上能生成，靠系统的 iconutil）
"""

from __future__ import annotations

import shutil
import subprocess
import sys
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

# 和 [display] color 一致
GREEN = (0, 255, 102, 255)
# 和 [display] background 一致，稍微偏绿一点点
DARK = (13, 16, 13, 255)
CANVAS = 1024
# macOS Big Sur 之后的图标栅格：内容占 824×824，居中留白
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


def draw_icon(size: int = CANVAS) -> Image.Image:
    # 先在 4× 上画再缩回来，抗锯齿才干净（PIL 的超采样老办法）
    ss = 4
    big = size * ss
    margin = MARGIN * ss
    radius = RADIUS * ss

    image = Image.new("RGBA", (big, big), (0, 0, 0, 0))
    draw = ImageDraw.Draw(image)

    # 底座：近黑圆角方块
    draw.rounded_rectangle(
        (margin, margin, big - margin - 1, big - margin - 1),
        radius=radius,
        fill=DARK,
    )
    # 内描边，让方块从深色壁纸上也能看出边界
    stroke = max(1, int(2.5 * ss))
    draw.rounded_rectangle(
        (margin, margin, big - margin - 1, big - margin - 1),
        radius=radius,
        outline=(0, 255, 102, 38),
        width=stroke,
    )

    # 表盘：留 1/4 缺口在右上，暗示还在倒数
    center = big / 2
    ring_r = 268 * ss
    ring_w = 58 * ss
    box = (center - ring_r, center - ring_r, center + ring_r, center + ring_r)
    draw.arc(box, start=-58, end=270, fill=GREEN, width=ring_w)

    # 缺口两端各点一个圆头，边缘才不秃
    import math

    for angle in (-58, 270):
        x = center + ring_r * math.cos(math.radians(angle))
        y = center + ring_r * math.sin(math.radians(angle))
        draw.ellipse(
            (x - ring_w / 2, y - ring_w / 2, x + ring_w / 2, y + ring_w / 2),
            fill=GREEN,
        )

    # 中间的 `T−`：主标题的前缀，等宽粗体
    font = load_mono(int(300 * ss))
    text = "T−"
    left, top, right, bottom = draw.textbbox((0, 0), text, font=font)
    draw.text(
        (center - (right - left) / 2 - left, center - (bottom - top) / 2 - top),
        text,
        font=font,
        fill=GREEN,
    )

    image = image.resize((size, size), Image.LANCZOS)
    image.putalpha(rounded_mask(size, MARGIN, RADIUS))
    return image


def write_ico(image: Image.Image, path: Path) -> None:
    sizes = [(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)]
    image.resize((256, 256), Image.LANCZOS).save(
        path, format="ICO", sizes=sizes
    )


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
    print(f"  → {assets / 'icon.png'}")

    image.resize((512, 512), Image.LANCZOS).save(assets / "icon-512.png")
    print(f"  → {assets / 'icon-512.png'}")

    write_ico(image, assets / "icon.ico")
    print(f"  → {assets / 'icon.ico'}")

    if write_icns(image, assets / "icon.icns", tmp):
        print(f"  → {assets / 'icon.icns'}")
    else:
        print("  （这台机器没有 iconutil，跳过 .icns）", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
