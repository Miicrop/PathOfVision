"""Crop the Path of Exile client area out of full-desktop screenshots.

Usage: python tools/crop_client.py <folder> [--expect WxH] [--monitor-width PX]

Every "Screenshot (N).png" in <folder> is a multi-monitor desktop with a windowed Path of
Exile on the left monitor. The window is found by its light Windows 11 title bar and its
1 px border; the client area (below the title bar, inside the border) is written to
<folder>/client/screen_NN.png, NN = 01.. in numeric order of N. The originals are not
changed. With --expect, every crop must measure exactly WxH or the tool stops.

Assumes the light (default) Windows 11 title bar. Windows rounds the window corners, so the
crop's four corner pixels show a little of the frame; that does not matter for reading altars.
"""

import argparse
import re
import sys
from pathlib import Path

import cv2
import numpy as np

TITLE_MIN_WIDTH = 400  # px of uniform light title bar that make a row count as title bar
LIGHT = 200  # every channel above this counts as light title-bar paint
MIN_CLIENT_H = 400
LINE_MIN = 15  # median contrast of a 1 px border row against both neighbors


def light_run(row: np.ndarray) -> tuple[int, int]:
    """Longest run of light pixels in a BGR row: (start, end exclusive)."""
    light = np.concatenate(([0], (row.min(axis=1) > LIGHT).astype(np.int8), [0]))
    edges = np.diff(light)
    starts, ends = np.where(edges == 1)[0], np.where(edges == -1)[0]
    if len(starts) == 0:
        return (0, 0)
    i = int(np.argmax(ends - starts))
    return int(starts[i]), int(ends[i])


def vertical_line(img: np.ndarray, x: int) -> np.ndarray:
    """Per row: how much column x stands out from both horizontal neighbors (a 1 px border)."""
    c = img[:, x]
    left = np.abs(c - img[:, x - 1]).max(axis=1) if x > 0 else np.full(len(c), 255)
    right = np.abs(c - img[:, x + 1]).max(axis=1) if x + 1 < img.shape[1] else np.full(len(c), 255)
    return np.minimum(left, right)


def find_client(img: np.ndarray, monitor_w: int) -> tuple[int, int, int, int]:
    """(x, y, w, h) of the client area, or raises ValueError.

    Windows 11 draws a light title bar exactly as wide as the client, and a 1 px border that
    is partly transparent (the desktop shows through), so the border is found as a thin line
    that stands out from both neighbors rather than by its color.
    """
    left = img[:, :monitor_w].astype(int)
    h = left.shape[0]
    runs = [light_run(left[y]) for y in range(h)]
    rows = [y for y, (a, b) in enumerate(runs) if b - a >= TITLE_MIN_WIDTH]
    if not rows:
        raise ValueError("no title bar found")
    # The title bar is the first contiguous block of such rows.
    top = rows[0]
    bottom = top
    while bottom + 1 < h and runs[bottom + 1][1] - runs[bottom + 1][0] >= TITLE_MIN_WIDTH:
        bottom += 1
    # Caption buttons and rounded corners shorten single rows; the widest extent is the client.
    extents = [runs[y] for y in range(top, bottom + 1)]
    x0 = min(a for a, _ in extents)
    x1 = max(b for _, b in extents)
    y0 = bottom + 1
    # Side borders run down columns x0 - 1 and x1 and stop at the bottom border.
    side = (vertical_line(left, x0 - 1) + vertical_line(left, x1)) / 2
    if np.median(side[y0 : y0 + MIN_CLIENT_H]) < LINE_MIN / 2:
        raise ValueError(f"no side borders at x={x0 - 1} / x={x1}")
    # Bottom border: a row that stands out from the rows above and below across the client
    # width, and below which the side borders end.
    seg = left[:, x0:x1]
    up = np.abs(seg[1:-1] - seg[:-2]).max(axis=2)
    down = np.abs(seg[1:-1] - seg[2:]).max(axis=2)
    line = np.median(np.minimum(up, down), axis=1)  # line[i] scores row i + 1
    # Where the border barely stands out from the rows next to it, it is still an even gray row.
    even = (seg.std(axis=1).max(axis=1) < 8) & (seg.mean(axis=1).min(axis=1) > 40) & (seg.mean(axis=1).max(axis=1) < 140)
    best, best_y = 0.0, None
    for y in range(y0 + MIN_CLIENT_H, h - 33):
        if line[y - 1] < LINE_MIN and not even[y]:
            continue
        drop = float(np.median(side[y - 30 : y]) - np.median(side[y + 2 : y + 32]))
        if drop > best:
            best, best_y = drop, y
    if best_y is None or best < LINE_MIN:
        raise ValueError("no bottom border found")
    return x0, y0, x1 - x0, best_y - y0


def shot_number(p: Path) -> int:
    m = re.search(r"\((\d+)\)", p.name)
    return int(m.group(1)) if m else -1


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("folder", type=Path)
    ap.add_argument("--expect", help="WxH every crop must measure")
    ap.add_argument("--monitor-width", type=int, default=3440, help="width of the monitor holding the game")
    args = ap.parse_args()
    shots = sorted(args.folder.glob("Screenshot (*).png"), key=shot_number)
    if not shots:
        print(f"no 'Screenshot (N).png' in {args.folder}", file=sys.stderr)
        return 1
    out = args.folder / "client"
    out.mkdir(exist_ok=True)
    sizes = set()
    for i, p in enumerate(shots, 1):
        img = cv2.imread(str(p))
        try:
            x, y, w, h = find_client(img, args.monitor_width)
        except ValueError as e:
            print(f"{p.name}: {e}", file=sys.stderr)
            return 1
        if args.expect and f"{w}x{h}" != args.expect:
            print(f"{p.name}: client {w}x{h} at ({x}, {y}), expected {args.expect}", file=sys.stderr)
            return 1
        name = f"screen_{i:02d}.png"
        cv2.imwrite(str(out / name), img[y : y + h, x : x + w])
        sizes.add((w, h))
        print(f"{p.name} -> client/{name}: {w}x{h} at ({x}, {y})")
    print("client sizes:", ", ".join(f"{w}x{h}" for w, h in sorted(sizes)))
    return 0


if __name__ == "__main__":
    sys.exit(main())
