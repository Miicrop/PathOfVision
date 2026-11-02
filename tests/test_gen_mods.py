from tools.gen_mods.gen_mods import build_mods, diff_mods, parse_ranges, sync_labels, to_template


def raw_mod(text, tag, weight=1000, blocked="primordial_altar_downside"):
    return {
        "domain": "primordial_altar",
        "spawn_weights": [{"tag": blocked, "weight": 0}, {"tag": tag, "weight": weight}, {"tag": "default", "weight": 0}],
        "text": text,
    }


RAW = {
    "TangleAltarUpsideMonsterDivineOrb": raw_mod("(1.6-3.2)% chance to drop an additional Divine Orb", "tangle_altar"),
    "TangledAltarUpsidePlayerIncreasedRarity": raw_mod("(15-30)% increased Rarity of Items found in this Area", "tangle_altar", 165600),
    "CleansingAltarUpsidePlayerIncreasedRarity": raw_mod("(15-30)% increased Rarity of Items found in this Area", "cleansing_fire_altar", 75225),
    "HardModeTangledAltarUpsidePlayerIncreasedRarity": raw_mod("(5-10)% increased Rarity of Items found in this Area", "tangle_altar", 500),
    "TangleAltarUpsideBossGrandEldritchIchor": raw_mod("Final Boss drops (2-4) additional Grand Eldritch Ichors", "tangle_altar", 20000),
    "HardModeTangleAltarUpsideBossGrandEldritchIchor": raw_mod("Final Boss drops 1 additional Grand Eldritch Ichor", "tangle_altar", 165),
    "TangledAltarDownsidePlayerSapped": raw_mod(
        "(25-35)% chance to be Sapped when Hit\nAll Damage taken from Hits can Sap you", "tangle_altar", blocked="primordial_altar_upside"),
    "TangledAltarDownsideBossAllDamageCanChill": raw_mod("All Damage with Hits can Chill", "tangle_altar", 0, "primordial_altar_upside"),
    "Strength1": {"domain": "item", "spawn_weights": [], "text": "+(8-12) to Strength"},
}


def cards_by_id():
    return {c["id"]: c for c in build_mods(RAW, "test")["mods"]}


def test_to_template_matches_ocr_text():
    assert to_template("(1.6-3.2)% chance to drop an additional Divine Orb") == "#% chance to drop an additional Divine Orb"
    assert to_template("2.2% chance to drop an additional Divine Orb") == "#% chance to drop an additional Divine Orb"
    assert to_template("-(60-40)% to Cold Resistance") == to_template("-52% to Cold Resistance") == "-#% to Cold Resistance"
    assert to_template("Take 600.0 Chaos Damage per second") == "Take # Chaos Damage per second"


def test_parse_ranges_sorts_and_applies_sign():
    assert parse_ranges("(20-10)% reduced Recovery Rate") == [[10, 20]]
    assert parse_ranges("-(60-40)% to Cold Resistance") == [[-60, -40]]
    assert parse_ranges("(1.6-3.2)% chance") == [[1.6, 3.2]]
    assert parse_ranges("Enemies lose 6 Flask Charges every 3 seconds") == [[6, 6], [3, 3]]


def test_only_spawnable_altar_mods_are_kept():
    cards = cards_by_id()
    variant_ids = {v["id"] for c in cards.values() for v in c["variants"]}
    assert "TangledAltarDownsideBossAllDamageCanChill" not in variant_ids
    assert "Strength1" not in variant_ids
    assert len(variant_ids) == 7


def test_side_target_and_altar():
    card = cards_by_id()["TangleAltarUpsideMonsterDivineOrb"]
    assert (card["side"], card["target"], card["altars"]) == ("upside", "minions", ["eater"])
    sap = cards_by_id()["TangledAltarDownsidePlayerSapped"]
    assert (sap["side"], sap["target"]) == ("downside", "player")
    assert sap["variants"][0]["templates"] == ["#% chance to be Sapped when Hit", "All Damage taken from Hits can Sap you"]


def test_generic_and_hard_mode_variants_share_a_card():
    rarity = cards_by_id()["TangledAltarUpsidePlayerIncreasedRarity"]
    assert rarity["altars"] == ["eater", "exarch"]
    assert rarity["weights"] == {"eater": 166100, "exarch": 75225}
    assert [v["hardMode"] for v in rarity["variants"]] == [False, False, True]

    ichor = cards_by_id()["TangleAltarUpsideBossGrandEldritchIchor"]
    assert [v["id"] for v in ichor["variants"]] == [
        "TangleAltarUpsideBossGrandEldritchIchor", "HardModeTangleAltarUpsideBossGrandEldritchIchor"]


def test_build_is_deterministic():
    assert build_mods(RAW, "test") == build_mods(dict(reversed(RAW.items())), "test")


def test_diff_reports_added_removed_changed():
    old = build_mods(RAW, "test")
    raw = dict(RAW)
    del raw["TangleAltarUpsideMonsterDivineOrb"]
    raw["TangledAltarDownsidePlayerSapped"] = raw_mod(
        "(25-40)% chance to be Sapped when Hit\nAll Damage taken from Hits can Sap you", "tangle_altar", blocked="primordial_altar_upside")
    raw["TangleAltarUpsideMonsterExaltedOrb"] = raw_mod("(1.6-3.2)% chance to drop an additional Exalted Orb", "tangle_altar")
    diff = diff_mods(old, build_mods(raw, "test"))
    assert diff[0].startswith("+ added   TangleAltarUpsideMonsterExaltedOrb")
    assert diff[1].startswith("- removed TangleAltarUpsideMonsterDivineOrb")
    assert diff[2] == "~ changed TangledAltarDownsidePlayerSapped: lines, variants"
    assert diff_mods(old, old) == []


def test_sync_labels_keeps_hand_work_and_adds_new_cards():
    mods = build_mods(RAW, "test")
    old = {"labels": {
        "TangledAltarDownsidePlayerSapped": {"text": "stale", "alias": "Sap #%", "icon": "sap"},
        "GoneAltarUpsidePlayerSomething": {"text": "old", "alias": "Gone", "icon": ""},
    }}
    labels, notes = sync_labels(mods, old)
    entries = labels["labels"]
    sap = entries["TangledAltarDownsidePlayerSapped"]
    assert (sap["alias"], sap["icon"]) == ("Sap #%", "sap")
    assert sap["text"].startswith("player - (25-35)% chance to be Sapped")
    assert entries["TangleAltarUpsideMonsterDivineOrb"] == {
        "text": "minions + (1.6-3.2)% chance to drop an additional Divine Orb", "alias": "", "icon": ""}
    assert entries["GoneAltarUpsidePlayerSomething"]["alias"] == "Gone"
    assert notes == ["? unused label GoneAltarUpsidePlayerSomething (card no longer exists)"]
    assert set(entries) - {"GoneAltarUpsidePlayerSomething"} == {c["id"] for c in mods["mods"]}
