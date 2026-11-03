"""Cut the target scenes out of the altar half-circles for the board card backgrounds.

Usage: python -m tools.gen_board_art

Reads prototype/assets/target_<target>_top_<altar>.png, drops the bronze rim with a soft
half-disc mask and writes ui/board/img/art_<target>_<altar>.png. Top and bottom halves show
the same scene, the top one frames the player's head better.
"""

from __future__ import annotations

import sys
from pathlib import Path

import cv2
import numpy as np

ROOT = Path(__file__).resolve().parents[1]
ASSETS = ROOT / "prototype" / "assets"
OUT = ROOT / "ui" / "board" / "img"
TARGETS = ("player", "minions", "boss")
ALTARS = ("eater", "exarch")
INNER_RADIUS = 90            # source is 220 x 108, the rim starts around r = 100
CROP = (8, 4, 212, 100)      # x0, y0, x1, y1


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


def main() -> int:
    OUT.mkdir(parents=True, exist_ok=True)
    for target in TARGETS:
        for altar in ALTARS:
            img = cv2.imread(str(ASSETS / f"target_{target}_top_{altar}.png"), cv2.IMREAD_UNCHANGED)
            path = OUT / f"art_{target}_{altar}.png"
            cv2.imwrite(str(path), cut(img))
            print(path.relative_to(ROOT))
    return 0


if __name__ == "__main__":
    sys.exit(main())
