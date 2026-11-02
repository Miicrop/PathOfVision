"""Short HUD labels per card (data/labels.json, kept in sync by tools/gen_mods) and keyword colors
(data/keywords.json). Both files are edited by hand."""

from __future__ import annotations

import html
import json
import re
from dataclasses import dataclass
from pathlib import Path

from .match import VALUE_SLOT_RE

LABELS_PATH = Path(__file__).resolve().parents[1] / "data" / "labels.json"
KEYWORDS_PATH = LABELS_PATH.with_name("keywords.json")
PLACEHOLDER_RE = re.compile(r"#(\d+)?")   # "#" = next value, "#2" = second value


@dataclass(frozen=True)
class Label:
    alias: str = ""
    icon: str = ""


def load_labels(path: Path = LABELS_PATH) -> dict[str, Label]:
    try:
        entries = json.loads(path.read_text(encoding="utf-8"))["labels"]
    except (FileNotFoundError, json.JSONDecodeError, KeyError):
        return {}
    return {cid: Label(e.get("alias", "").strip(), e.get("icon", "").strip()) for cid, e in entries.items()}


def render_alias(alias: str, display_lines: list[str]) -> str:
    """Fill the alias placeholders with the values of the displayed mod text.

    Values not read in game stay as their range ("(15-30)"); placeholders without a value stay "#".
    """
    values = [v for line in display_lines for v in VALUE_SLOT_RE.findall(line)]
    next_slot = iter(range(len(values)))

    def fill(m: re.Match) -> str:
        n = int(m.group(1)) - 1 if m.group(1) else next(next_slot, None)
        return values[n] if n is not None and 0 <= n < len(values) else m.group(0)

    return PLACEHOLDER_RE.sub(fill, alias)


def load_keywords(path: Path = KEYWORDS_PATH) -> dict[str, str]:
    """Keyword -> color for the HUD text (data/keywords.json, edited by hand)."""
    try:
        return dict(json.loads(path.read_text(encoding="utf-8"))["keywords"])
    except (FileNotFoundError, json.JSONDecodeError, KeyError):
        return {}


def color_keywords(text: str, keywords: dict[str, str]) -> str:
    """HTML of the text with every keyword (whole word, any case) wrapped in its color."""
    escaped = html.escape(text)
    if not keywords:
        return escaped
    colors = {k.lower(): c for k, c in keywords.items()}
    pattern = re.compile(r"\b(" + "|".join(re.escape(html.escape(k)) for k in
                                         sorted(keywords, key=len, reverse=True)) + r")\b", re.IGNORECASE)
    return pattern.sub(lambda m: f'<span style="color:{colors[html.unescape(m.group(1)).lower()]}">{m.group(1)}</span>',
                       escaped)
