"""Match OCR lines of one altar box against data/mods.json."""

from __future__ import annotations

import difflib
import itertools
import json
import re
from dataclasses import dataclass, field
from pathlib import Path

from rapidfuzz import fuzz

from tools.gen_mods.gen_mods import to_template

MODS_PATH = Path(__file__).resolve().parents[1] / "data" / "mods.json"
HEADERS = {"PLAYER": "player", "MINION": "minions", "BOSS": "boss"}
MIN_LINE_SCORE = 80
LINE_MARGIN = 6       # a line may also count for templates scoring at most this much below its best
NUMBER_RE = re.compile(r"\d+(?:\.\d+)?")
VALUE_SLOT_RE = re.compile(r"\(-?[\d.]+--?[\d.]+\)|\d+(?:\.\d+)?")   # "(15-30)" or "100"


def normalize(text: str) -> str:
    """Uppercase template of a line, robust to typical OCR slips."""
    text = text.upper().replace("—", "-").replace("–", "-")
    text = re.sub(r"(?<=\d)[O](?=[\d%])|(?<=[\d.])O\b", "0", text)   # 1O% -> 10%
    text = re.sub(r"(?<=\d) +(?=%)", "", text)                       # 33 % -> 33%
    text = re.sub(r"(?<=\d),(?=\d)", ".", text)                       # 2,5% -> 2.5%
    return re.sub(r"\s+", " ", to_template(text)).strip()


def _fill_values(data_line: str, ocr_line: str) -> str:
    """Put the numbers read in game into the data text's value slots.

    Slots are matched to OCR numbers by the words around them, because the two texts do
    not always have the same numbers ("100% chance to create Shocked Ground ... 3 seconds"
    in game vs. "Create Shocked Ground ... 3 seconds" in the data).
    """
    ocr_line = re.sub(r"(?<=\d),(?=\d)", ".", ocr_line)   # 4,3% -> 4.3%
    data_tokens = VALUE_SLOT_RE.split(data_line)             # text pieces between the slots
    slots = VALUE_SLOT_RE.findall(data_line)
    tokenize = lambda text: [w for w in re.split(r"[^A-Z#]+", VALUE_SLOT_RE.sub(" # ", text).upper()) if w]
    data_words, ocr_words = tokenize(data_line), tokenize(ocr_line)
    ocr_values = iter(NUMBER_RE.findall(ocr_line))
    ocr_slot_values = [next(ocr_values) if w == "#" else None for w in ocr_words]

    filled: dict[int, str] = {}
    slot_at = [k for k, w in enumerate(data_words) if w == "#"]   # data word index of each slot
    for block in difflib.SequenceMatcher(None, data_words, ocr_words, autojunk=False).get_matching_blocks():
        for offset in range(block.size):
            d, o = block.a + offset, block.b + offset
            if data_words[d] == "#" and d in slot_at:
                filled[slot_at.index(d)] = ocr_slot_values[o]
    out = data_tokens[0]
    for n, slot in enumerate(slots):
        out += filled.get(n) or slot
        out += data_tokens[n + 1]
    return out


def parse_header(text: str) -> str | None:
    """'PLAYER GAINS:' -> 'player'. None if the line is not a header.

    Headers are short ("MAP BOSS GAINS:"); mod lines may contain "gain" too but are longer.
    The keyword check is fuzzy because the header is often partly hidden ("MINIONS GAIN:").
    """
    up = text.upper().strip()
    if len(up) > 28 or fuzz.partial_ratio("GAIN", up) < 75:
        return None
    best = max(HEADERS, key=lambda k: fuzz.partial_ratio(k, up))
    return HEADERS[best] if fuzz.partial_ratio(best, up) >= 80 else None


@dataclass
class Card:
    id: str
    side: str
    target: str
    altars: list[str]
    templates: list[list[str]]          # per variant: normalized template lines
    ranges: list[list[list[float]]]     # per variant
    lines: list[list[str]] = field(default_factory=list)   # per variant: display text with ranges


def load_cards(path: Path = MODS_PATH) -> list[Card]:
    mods = json.loads(path.read_text(encoding="utf-8"))["mods"]
    return [
        Card(
            id=m["id"], side=m["side"], target=m["target"], altars=m["altars"],
            templates=[[normalize(t) for t in v["templates"]] for v in m["variants"]],
            ranges=[v["ranges"] for v in m["variants"]],
            lines=[v["lines"] for v in m["variants"]],
        )
        for m in mods
    ]


@dataclass
class Match:
    card: Card
    lines: list[int]                     # indices into the OCR lines, in template order
    score: float
    values: list[float] = field(default_factory=list)
    in_range: bool = True                # informational: atlas passives push values past the base range
    partial: bool = False                # some lines of a multi-line mod were not readable
    variant: int = 0
    assign: list[int | None] = field(default_factory=list)   # OCR line per template line, None if hidden

    def display(self, ocr_lines: list[str]) -> list[str]:
        """The mod's clean text with the rolled values filled in where they were read."""
        return [_fill_values(text, ocr_lines[i]) if i is not None else text
                for text, i in zip(self.card.lines[self.variant], self.assign)]


def _values(line: str) -> list[float]:
    return [float(n) for n in NUMBER_RE.findall(line)]


def _candidates(norm: list[str], pool: list[Card]):
    """Every way a card's variant can explain some OCR lines: (card, variant, line per template or None, score)."""
    scores = {}   # (card index, variant, template line, ocr line) -> score
    best_for_line = [0.0] * len(norm)
    for ci, card in enumerate(pool):
        for vi, tpls in enumerate(card.templates):
            for ti, tpl in enumerate(tpls):
                for i, n in enumerate(norm):
                    s = fuzz.ratio(tpl, n)
                    if s >= MIN_LINE_SCORE:
                        scores[ci, vi, ti, i] = s
                        best_for_line[i] = max(best_for_line[i], s)

    out = []
    for ci, card in enumerate(pool):
        for vi, tpls in enumerate(card.templates):
            options = []   # per template line: [(ocr line index, score)]
            for ti in range(len(tpls)):
                # Near-identical lines ("+#% TO COLD/FIRE RESISTANCE") would explode the
                # search, so a line only counts for templates close to its best match.
                opts = [(i, s) for i in range(len(norm))
                        if (s := scores.get((ci, vi, ti, i), 0)) and s >= best_for_line[i] - LINE_MARGIN]
                # A template line may stay unmatched (hidden behind HUD) only if nothing matches it.
                options.append(opts or [(None, 0.0)])
            for combo in itertools.product(*options):
                idx = [i for i, _ in combo if i is not None]
                if not idx or len(idx) != len(set(idx)):
                    continue
                score = sum(s for _, s in combo) / len(tpls)   # missing lines count as 0
                out.append((card, vi, [i for i, _ in combo], score))
    return out


def match_box(lines: list[str], cards: list[Card], target: str | None, altar: str | None) -> tuple[list[Match], list[int]]:
    """Assign OCR lines to cards so that every line is used at most once.

    Lines of a multi-line mod are not shown in data order and can be interleaved
    with other mods, so this is an exact-cover search over the box's lines.
    Prefers covering more lines, then fewer partially visible mods, then higher scores.
    Without a target (header not readable) every target is tried.
    Returns the matches and the indices of lines nothing explained.
    """
    if target is None:
        results = [match_box(lines, cards, t, altar) for t in ("player", "minions", "boss")]
        return max(results, key=lambda r: (len(lines) - len(r[1]), sum(m.score for m in r[0])))

    norm = [normalize(l) for l in lines]
    pool = [c for c in cards if c.target == target and (altar is None or altar in c.altars)]
    candidates = sorted(_candidates(norm, pool), key=lambda c: -c[3])
    best_key, best_set = (0, 0, 0.0), []

    def search(start: int, used: set[int], chosen: list):
        nonlocal best_key, best_set
        partials = sum(1 for c in chosen if None in c[2])
        key = (len(used), -partials, sum(c[3] for c in chosen))
        if key > best_key:
            best_key, best_set = key, list(chosen)
        if len(used) == len(norm):
            return
        for k in range(start, len(candidates)):
            cand = candidates[k]
            idx = {i for i in cand[2] if i is not None}
            if used.isdisjoint(idx):   # the same mod can roll twice on one option
                chosen.append(cand)
                search(k + 1, used | idx, chosen)
                chosen.pop()

    search(0, set(), [])

    matches = []
    for card, vi, assign, score in best_set:
        present = [i for i in assign if i is not None]
        values = [v for i in present for v in _values(lines[i])]
        # OCR values carry no sign, so compare magnitudes
        bounds = [sorted((abs(lo), abs(hi))) for lo, hi in card.ranges[vi]]
        in_range = len(values) == len(bounds) and all(lo <= v <= hi for v, (lo, hi) in zip(values, bounds))
        matches.append(Match(card, present, round(score, 1), values, in_range,
                             partial=len(present) < len(assign), variant=vi, assign=assign))
    matches.sort(key=lambda m: min(m.lines))
    unexplained = sorted(set(range(len(lines))) - {i for m in matches for i in m.lines})
    return matches, unexplained
