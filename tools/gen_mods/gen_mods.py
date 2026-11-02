"""Generate data/mods.json from the RePoE game data export.

Usage:
    python -m tools.gen_mods               # download the latest export
    python -m tools.gen_mods --offline     # reuse the cached download
    python -m tools.gen_mods --input mods.min.json --game-version 3.29.3.3

Every run prints a diff (added / removed / changed cards) against the
existing data/mods.json before overwriting it.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
import urllib.request
from collections import defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
OUT_PATH = ROOT / "data" / "mods.json"
CACHE_DIR = Path(__file__).resolve().parent / ".cache"

SOURCE_URL = "https://repoe-fork.github.io/mods.min.json"
VERSION_URL = "https://repoe-fork.github.io/version.txt"

DOMAIN = "primordial_altar"
ALTAR_TAGS = {"tangle_altar": "eater", "cleansing_fire_altar": "exarch"}
TARGETS = {"Player": "player", "Monster": "minions", "Boss": "boss"}

# Game IDs are inconsistent ("TangledAltar", "TangleAltar", "TangledAtlar",
# "CleansingAltar"), so only this part of the ID is trusted.
SIDE_TARGET_RE = re.compile(r"(Upside|Downside)(Boss|Monster|Player)")
RANGE_RE = re.compile(r"\((-?\d+(?:\.\d+)?)-(-?\d+(?:\.\d+)?)\)")
NUMBER_RE = re.compile(r"(?<![\w.#])\d+(?:\.\d+)?")
VALUE_RE = re.compile(r"(-?)(?:\((-?\d+(?:\.\d+)?)-(-?\d+(?:\.\d+)?)\)|(?<![\w.])(\d+(?:\.\d+)?))")


def to_template(line: str) -> str:
    """Replace every value or value range with '#'. Also used on OCR text."""
    return NUMBER_RE.sub("#", RANGE_RE.sub("#", line))


def parse_ranges(line: str) -> list[list[float]]:
    """Return [low, high] per value in display order; signs outside the parens apply."""
    ranges = []
    for sign, lo, hi, single in VALUE_RE.findall(line):
        values = [float(single)] * 2 if single else [float(lo), float(hi)]
        if sign:
            values = [-v for v in values]
        ranges.append([_num(min(values)), _num(max(values))])
    return ranges


def _num(value: float) -> float | int:
    return int(value) if value.is_integer() else value


def parse_variant(mod_id: str, mod: dict) -> dict | None:
    """Turn one raw game mod into a variant, or None if it cannot spawn."""
    if mod_id.startswith("HardMode"):  # Ruthless-only rolls, out of scope
        return None
    match = SIDE_TARGET_RE.search(mod_id)
    if not match:
        raise ValueError(f"unexpected altar mod id: {mod_id}")
    side, target = match.groups()

    spawning = [w for w in mod["spawn_weights"] if w["weight"] > 0 and w["tag"] in ALTAR_TAGS]
    if not spawning:
        return None
    if len(spawning) > 1:
        raise ValueError(f"mod spawns on several altars: {mod_id}")

    lines = mod["text"].split("\n")
    return {
        "id": mod_id,
        "altar": ALTAR_TAGS[spawning[0]["tag"]],
        "side": side.lower(),
        "target": TARGETS[target],
        "lines": lines,
        "templates": [to_template(line) for line in lines],
        "ranges": [r for line in lines for r in parse_ranges(line)],
        "weight": spawning[0]["weight"],
    }


def group_variants(variants: list[dict]) -> list[list[dict]]:
    """Group variants into cards: the same side, target and text on both altars is one generic card."""
    groups = defaultdict(list)
    for v in variants:
        groups[(v["side"], v["target"], tuple(v["templates"]))].append(v)
    return list(groups.values())


def build_card(group: list[dict]) -> dict:
    group = sorted(group, key=lambda v: (v["altar"], v["id"]))
    primary = group[0]
    weights: dict[str, int] = {}
    for v in group:
        weights[v["altar"]] = weights.get(v["altar"], 0) + v["weight"]
    return {
        "id": primary["id"],
        "altars": sorted(weights),
        "side": primary["side"],
        "target": primary["target"],
        "lines": primary["lines"],
        "weights": weights,
        "variants": [
            {k: v[k] for k in ("id", "altar", "lines", "templates", "ranges", "weight")}
            for v in group
        ],
    }


def build_mods(raw: dict, game_version: str) -> dict:
    variants = []
    for mod_id, mod in raw.items():
        if mod.get("domain") != DOMAIN:
            continue
        variant = parse_variant(mod_id, mod)
        if variant:
            variants.append(variant)

    cards = [build_card(g) for g in group_variants(variants)]
    cards.sort(key=lambda c: (c["side"] != "upside", c["target"], c["id"]))
    return {"gameVersion": game_version, "source": SOURCE_URL, "mods": cards}


def diff_mods(old: dict | None, new: dict) -> list[str]:
    """Human-readable diff between two mods.json documents, keyed by card id."""
    old_cards = {c["id"]: c for c in (old or {}).get("mods", [])}
    new_cards = {c["id"]: c for c in new["mods"]}
    lines = []
    if old and old.get("gameVersion") != new["gameVersion"]:
        lines.append(f"game version: {old.get('gameVersion')} -> {new['gameVersion']}")
    for cid in sorted(new_cards.keys() - old_cards.keys()):
        lines.append(f"+ added   {cid}: {' / '.join(new_cards[cid]['lines'])}")
    for cid in sorted(old_cards.keys() - new_cards.keys()):
        lines.append(f"- removed {cid}: {' / '.join(old_cards[cid]['lines'])}")
    for cid in sorted(old_cards.keys() & new_cards.keys()):
        changed = [k for k in new_cards[cid] if old_cards[cid].get(k) != new_cards[cid][k]]
        if changed:
            lines.append(f"~ changed {cid}: {', '.join(changed)}")
    return lines


def fetch(url: str, cache_name: str, offline: bool) -> bytes:
    path = CACHE_DIR / cache_name
    if offline:
        return path.read_bytes()
    with urllib.request.urlopen(url, timeout=120) as resp:
        data = resp.read()
    CACHE_DIR.mkdir(exist_ok=True)
    path.write_bytes(data)
    return data


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--input", type=Path, help="local RePoE mods.min.json instead of downloading")
    parser.add_argument("--game-version", help="game version label (default: RePoE version.txt)")
    parser.add_argument("--offline", action="store_true", help="use the cached download")
    parser.add_argument("--out", type=Path, default=OUT_PATH)
    parser.add_argument("--dry-run", action="store_true", help="print the diff without writing")
    args = parser.parse_args(argv)
    sys.stdout.reconfigure(errors="replace")  # Windows consoles default to cp1252

    raw_bytes = args.input.read_bytes() if args.input else fetch(SOURCE_URL, "mods.min.json", args.offline)
    game_version = args.game_version or fetch(VERSION_URL, "version.txt", args.offline).decode().strip()

    new = build_mods(json.loads(raw_bytes), game_version)
    old = json.loads(args.out.read_text(encoding="utf-8")) if args.out.exists() else None

    diff = diff_mods(old, new)
    variant_count = sum(len(c["variants"]) for c in new["mods"])
    print(f"{len(new['mods'])} cards from {variant_count} spawnable mods (game {game_version})")
    print("\n".join(diff) if diff else "no changes")

    if not args.dry_run:
        args.out.parent.mkdir(parents=True, exist_ok=True)
        args.out.write_text(json.dumps(new, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    return 0


if __name__ == "__main__":
    sys.exit(main())
