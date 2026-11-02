import pytest

from prototype.verdict import (ADAPT, AVOID, CONSIDER, NEUTRAL, TAKE, OptionInput, Profile, judge,
                               recommend)

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
    assert (v.up_tier, v.up_pos, v.down_tier, v.verdict) == ("s", 1, "brick", ADAPT)


def options(top, bottom):
    opts = {"top": top, "bottom": bottom}
    return opts, {k: judge(o, PROFILE) for k, o in opts.items()}


def test_take_prefers_higher_tier_then_column_position():
    rec = recommend(*options(OptionInput("player", ["u_a"], []), OptionInput("boss", ["u_s2"], [])))
    assert rec.choice == "bottom"
    rec = recommend(*options(OptionInput("player", ["u_s2"], []), OptionInput("boss", ["u_s1"], [])))
    assert rec.choice == "bottom"


def test_risky_is_your_call_with_minions_fallback():
    rec = recommend(*options(OptionInput("player", ["u_s1"], ["d_dan"]), OptionInput("minions", ["u_x"], [])))
    assert rec.choice is None
    assert "Your call: top" in rec.text and "bottom option (Minions)" in rec.text


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
    assert profile.read_only and profile.upsides["s"]


def test_unreadable_option_blocks_the_recommendation():
    from prototype.verdict import UNREADABLE
    rec_input = options(OptionInput(None, [], []), OptionInput("player", ["u_s1"], []))
    assert rec_input[1]["top"].verdict == UNREADABLE
    assert recommend(*rec_input).text == "Can't read both options yet."
