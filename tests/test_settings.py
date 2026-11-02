import json

from prototype import settings
from prototype.board import BoardApi


def test_clean_clamps_and_falls_back_to_defaults():
    assert settings.clean("hud_font_size", 99) == 24
    assert settings.clean("hud_font_size", "abc") == settings.DEFAULTS["hud_font_size"]
    assert settings.clean("hud_tiers", "loud") == settings.DEFAULTS["hud_tiers"]
    assert settings.clean("hud_other_opacity", 0.0) == 0.1
    assert settings.clean("hud_offset_x", -5000) == -3000


def test_board_saves_only_hud_keys_and_keeps_the_rest(tmp_path, monkeypatch):
    path = tmp_path / "settings.json"
    path.write_text(json.dumps({"profile": "mine.json", "hud_bottom": 60}), encoding="utf-8")
    monkeypatch.setattr(settings, "SETTINGS_PATH", path)
    api = BoardApi()
    result = json.loads(api.saveSettings(json.dumps({"hud_font_size": 30, "hud_values": False, "profile": "x.json"})))
    assert result["current"]["hud_font_size"] == 24 and result["current"]["hud_values"] is False
    saved = json.loads(path.read_text(encoding="utf-8"))
    assert saved["profile"] == "mine.json" and saved["hud_bottom"] == 60
    assert json.loads(api.getSettings())["current"]["hud_font_size"] == 24
