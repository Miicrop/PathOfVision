"""Measure recognition accuracy against tests/fixtures/labels.json.

Usage:
    python -m prototype.evaluate                 # accuracy + timing report
    python -m prototype.evaluate --draft out.json  # write current results as a label draft

Labels (one entry per altar, matched to detections by icon position):
    {"screen_02.png": [{"icon": [956, 1134], "altar": "eater",
                         "top":    {"target": "boss",   "mods": ["<card id>", ...]},
                         "bottom": {"target": "player", "mods": null, "note": "hidden by HUD"}}]}
"mods": null marks a box that is not readable on the screenshot; it is skipped.
An option counts as correct when target and the multiset of mod ids match exactly.
"""

from __future__ import annotations

import argparse
import json
import sys
import time
from collections import Counter
from pathlib import Path

import cv2

from .match import load_cards
from .ocr import WindowsOcr
from .read import read_altars

FIXTURES = Path(__file__).resolve().parents[1] / "tests" / "fixtures"
SCREENS = FIXTURES / "screens"
LABELS = FIXTURES / "labels.json"
ICON_TOLERANCE = 40   # px between labeled and detected icon centers


def _results():
    ocr, cards = WindowsOcr(), load_cards()
    for path in sorted(SCREENS.glob("screen_*.png")):
        img = cv2.imread(str(path))
        t = time.perf_counter()
        altars = read_altars(img, ocr, cards)
        yield path.name, altars, (time.perf_counter() - t) * 1000


def draft(out: Path) -> None:
    labels = {}
    for name, altars, _ in _results():
        labels[name] = [
            {
                "icon": [a.icon.x, a.icon.y],
                "altar": a.altar,
                **{o.position: {"target": o.target, "mods": [m.card.id for m in o.matches]} for o in a.options},
            }
            for a in altars
        ]
    out.write_text(json.dumps(labels, indent=1) + "\n", encoding="utf-8")
    print(f"draft written to {out}")


def evaluate() -> int:
    labels = json.loads(LABELS.read_text(encoding="utf-8"))
    altars_total = altars_found = options_total = options_ok = false_altars = 0
    timings, failures = [], []

    for name, altars, ms in _results():
        timings.append(ms)
        expected = labels.get(name, [])
        unmatched = list(altars)
        for exp in expected:
            altars_total += 1
            hit = next((a for a in unmatched if abs(a.icon.x - exp["icon"][0]) <= ICON_TOLERANCE
                        and abs(a.icon.y - exp["icon"][1]) <= ICON_TOLERANCE), None)
            if hit:
                altars_found += 1
                unmatched.remove(hit)
            for position in ("top", "bottom"):
                want = exp[position]
                if want["mods"] is None:
                    continue
                options_total += 1
                got = next((o for o in hit.options if o.position == position), None) if hit else None
                got_mods = Counter(m.card.id for m in got.matches) if got else Counter()
                if got and got.target == want["target"] and got_mods == Counter(want["mods"]):
                    options_ok += 1
                else:
                    missing = Counter(want["mods"]) - got_mods
                    extra = got_mods - Counter(want["mods"])
                    target = got.target if got else "-"
                    failures.append(f"{name} {exp['icon']} {position}: target {target} (want {want['target']}), "
                                    f"missing {sorted(missing.elements())}, extra {sorted(extra.elements())}")
        false_altars += len(unmatched)

    timings.sort()
    print(f"altars detected : {altars_found}/{altars_total}  (+{false_altars} false)")
    print(f"options correct : {options_ok}/{options_total}  ({100 * options_ok / max(options_total, 1):.1f} %)")
    print(f"time per screen : median {timings[len(timings) // 2]:.0f} ms, max {timings[-1]:.0f} ms")
    for f in failures:
        print("  FAIL", f)
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--draft", type=Path, help="write current results as a label draft")
    args = parser.parse_args(argv)
    if args.draft:
        draft(args.draft)
        return 0
    return evaluate()


if __name__ == "__main__":
    sys.exit(main())
