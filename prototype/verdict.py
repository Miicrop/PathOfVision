"""Verdict engine: rates altar options against a profile (see plans/Plan.md, section 6).

Pure logic without I/O besides loading a profile, so it can be ported 1:1.
"""

from __future__ import annotations

import json
from dataclasses import dataclass, field
from pathlib import Path

DOWN_TIERS = ("brick", "dangerous")        # worst first
# Upside columns, left to right: Unrated | user columns ... | top. Every column right of Unrated is
# rated (TAKE / CONSIDER); only the top column and user columns marked as key ("key": true) can make
# a Brick workable (ADAPT - BRICK).
TOP, RATED = "top", "rated"                # tier kinds for display

TAKE, CONSIDER, ADAPT, AVOID, NEUTRAL = "TAKE", "CONSIDER", "ADAPT - BRICK", "AVOID", "NEUTRAL"
UNREADABLE = "UNREADABLE"   # no mod of the option could be read


def upgrade_upsides(upsides: dict) -> dict:
    """Upsides in the column format; old profiles ({"s", "a", "b"}) become top = S, columns = [B, A]."""
    if "top" in upsides or "columns" in upsides:
        return {"columns": [{"name": c.get("name", ""), "ids": list(c.get("ids", []))} | ({"key": True} if c.get("key") else {})
                            for c in upsides.get("columns", [])],
                "top": {"name": upsides.get("top", {}).get("name", "S"), "ids": list(upsides.get("top", {}).get("ids", []))}}
    return {"columns": [{"name": "B", "ids": list(upsides.get("b", []))}, {"name": "A", "ids": list(upsides.get("a", []))}],
            "top": {"name": "S", "ids": list(upsides.get("s", []))}}


@dataclass
class Profile:
    name: str
    upsides: dict                         # {"columns": [{"name", "ids", "key"?}, ...], "top": {"name", "ids"}}
    downsides: dict[str, list[str]]
    read_only: bool = False

    def __post_init__(self):
        self.upsides = upgrade_upsides(self.upsides)

    @classmethod
    def load(cls, path: Path) -> "Profile":
        data = json.loads(Path(path).read_text(encoding="utf-8"))
        return cls(data["name"], data.get("upsides", {}), data.get("downsides", {}), data.get("readOnly", False))

    def up_tier(self, mod_id: str) -> tuple[str | None, int, int]:
        """(kind, rank, position in its column). Rank grows to the right: unrated 0, top highest.
        A key column has the kind TOP (same treatment as the top column) but keeps its rank."""
        columns = self.upsides["columns"]
        top = self.upsides["top"]["ids"]
        if mod_id in top:
            return TOP, len(columns) + 1, top.index(mod_id)
        for i in range(len(columns) - 1, -1, -1):
            if mod_id in columns[i]["ids"]:
                return TOP if columns[i].get("key") else RATED, i + 1, columns[i]["ids"].index(mod_id)
        return None, 0, 10**6

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
    up_tier: str | None          # kind of the best upside: TOP, RATED or None (unrated)
    up_pos: int                  # position of that upside in its column (lower = higher priority)
    down_tier: str | None        # worst downside tier
    tiers: dict[str, str | None] = field(default_factory=dict)   # mod id -> TOP / RATED / dangerous / brick
    up_rank: int = 0             # column rank of the best upside (higher = further right)


def judge(option: OptionInput, profile: Profile) -> OptionVerdict:
    tiers: dict[str, str | None] = {}
    best_tier, best_rank, best_pos = None, 0, 10**6
    for mod_id in option.upsides:
        tier, rank, pos = profile.up_tier(mod_id)
        tiers[mod_id] = tier
        if (rank, -pos) > (best_rank, -best_pos):
            best_tier, best_rank, best_pos = tier, rank, pos

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
        verdict = ADAPT if best_tier == TOP else AVOID
    elif worst == "dangerous":
        verdict = CONSIDER if rated else AVOID
    else:
        verdict = TAKE if rated else NEUTRAL
    return OptionVerdict(verdict, best_tier, best_pos, worst, tiers, best_rank)


@dataclass
class Recommendation:
    choice: str | None           # "top", "bottom" or None
    text: str
    about: str | None = None     # option the text talks about when there is no firm choice ("Your call")


def recommend(options: dict[str, OptionInput], verdicts: dict[str, OptionVerdict]) -> Recommendation:
    """Recommendation line for one altar; keys are "top" and "bottom"."""
    def strength(pos: str):
        v = verdicts[pos]
        return v.up_rank, -v.up_pos

    def minions_option(allowed: set[str]) -> str | None:
        return next((p for p, o in options.items() if o.target == "minions" and verdicts[p].verdict in allowed), None)

    if any(v.verdict == UNREADABLE for v in verdicts.values()):
        return Recommendation(None, "Can't read both options yet.")

    takes = [p for p, v in verdicts.items() if v.verdict == TAKE]
    if takes:
        best = max(takes, key=strength)
        # A CONSIDER whose upside sits in a column further right beats the safe pick: a Dangerous
        # downside only warns, so the better upside wins (the box still shows CONSIDER).
        other = "bottom" if best == "top" else "top"
        if verdicts[other].verdict == CONSIDER and verdicts[other].up_rank > verdicts[best].up_rank:
            best = other
        return Recommendation(best, f"Take the {best} option.")

    risky = [p for p, v in verdicts.items() if v.verdict in (CONSIDER, ADAPT)]
    if risky:
        best = max(risky, key=strength)
        # The HUD arrows point at `best`, so the text does not repeat which option it is.
        reason = "Brick downside, good upside." if verdicts[best].verdict == ADAPT else "Be careful."
        other = "bottom" if best == "top" else "top"
        tail = f" {other.title()} is safe." if verdicts[other].verdict == NEUTRAL else ""
        return Recommendation(None, reason + tail, about=best)

    if all(v.verdict == AVOID for v in verdicts.values()):
        return Recommendation(None, "Leave this altar alone.")

    # Minions is the safe default for new players; the text no longer says so (hidden for now).
    neutral = minions_option({NEUTRAL}) or next(p for p, v in verdicts.items() if v.verdict == NEUTRAL)
    return Recommendation(neutral, f"Nothing rated. Take the {neutral} option.")
