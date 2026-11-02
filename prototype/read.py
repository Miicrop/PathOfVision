"""Stage 2: read both options of every altar on a screenshot.

Usage: python -m prototype.read tests/fixtures/screens/screen_02.png [...]
"""

from __future__ import annotations

import sys
import time
from dataclasses import dataclass

import cv2
import numpy as np

from .detect import RING, Icon, altar_type, find_icons
from .match import Card, Match, load_cards, match_box, parse_header
from .ocr import WindowsOcr

SEARCH_HALF_WIDTH = 650    # px left/right of the icon center to OCR (widest box is ~1000 px)
SEARCH_HEIGHT = 320        # px above/below the icon to OCR (tallest box is ~250 px)
UPSCALE = 2
TEXT_BRIGHTNESS = 110      # max(B, G, R) above this counts as text
CENTER_TOLERANCE = 20      # slack when testing that a line crosses the icon center column
MAX_FRAGMENT_GAP = 220     # px between pieces of one row split by an icon or life bar
LINE_GAP = 22              # max vertical gap (full-res px) between lines of one box


@dataclass
class Option:
    position: str              # "top" or "bottom"
    target: str | None
    lines: list[str]
    matches: list[Match]
    unexplained: list[str]


@dataclass
class Altar:
    icon: Icon
    altar: str
    options: list[Option]


@dataclass
class _Line:
    text: str
    x0: float
    x1: float
    y0: float
    y1: float


def _ocr_region(ocr: WindowsOcr, img: np.ndarray, x0: int, y0: int, x1: int, y1: int) -> list[_Line]:
    """OCR a region; returns lines in full-res coords."""
    crop = img[y0:y1, x0:x1]
    if crop.size == 0:
        return []
    # Box text is bright; the semi-transparent background lets dimmer game art through.
    # Keep only bright pixels, as dark text on white.
    brightest = crop.max(axis=2)
    clean = np.where(brightest > TEXT_BRIGHTNESS, 0, 255).astype(np.uint8)
    big = cv2.cvtColor(cv2.resize(clean, None, fx=UPSCALE, fy=UPSCALE, interpolation=cv2.INTER_CUBIC), cv2.COLOR_GRAY2BGR)
    return [
        _Line(text, x0 + x / UPSCALE, x0 + (x + w) / UPSCALE, y0 + y / UPSCALE, y0 + (y + h) / UPSCALE)
        for text, (x, y, w, h) in ocr.read(big)
    ]


def _merge_rows(lines: list[_Line]) -> list[_Line]:
    """Join fragments of one text row that an icon or life bar split into pieces."""
    merged: list[_Line] = []
    for line in sorted(lines, key=lambda l: l.x0):
        for row in merged:
            overlap = min(row.y1, line.y1) - max(row.y0, line.y0)
            if overlap > 0.5 * min(row.y1 - row.y0, line.y1 - line.y0) and line.x0 - row.x1 <= MAX_FRAGMENT_GAP:
                row.text, row.x1 = f"{row.text} {line.text}", max(row.x1, line.x1)
                row.y0, row.y1 = min(row.y0, line.y0), max(row.y1, line.y1)
                break
        else:
            merged.append(_Line(line.text, line.x0, line.x1, line.y0, line.y1))
    return merged


def _box_lines(lines: list[_Line], cx: float, near_y: float, upward: bool) -> list[_Line]:
    """Lines crossing the icon's center column that form one contiguous block next to the icon.

    Box text is centered on the icon, but lines cut off by HUD or screen edges are not,
    so the test is only that the line spans the center column.
    """
    centered = [l for l in _merge_rows(lines) if l.x0 - CENTER_TOLERANCE <= cx <= l.x1 + CENTER_TOLERANCE]
    centered.sort(key=lambda l: -l.y1 if upward else l.y0)
    block, edge = [], near_y
    for line in centered:
        gap = (edge - line.y1) if upward else (line.y0 - edge)
        if gap > (LINE_GAP if block else LINE_GAP * 3):
            break
        block.append(line)
        edge = line.y0 if upward else line.y1
    block.sort(key=lambda l: l.y0)
    return block


def read_altars(img: np.ndarray, ocr: WindowsOcr, cards: list[Card]) -> list[Altar]:
    altars = []
    h, w = img.shape[:2]
    for icon in find_icons(img):
        kind = altar_type(img, icon)
        x0, x1 = max(icon.x - SEARCH_HALF_WIDTH, 0), min(icon.x + SEARCH_HALF_WIDTH, w)
        top_edge, bottom_edge = icon.y - RING[1], icon.y + RING[1]
        regions = {
            "top": (_ocr_region(ocr, img, x0, max(top_edge - SEARCH_HEIGHT, 0), x1, top_edge), top_edge, True),
            "bottom": (_ocr_region(ocr, img, x0, bottom_edge, x1, min(bottom_edge + SEARCH_HEIGHT, h)), bottom_edge, False),
        }
        options = []
        for position, (lines, edge, upward) in regions.items():
            block = [l.text for l in _box_lines(lines, icon.x, edge, upward)]
            target = None
            # The header is the box's first line; centered strays before it (gold labels, timers) go.
            header_at = next((i for i, text in enumerate(block) if parse_header(text)), None)
            if header_at is not None:
                target = parse_header(block[header_at])
                block = block[header_at + 1:]
            # No altar filter: mods of the other altar type do show up (e.g. Embers on an Eater altar).
            matches, rest = match_box(block, cards, target, None)
            if target is None and matches:
                target = matches[0].card.target
            options.append(Option(position, target, block, matches, [block[i] for i in rest]))
        if any(o.matches for o in options):   # verified: real altar boxes found
            altars.append(Altar(icon, kind, options))
    return altars


def main(paths: list[str]) -> int:
    ocr, cards = WindowsOcr(), load_cards()
    for path in paths:
        img = cv2.imread(path)
        t = time.perf_counter()
        altars = read_altars(img, ocr, cards)
        ms = (time.perf_counter() - t) * 1000
        print(f"== {path}  ({ms:.0f} ms, {len(altars)} altar(s))")
        for a in altars:
            print(f"  [{a.altar}] icon at ({a.icon.x}, {a.icon.y}) score {a.icon.score}")
            for o in a.options:
                print(f"    {o.position.upper():6} {o.target}")
                for m in o.matches:
                    flag = ("" if m.in_range else "  !range") + ("  partial" if m.partial else "")
                    print(f"      {m.card.side[:4]} {m.card.id}  {m.values} ({m.score}){flag}")
                for u in o.unexplained:
                    print(f"      ?    {u}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
