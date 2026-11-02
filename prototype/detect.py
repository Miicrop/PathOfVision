"""Stage 1: find Eldritch Altar icons on a full screenshot via template matching on the ring."""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path

import cv2
import numpy as np

ASSETS = Path(__file__).resolve().parent / "assets"
COARSE_SCALE = 0.25   # full-frame search for candidates
FINE_SCALE = 0.5      # re-score each candidate in a small window
COARSE_THRESHOLD = 0.35
THRESHOLD = 0.55      # real icons score >= 0.66 at the fine scale, look-alikes <= 0.50
RING = (52, 63)       # ring radius band (px, full resolution) that both altar types share
FINE_WINDOW = 12      # px (full resolution) around a coarse hit to search at the fine scale


@dataclass(frozen=True)
class Icon:
    x: int            # ring center, full-resolution pixels
    y: int
    score: float


def _ring_mask(radius: int) -> np.ndarray:
    yy, xx = np.mgrid[-radius:radius + 1, -radius:radius + 1]
    rr = np.hypot(xx, yy)
    ring = (rr >= RING[0]) & (rr <= RING[1])
    bar = (np.abs(yy) <= 4) & (rr < RING[0])   # the horizontal divider between the two halves
    return ((ring | bar) * 255).astype(np.uint8)


def _load_templates(scale: float) -> list[tuple[np.ndarray, np.ndarray]]:
    """One template per altar type; the ring is shared but the ornaments differ slightly."""
    templates = []
    for altar in ("eater", "exarch"):
        gray = cv2.cvtColor(cv2.imread(str(ASSETS / f"icon_{altar}.png")), cv2.COLOR_BGR2GRAY)
        small = cv2.resize(gray, None, fx=scale, fy=scale, interpolation=cv2.INTER_AREA)
        mask = cv2.resize(_ring_mask(gray.shape[0] // 2), small.shape[::-1], interpolation=cv2.INTER_NEAREST)
        templates.append((small, mask))
    return templates


_RADIUS = cv2.imread(str(ASSETS / "icon_eater.png")).shape[0] // 2
_COARSE = _load_templates(COARSE_SCALE)
_FINE = _load_templates(FINE_SCALE)


def _match(gray: np.ndarray, templates) -> np.ndarray:
    """Best score over all templates per top-left position."""
    best = None
    for tpl, mask in templates:
        res = np.nan_to_num(cv2.matchTemplate(gray, tpl, cv2.TM_CCOEFF_NORMED, mask=mask), nan=0, posinf=0, neginf=0)
        best = res if best is None else np.maximum(best, res)
    return best


def find_icons(bgr: np.ndarray, threshold: float = THRESHOLD, max_icons: int = 6) -> list[Icon]:
    gray = cv2.cvtColor(bgr, cv2.COLOR_BGR2GRAY)
    h, w = gray.shape
    coarse = _match(cv2.resize(gray, None, fx=COARSE_SCALE, fy=COARSE_SCALE, interpolation=cv2.INTER_AREA), _COARSE)

    icons = []
    suppress_r = int(RING[1] * COARSE_SCALE * 1.5)
    for _ in range(max_icons * 2):   # weak candidates may be rejected at the fine stage
        _, score, _, (mx, my) = cv2.minMaxLoc(coarse)
        if score < COARSE_THRESHOLD or len(icons) == max_icons:
            break
        cv2.circle(coarse, (mx, my), suppress_r, 0, -1)

        # Re-score at the fine scale in a window around the coarse top-left corner.
        x0 = max(int(mx / COARSE_SCALE) - FINE_WINDOW, 0)
        y0 = max(int(my / COARSE_SCALE) - FINE_WINDOW, 0)
        x1 = min(int(mx / COARSE_SCALE) + 2 * _RADIUS + FINE_WINDOW + 1, w)
        y1 = min(int(my / COARSE_SCALE) + 2 * _RADIUS + FINE_WINDOW + 1, h)
        window = cv2.resize(gray[y0:y1, x0:x1], None, fx=FINE_SCALE, fy=FINE_SCALE, interpolation=cv2.INTER_AREA)
        if min(window.shape) < _FINE[0][0].shape[0]:
            continue   # icon cut off by the screen edge
        fine = _match(window, _FINE)
        _, fscore, _, (fx, fy) = cv2.minMaxLoc(fine)
        if fscore < threshold:
            continue
        cx = x0 + int(fx / FINE_SCALE) + _RADIUS
        cy = y0 + int(fy / FINE_SCALE) + _RADIUS
        if all(abs(cx - i.x) > RING[1] or abs(cy - i.y) > RING[1] for i in icons):
            icons.append(Icon(cx, cy, round(float(fscore), 3)))
    return icons


def altar_type(bgr: np.ndarray, icon: Icon) -> str:
    """Eater icons are blue/teal inside the ring, Exarch icons are red/orange."""
    r = RING[0] - 4
    patch = bgr[max(icon.y - r, 0):icon.y + r, max(icon.x - r, 0):icon.x + r]
    hsv = cv2.cvtColor(patch, cv2.COLOR_BGR2HSV)
    h, s, v = hsv[..., 0], hsv[..., 1], hsv[..., 2]
    vivid = (s > 90) & (v > 70)
    red = np.count_nonzero(vivid & ((h < 20) | (h > 165)))
    blue = np.count_nonzero(vivid & (h > 80) & (h < 125))
    return "exarch" if red > blue else "eater"
