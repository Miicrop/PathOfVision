from prototype.labels import render_alias

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
