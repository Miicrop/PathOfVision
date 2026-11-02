"""Verdict engine: rates altar options against a profile (see plans/Plan.md, section 6).

Pure logic without I/O besides loading a profile, so it can be ported 1:1.
"""

from __future__ import annotations

import json
from dataclasses import dataclass, field
from pathlib import Path

UP_TIERS = ("s", "a", "b")                 # best first
DOWN_TIERS = ("brick", "dangerous")        # worst first
UP_RANK = {"s": 3, "a": 2, "b": 1, None: 0}

TAKE, CONSIDER, ADAPT, AVOID, NEUTRAL = "TAKE", "CONSIDER", "ADAPT - BRICK", "AVOID", "NEUTRAL"
UNREADABLE = "UNREADABLE"   # no mod of the option could be read


@dataclass
class Profile:
    name: str
    upsides: dict[str, list[str]]
    downsides: dict[str, list[str]]
    read_only: bool = False

    @classmethod
    def load(cls, path: Path) -> "Profile":
        data = json.loads(Path(path).read_text(encoding="utf-8"))
        return cls(data["name"], data.get("upsides", {}), data.get("downsides", {}), data.get("readOnly", False))

    def up_tier(self, mod_id: str) -> tuple[str | None, int]:
        """(tier, position in its column); unrated mods get (None, large)."""
        for tier in UP_TIERS:
            ids = self.upsides.get(tier, [])
            if mod_id in ids:
                return tier, ids.index(mod_id)
        return None, 10**6

    def down_tier(self, mod_id: str) -> str | None:
        for tier in DOWN_TIERS:
            if mod_id in self.downsides.get(tier, []):
                return tier
        return None


@dataclass
class OptionInput:
    target: str | None
    upsides: list[str]
    downsides: list[str]


@dataclass
class OptionVerdict:
    verdict: str
    up_tier: str | None          # best upside tier
    up_pos: int                  # position of that upside in its column (lower = higher priority)
    down_tier: str | None        # worst downside tier
    tiers: dict[str, str | None] = field(default_factory=dict)   # mod id -> tier, for display


def judge(option: OptionInput, profile: Profile) -> OptionVerdict:
    tiers: dict[str, str | None] = {}
    best_tier, best_pos = None, 10**6
    for mod_id in option.upsides:
        tier, pos = profile.up_tier(mod_id)
        tiers[mod_id] = tier
        if (UP_RANK[tier], -pos) > (UP_RANK[best_tier], -best_pos):
            best_tier, best_pos = tier, pos

    worst = None
    for mod_id in option.downsides:
        tier = profile.down_tier(mod_id)
        tiers[mod_id] = tier
        if tier == "brick" or (tier == "dangerous" and worst is None):
            worst = tier

    rated = best_tier is not None
    if not option.upsides and not option.downsides:
        verdict = UNREADABLE
    elif worst == "brick":
        verdict = ADAPT if best_tier == "s" else AVOID
    elif worst == "dangerous":
        verdict = CONSIDER if rated else AVOID
    else:
        verdict = TAKE if rated else NEUTRAL
    return OptionVerdict(verdict, best_tier, best_pos, worst, tiers)


@dataclass
class Recommendation:
    choice: str | None           # "top", "bottom" or None
    text: str
    about: str | None = None     # option the text talks about when there is no firm choice ("Your call")


def recommend(options: dict[str, OptionInput], verdicts: dict[str, OptionVerdict]) -> Recommendation:
    """Recommendation line for one altar; keys are "top" and "bottom"."""
    def strength(pos: str):
        v = verdicts[pos]
        return UP_RANK[v.up_tier], -v.up_pos

    def minions_option(allowed: set[str]) -> str | None:
        return next((p for p, o in options.items() if o.target == "minions" and verdicts[p].verdict in allowed), None)

    if any(v.verdict == UNREADABLE for v in verdicts.values()):
        return Recommendation(None, "Can't read both options yet.")

    takes = [p for p, v in verdicts.items() if v.verdict == TAKE]
    if takes:
        best = max(takes, key=strength)
        return Recommendation(best, f"Take the {best} option.")

    risky = [p for p, v in verdicts.items() if v.verdict in (CONSIDER, ADAPT)]
    if risky:
        best = max(risky, key=strength)
        reason = "Brick downside, needs build adjustments" if verdicts[best].verdict == ADAPT else "check the Dangerous downside"
        fallback = minions_option({NEUTRAL})
        tail = f" Safe fallback: the {fallback} option (Minions)." if fallback and fallback != best else ""
        return Recommendation(None, f"Your call: {best} option, {reason}.{tail}", about=best)

    if all(v.verdict == AVOID for v in verdicts.values()):
        return Recommendation(None, "Leave this altar alone.")

    neutral = minions_option({NEUTRAL}) or next(p for p, v in verdicts.items() if v.verdict == NEUTRAL)
    who = " (Minions)" if options[neutral].target == "minions" else ""
    return Recommendation(neutral, f"Nothing rated. Take the {neutral} option{who}.")
