"""Cut the target scenes out of the altar half-circles for the board card backgrounds.

Usage: python -m tools.gen_board_art

Reads ui/modules/altar/img/target_<target>_top_<altar>.png, drops the bronze rim with a soft
half-disc mask and writes ui/board/img/art_<target>_<altar>.png. Top and bottom halves show
the same scene, the top one frames the player's head better.

The card look (slight blur, less saturation, oval fade around the card's center) is baked into
the files: as CSS filter and mask on 201 cards it made scrolling the board stutter.
"""

from __future__ import annotations

import sys
from pathlib import Path

import cv2
import numpy as np

ROOT = Path(__file__).resolve().parents[1]
ASSETS = ROOT / "ui" / "modules" / "altar" / "img"
OUT = ROOT / "ui" / "board" / "img"
TARGETS = ("player", "minions", "boss")
ALTARS = ("eater", "exarch")
INNER_RADIUS = 90            # source is 220 x 108, the rim starts around r = 100
CROP = (8, 4, 212, 100)      # x0, y0, x1, y1
SATURATION = 0.8
BLUR = 0.6                   # px in the source; the card shows it about 2x larger
# Part of the image a card shows (board.css: background-size 130% wide, position center 30%), as
# fractions of the image; the oval fade is centered at 50% / 40% of that window like the old CSS mask.
VIEW_W, VIEW_H, VIEW_TOP = 1 / 1.3, 0.52, 0.144
FADE = (0.5, 1.05)           # fully visible inside, gone outside (relative oval radius)


def cut(img: np.ndarray) -> np.ndarray:
    h, w = img.shape[:2]
    mask = np.zeros((h, w), np.uint8)
    cv2.circle(mask, (w // 2, 104), INNER_RADIUS, 255, -1)
    mask[90:] = 0                                            # flat rim along the bottom edge
    # A wide fade: a sharp arc would draw a line through every card of the same kind.
    mask = cv2.GaussianBlur(mask, (0, 0), 12)
    out = img.copy()
    out[:, :, 3] = (out[:, :, 3].astype(np.uint16) * mask // 255).astype(np.uint8)
    x0, y0, x1, y1 = CROP
    return out[y0:y1, x0:x1]


def card_look(img: np.ndarray) -> np.ndarray:
    out = img.astype(np.float32)
    rgb = out[:, :, :3]
    gray = rgb @ np.array([0.114, 0.587, 0.299], np.float32)          # BGR luminance
    out[:, :, :3] = gray[..., None] + SATURATION * (rgb - gray[..., None])
    out = cv2.GaussianBlur(out, (0, 0), BLUR)
    h, w = out.shape[:2]
    y, x = np.mgrid[0:h, 0:w].astype(np.float32)
    cx, cy = w / 2, h * (VIEW_TOP + 0.4 * VIEW_H)
    d = np.hypot((x - cx) / (w * VIEW_W * 0.71), (y - cy) / (h * VIEW_H * 0.71))
    fade = np.clip((FADE[1] - d) / (FADE[1] - FADE[0]), 0, 1)
    out[:, :, 3] *= fade
    return np.clip(out, 0, 255).astype(np.uint8)


def main() -> int:
    OUT.mkdir(parents=True, exist_ok=True)
    for target in TARGETS:
        for altar in ALTARS:
            img = cv2.imread(str(ASSETS / f"target_{target}_top_{altar}.png"), cv2.IMREAD_UNCHANGED)
            path = OUT / f"art_{target}_{altar}.png"
            cv2.imwrite(str(path), card_look(cut(img)))
            print(path.relative_to(ROOT))
    return 0


if __name__ == "__main__":
    sys.exit(main())
