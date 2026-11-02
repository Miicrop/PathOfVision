"""User settings (settings.json in the project root, not in git) and profile files."""

from __future__ import annotations

import json
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SETTINGS_PATH = ROOT / "settings.json"
PROFILES_DIR = ROOT / "data" / "profiles"
# HUD: centered at the bottom; hud_bottom = px between the HUD and the bottom edge (above the skill bar).
# hud_font_size scales text and icons. hud_tiers: "off", "text", "highlight" (row tinted in the tier
# color) or "both". hud_other_opacity: 0.1-1, how visible the option that is not the pick stays
# (1 = no dimming; an ADAPT - BRICK option is never dimmed). hud_values: column with the rolled values.
DEFAULTS = {"profile": "default.json", "hud_bottom": 60, "hud_offset_x": 0,
            "hud_font_size": 15, "hud_tiers": "text", "hud_other_opacity": 0.6,
            "hud_values": True}
TIER_STYLES = ("off", "text", "highlight", "both")


def load() -> dict:
    try:
        return {**DEFAULTS, **json.loads(SETTINGS_PATH.read_text(encoding="utf-8"))}
    except (FileNotFoundError, json.JSONDecodeError):
        return dict(DEFAULTS)


def save(settings: dict) -> None:
    SETTINGS_PATH.write_text(json.dumps(settings, indent=2) + "\n", encoding="utf-8")


def active_profile_path() -> Path:
    path = PROFILES_DIR / load()["profile"]
    return path if path.exists() else PROFILES_DIR / DEFAULTS["profile"]


def profile_file_name(name: str) -> str:
    """A free file name derived from a profile name: "My Scarab Farm" -> my-scarab-farm.json."""
    stem = re.sub(r"[^a-z0-9]+", "-", name.lower()).strip("-") or "profile"
    candidate, n = f"{stem}.json", 2
    while (PROFILES_DIR / candidate).exists():
        candidate, n = f"{stem}-{n}.json", n + 1
    return candidate
