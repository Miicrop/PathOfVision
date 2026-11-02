from prototype.labels import color_keywords, render_alias

RARITY = ["27% increased Rarity of Items found in this Area"]
SAP = ["32% chance to be Sapped when Hit", "All Damage taken from Hits can Sap you"]


def test_alias_takes_the_read_values_in_order():
    assert render_alias("Rarity +#%", RARITY) == "Rarity +27%"
    assert render_alias("Sap #%", SAP) == "Sap 32%"


def test_numbered_placeholders_pick_a_value():
    lines = ["Final Boss drops 3 additional Orbs of Fusing", "lasting 100 seconds"]
    assert render_alias("Fusing ×#1 (#2 s)", lines) == "Fusing ×3 (100 s)"


def test_unread_values_show_the_range_and_missing_ones_stay():
    assert render_alias("Rarity +#%", ["(15-30)% increased Rarity of Items found in this Area"]) == "Rarity +(15-30)%"
    assert render_alias("Sap", SAP) == "Sap"
    assert render_alias("# and #", RARITY) == "27 and #"


def test_keywords_are_colored_as_whole_words_in_any_case():

    colors = {"Fire": "#f00", "Cold": "#00f"}
    out = color_keywords("Phys as Extra fire & Cold <x> Firestorm", colors)
    assert '<span style="color:#f00">fire</span>' in out
    assert '<span style="color:#00f">Cold</span>' in out
    assert "&amp;" in out and "&lt;x&gt;" in out      # text is escaped
    assert "Firestorm" in out and out.count("<span") == 2
