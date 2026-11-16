"""Build the app icons from the square source image.

Usage: python tools/gen_icons.py [source.png]   (default: app/icons/source/emblem.png)

Writes app/icons/icon.ico (16-256 px, used for the window, taskbar, start menu and installer)
and app/icons/icon.png (32 px, the tray icon). The source must be square with a transparent
background. Every size is stepped down by halves before the final resize, and sizes up to
48 px are sharpened afterwards, so the detailed emblem does not turn to mush in the tray.
"""

import sys
from pathlib import Path

from PIL import Image, ImageFilter

ROOT = Path(__file__).resolve().parents[1]
ICONS = ROOT / "app" / "icons"
ICO_SIZES = [16, 20, 24, 32, 40, 48, 64, 96, 128, 256]
TRAY_SIZE = 32


def square(img: Image.Image) -> Image.Image:
    """Centers the visible part (alpha bounding box) on a square transparent canvas."""
    x0, y0, x1, y1 = img.getchannel("A").getbbox()
    side = max(x1 - x0, y1 - y0)
    out = Image.new("RGBA", (side, side))
    out.alpha_composite(img.crop((x0, y0, x1, y1)), ((side - (x1 - x0)) // 2, (side - (y1 - y0)) // 2))
    return out


def scaled(img: Image.Image, size: int) -> Image.Image:
    while img.width // 2 >= size * 2:
        img = img.resize((img.width // 2,) * 2, Image.LANCZOS)
    img = img.resize((size, size), Image.LANCZOS)
    if size > 48:
        return img
    strong = size <= 32
    rgb = img.convert("RGB").filter(ImageFilter.UnsharpMask(radius=0.8 if strong else 1.0, percent=160 if strong else 110, threshold=0))
    out = rgb.convert("RGBA")
    out.putalpha(img.getchannel("A"))
    return out


def main(argv: list[str]) -> int:
    src = Path(argv[0]) if argv else ICONS / "source" / "emblem.png"
    img = Image.open(src).convert("RGBA")
    if img.getchannel("A").getextrema()[0] == 255:
        print(f"{src}: no transparent background", file=sys.stderr)
        return 1
    img = square(img)
    frames = [scaled(img, s) for s in ICO_SIZES]
    frames[-1].save(ICONS / "icon.ico", format="ICO", sizes=[(s, s) for s in ICO_SIZES], append_images=frames[:-1])
    scaled(img, TRAY_SIZE).save(ICONS / "icon.png")
    print(f"wrote icon.ico ({', '.join(map(str, ICO_SIZES))} px) and icon.png ({TRAY_SIZE} px) from {src.name}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
