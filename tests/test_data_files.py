"""Consistency checks for the committed data files."""

import json
from pathlib import Path

from prototype.verdict import upgrade_upsides

DATA = Path(__file__).resolve().parents[1] / "data"
MODS = json.loads((DATA / "mods.json").read_text(encoding="utf-8"))
CARDS = {c["id"]: c for c in MODS["mods"]}
PROFILES = sorted((DATA / "profiles").glob("*.json"))
LABELS = json.loads((DATA / "labels.json").read_text(encoding="utf-8"))["labels"]
MOD_ICONS = DATA.parent / "ui" / "modules" / "altar" / "img" / "mods"


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
        up = upgrade_upsides(profile["upsides"])   # user profiles may still be in the old s/a/b format
        up_lists = {c["name"]: c["ids"] for c in up["columns"]} | {"top": up["top"]["ids"]}
        seen = []
        for side, tiers in (("upside", up_lists), ("downside", profile["downsides"])):
            for tier, ids in tiers.items():
                for mod_id in ids:
                    assert mod_id in CARDS, f"{path.name}: unknown mod {mod_id}"
                    assert CARDS[mod_id]["side"] == side, f"{path.name}: {mod_id} is not an {side} ({tier})"
                    seen.append(mod_id)
        assert len(seen) == len(set(seen)), f"{path.name}: a mod is listed twice"


def test_default_profile_is_read_only():
    default = json.loads((DATA / "profiles" / "default.json").read_text(encoding="utf-8"))
    assert default["readOnly"] is True
    assert set(default["upsides"]) == {"columns", "top"}
    assert set(default["downsides"]) == {"dangerous", "brick"}


def test_labels_cover_every_card_and_icons_exist():
    assert set(LABELS) == set(CARDS), "run python -m tools.gen_mods --offline to sync data/labels.json"
    for cid, entry in LABELS.items():
        if entry["icon"]:
            assert (MOD_ICONS / f"{entry['icon']}.png").exists(), f"{cid}: missing icon {entry['icon']}.png"


def test_empty_template_only_rates_divine_orbs():
    empty = json.loads((DATA / "profiles" / "empty.json").read_text(encoding="utf-8"))
    assert empty["readOnly"] is True
    assert sorted(empty["upsides"]["top"]["ids"]) == ["TangleAltarUpsideBossDivineOrb", "TangleAltarUpsideMonsterDivineOrb"]
    assert not any(c["ids"] for c in empty["upsides"]["columns"]) and not any(empty["downsides"].values())
