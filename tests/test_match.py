from collections import Counter

import pytest

from prototype.match import load_cards, match_box, normalize, parse_header


@pytest.fixture(scope="module")
def cards():
    return load_cards()


def ids(matches):
    return Counter(m.card.id for m in matches)


def test_normalize_fixes_ocr_slips():
    assert normalize("2,5% chance to drop an additional Jeweller's Orb") == normalize("2.5% CHANCE TO DROP AN ADDITIONAL JEWELLER'S ORB")
    assert normalize("1O% increased Quantity") == "#% INCREASED QUANTITY"
    assert normalize("33 % chance") == "#% CHANCE"


@pytest.mark.parametrize("text, target", [
    ("PLAYER GAINS:", "player"),
    ("ELDRITCH MINIONS GAIN:", "minions"),
    ("MINIONS GAIN:", "minions"),          # partly hidden
    ("MAP Boss GAINS:", "boss"),
    ("NEARBY ENEMIES GAIN 100% OF THEIR PHYSICAL DAMAGE AS EXTRA COLD DAMAGE", None),
    ("GAIN 82% OF PHYSICAL DAMAGE AS EXTRA FIRE DAMAGE", None),
])
def test_parse_header(text, target):
    assert parse_header(text) == target


def test_interleaved_multi_line_mods(cards):
    # In-game order mixes the lines of different mods (screen_13).
    lines = [
        "HITS ALWAYS SHOCK",
        "GAIN 118% OF PHYSICAL DAMAGE AS EXTRA LIGHTNING DAMAGE",
        "ALL DAMAGE CAN SHOCK",
        "GAIN 105% OF PHYSICAL DAMAGE AS EXTRA COLD DAMAGE",
        "ALL DAMAGE WITH HITS CAN CHILL",
        "FINAL BOSS DROPS 4 ADDITIONAL DIVINATION CARDS WHICH REWARD BASIC CURRENCY",
    ]
    matches, rest = match_box(lines, cards, "boss", None)
    assert rest == []
    assert ids(matches) == Counter([
        "TangledAltarDownsideBossPhysToAddAsLightning",
        "TangledAltarDownsideBossPhysToAddAsCold",
        "TangledAltarUpsideBossDivinationCardsCurrencyBasic",
    ])


def test_shared_line_goes_to_the_right_mods(cards):
    # Two mods each own a "Gain X% ... as Extra Fire Damage" line (screen_23).
    lines = [
        "HITS ALWAYS IGNITE",
        "GAIN 82% OF PHYSICAL DAMAGE AS EXTRA FIRE DAMAGE",
        "ALL DAMAGE CAN IGNITE",
        "GAIN 71% OF PHYSICAL DAMAGE AS FIRE DAMAGE",   # OCR dropped "EXTRA"
        "COVER ENEMIES IN ASH ON HIT",
    ]
    matches, rest = match_box(lines, cards, "boss", None)
    assert rest == []
    assert ids(matches) == Counter(["CleansingAltarDownsideBossPhysToAddAsFire", "CleansingAltarDownsideBossCoveredInAshOnHit"])


def test_same_mod_twice(cards):
    lines = ["24% INCREASED RARITY OF ITEMS FOUND IN THIS AREA"] * 2
    matches, _ = match_box(lines, cards, "player", None)
    assert ids(matches) == Counter({"TangledAltarUpsidePlayerIncreasedRarity": 2})


def test_partly_hidden_multi_line_mod(cards):
    lines = ["+10% TO MAXIMUM FIRE RESISTANCE", "+80% TO FIRE RESISTANCE", "2.3% CHANCE TO DROP AN ADDITIONAL ORB OF UNMAKING"]
    matches, rest = match_box(lines, cards, "minions", None)
    assert rest == []
    by_id = {m.card.id: m for m in matches}
    assert by_id["CleansingAltarDownsideMonsterFireAndChaosResist"].partial
    assert not by_id["CleansingAltarUpsideMonsterOrbOfUnmaking"].partial


def test_unknown_target_picks_the_best_fit(cards):
    lines = [
        "+10% TO MAXIMUM COLD RESISTANCE", "+80% TO COLD RESISTANCE",
        "+10% TO MAXIMUM LIGHTNING RESISTANCE", "+80% TO LIGHTNING RESISTANCE",
        "FINAL BOSS DROPS 4 ADDITIONAL DIVINATION CARDS WHICH REWARD BASIC CURRENCY",
    ]
    matches, rest = match_box(lines, cards, None, None)
    assert rest == []
    assert {m.card.target for m in matches} == {"boss"}


def test_display_fills_read_values_into_clean_text(cards):
    lines = ["33% CHANCE TO BE SAPPED WHEN HIT 78 GOLD", "ALL DAMAGE TAKEN FROM HITS CAN SAP YOU"]
    (match,), _ = match_box(lines, cards, "player", None)
    assert match.display(lines) == ["33% chance to be Sapped when Hit", "All Damage taken from Hits can Sap you"]


@pytest.mark.parametrize("data, ocr, shown", [
    ("Create Shocked Ground on Death, lasting 3 seconds",
     "100% CHANCE TO CREATE SHOCKED GROUND ON DEATH, LASTING 3 SECONDS",
     "Create Shocked Ground on Death, lasting 3 seconds"),
    ("(1.6-3.2)% chance to drop an additional Orb of Fusing", "4,3% CHANCE TO DROP AN ADDITIONAL ORB OF FUSING",
     "4.3% chance to drop an additional Orb of Fusing"),
    ("-(60-40)% to Cold Resistance", "-52% TO COLD RESISTANCE", "-52% to Cold Resistance"),
])
def test_fill_values_aligns_by_words(data, ocr, shown):
    from prototype.match import _fill_values
    assert _fill_values(data, ocr) == shown
