import pytest

from prototype.verdict import (ADAPT, AVOID, CONSIDER, NEUTRAL, TAKE, TOP, OptionInput, Profile, judge,
                               recommend, upgrade_upsides)

PROFILE = Profile(
    "test",
    upsides={"s": ["u_s1", "u_s2"], "a": ["u_a"], "b": ["u_b"]},
    downsides={"dangerous": ["d_dan"], "brick": ["d_brick"]},
)


@pytest.mark.parametrize("up, down, verdict", [
    ("u_s1", "d_brick", ADAPT),
    ("u_a", "d_brick", AVOID),
    ("u_unrated", "d_brick", AVOID),
    ("u_b", "d_dan", CONSIDER),
    ("u_s1", "d_dan", CONSIDER),
    ("u_unrated", "d_dan", AVOID),
    ("u_b", "d_fine", TAKE),
    ("u_unrated", "d_fine", NEUTRAL),
])
def test_verdict_table(up, down, verdict):
    assert judge(OptionInput("player", [up], [down]), PROFILE).verdict == verdict


def test_best_upside_and_worst_downside_count():
    v = judge(OptionInput("boss", ["u_b", "u_s2"], ["d_fine", "d_dan", "d_brick"]), PROFILE)
    assert (v.up_tier, v.up_pos, v.down_tier, v.verdict) == (TOP, 1, "brick", ADAPT)


def options(top, bottom):
    opts = {"top": top, "bottom": bottom}
    return opts, {k: judge(o, PROFILE) for k, o in opts.items()}


def test_take_prefers_higher_tier_then_column_position():
    rec = recommend(*options(OptionInput("player", ["u_a"], []), OptionInput("boss", ["u_s2"], [])))
    assert rec.choice == "bottom"
    rec = recommend(*options(OptionInput("player", ["u_s2"], []), OptionInput("boss", ["u_s1"], [])))
    assert rec.choice == "bottom"


def test_risky_is_your_call_with_safe_other_option():
    rec = recommend(*options(OptionInput("player", ["u_s1"], ["d_dan"]), OptionInput("boss", ["u_x"], [])))
    assert (rec.choice, rec.about) == (None, "top")
    assert rec.text == "Be careful. Bottom is safe."


def test_adapt_text_without_safe_option():
    rec = recommend(*options(OptionInput("player", ["u_x"], ["d_dan"]), OptionInput("boss", ["u_s1"], ["d_brick"])))
    assert (rec.choice, rec.about, rec.text) == (None, "bottom", "Brick downside, good upside.")


def test_both_avoid():
    rec = recommend(*options(OptionInput("player", ["u_x"], ["d_dan"]), OptionInput("boss", ["u_a"], ["d_brick"])))
    assert (rec.choice, rec.text) == (None, "Leave this altar alone.")


def test_neutral_takes_minions():
    rec = recommend(*options(OptionInput("player", ["u_x"], []), OptionInput("minions", ["u_y"], [])))
    assert rec.choice == "bottom"
    rec = recommend(*options(OptionInput("player", ["u_x"], []), OptionInput("boss", ["u_y"], ["d_dan"])))
    assert rec.choice == "top"


def test_default_profile_loads():
    from pathlib import Path
    profile = Profile.load(Path(__file__).resolve().parents[1] / "data" / "profiles" / "default.json")
    assert profile.read_only and profile.upsides["top"]["ids"]


def test_unreadable_option_blocks_the_recommendation():
    from prototype.verdict import UNREADABLE
    rec_input = options(OptionInput(None, [], []), OptionInput("player", ["u_s1"], []))
    assert rec_input[1]["top"].verdict == UNREADABLE
    assert recommend(*rec_input).text == "Can't read both options yet."


def test_old_profiles_are_upgraded_to_columns():
    up = upgrade_upsides({"s": ["x"], "a": ["y"], "b": ["z"]})
    assert up == {"columns": [{"name": "B", "ids": ["z"]}, {"name": "A", "ids": ["y"]}], "top": {"name": "S", "ids": ["x"]}}


def test_custom_columns_rank_by_position_and_only_top_adapts():
    profile = Profile("cols", {"columns": [{"name": "Meh", "ids": ["u_meh"]}, {"name": "Good", "ids": ["u_good"]},
                                           {"name": "Great", "ids": ["u_great"]}],
                               "top": {"name": "Jackpot", "ids": ["u_top"]}},
                      {"dangerous": [], "brick": ["d_brick"]})
    assert judge(OptionInput("boss", ["u_great"], ["d_brick"]), profile).verdict == AVOID
    assert judge(OptionInput("boss", ["u_top"], ["d_brick"]), profile).verdict == ADAPT
    assert judge(OptionInput("boss", ["u_meh"], []), profile).verdict == TAKE
    opts = {"top": OptionInput("boss", ["u_meh"], []), "bottom": OptionInput("player", ["u_great"], [])}
    assert recommend(opts, {k: judge(o, profile) for k, o in opts.items()}).choice == "bottom"
