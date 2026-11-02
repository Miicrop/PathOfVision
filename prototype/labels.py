"""Short HUD labels per card (data/labels.json, edited by hand, kept in sync by tools/gen_mods)."""

from __future__ import annotations

import json
import re
from dataclasses import dataclass
from pathlib import Path

from .match import VALUE_SLOT_RE

LABELS_PATH = Path(__file__).resolve().parents[1] / "data" / "labels.json"
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
