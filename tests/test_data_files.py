"""Consistency checks for the committed data files."""

import json
from pathlib import Path

DATA = Path(__file__).resolve().parents[1] / "data"
MODS = json.loads((DATA / "mods.json").read_text(encoding="utf-8"))
CARDS = {c["id"]: c for c in MODS["mods"]}
PROFILES = sorted((DATA / "profiles").glob("*.json"))


def test_mods_have_unique_ids_and_variants():
    variant_ids = [v["id"] for c in MODS["mods"] for v in c["variants"]]
    assert len(CARDS) == len(MODS["mods"])
    assert len(variant_ids) == len(set(variant_ids))


def test_mods_are_spawnable():
    for card in MODS["mods"]:
        assert all(v["weight"] > 0 for v in card["variants"]), card["id"]


def test_profiles_reference_existing_mods_on_the_right_side():
    assert PROFILES
    for path in PROFILES:
        profile = json.loads(path.read_text(encoding="utf-8"))
        seen = []
        for side, tiers in (("upside", profile["upsides"]), ("downside", profile["downsides"])):
            for tier, ids in tiers.items():
                for mod_id in ids:
                    assert mod_id in CARDS, f"{path.name}: unknown mod {mod_id}"
                    assert CARDS[mod_id]["side"] == side, f"{path.name}: {mod_id} is not an {side} ({tier})"
                    seen.append(mod_id)
        assert len(seen) == len(set(seen)), f"{path.name}: a mod is listed twice"


def test_default_profile_is_read_only():
    default = json.loads((DATA / "profiles" / "default.json").read_text(encoding="utf-8"))
    assert default["readOnly"] is True
    assert set(default["upsides"]) == {"s", "a", "b"}
    assert set(default["downsides"]) == {"dangerous", "brick"}
