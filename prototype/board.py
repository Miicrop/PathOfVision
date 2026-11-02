"""Settings board: hosts ui/board in a Qt WebEngine window and gives it file access.

Usage: python -m prototype.board

The board talks to Python only through BoardApi (a QWebChannel object). Every change
is written to data/profiles/*.json or settings.json right away; the scanner picks it up
on its next scan.
"""

from __future__ import annotations

import json
import sys

from PySide6.QtCore import QObject, QUrl, Slot
from PySide6.QtWebChannel import QWebChannel
from PySide6.QtWebEngineCore import QWebEnginePage
from PySide6.QtWebEngineWidgets import QWebEngineView
from PySide6.QtWidgets import QApplication

from . import settings
from .settings import PROFILES_DIR, ROOT
from .verdict import upgrade_upsides

BOARD_HTML = ROOT / "ui" / "board" / "index.html"
MODS_PATH = ROOT / "data" / "mods.json"


class BoardApi(QObject):
    """Slots take and return JSON strings to keep the bridge format-neutral (WebView2 later)."""

    def _profile(self, file: str) -> dict:
        """A profile with its upsides in the column format (older files are upgraded on read)."""
        profile = json.loads((PROFILES_DIR / file).read_text(encoding="utf-8"))
        profile["upsides"] = upgrade_upsides(profile.get("upsides", {}))
        profile.setdefault("downsides", {})
        return profile

    def _profiles(self) -> list[dict]:
        out = []
        for path in sorted(PROFILES_DIR.glob("*.json")):
            data = json.loads(path.read_text(encoding="utf-8"))
            out.append({"file": path.name, "name": data["name"], "readOnly": data.get("readOnly", False)})
        return sorted(out, key=lambda p: (not p["readOnly"], p["name"].lower()))

    @Slot(result=str)
    def getState(self) -> str:
        mods = json.loads(MODS_PATH.read_text(encoding="utf-8"))
        return json.dumps({
            "gameVersion": mods["gameVersion"],
            "mods": [{k: m[k] for k in ("id", "side", "target", "altars", "lines", "weights")} for m in mods["mods"]],
            "profiles": self._profiles(),
            "active": settings.active_profile_path().name,
        })

    @Slot(str, result=str)
    def loadProfile(self, file: str) -> str:
        return json.dumps(self._profile(file))

    @Slot(str, str, result=str)
    def saveProfile(self, file: str, profile_json: str) -> str:
        if self._profile(file).get("readOnly"):
            return json.dumps({"ok": False, "error": "read-only profile"})
        profile = json.loads(profile_json)
        profile["readOnly"] = False
        (PROFILES_DIR / file).write_text(json.dumps(profile, indent=2) + "\n", encoding="utf-8")
        return json.dumps({"ok": True})

    @Slot(str, str, result=str)
    def duplicateProfile(self, file: str, name: str) -> str:
        profile = self._profile(file)
        profile.update(name=name, readOnly=False)
        new_file = settings.profile_file_name(name)
        (PROFILES_DIR / new_file).write_text(json.dumps(profile, indent=2) + "\n", encoding="utf-8")
        self.setActive(new_file)
        return json.dumps({"file": new_file, "profiles": self._profiles()})

    @Slot(str, result=str)
    def deleteProfile(self, file: str) -> str:
        if self._profile(file).get("readOnly"):
            return json.dumps({"ok": False, "error": "read-only profile"})
        (PROFILES_DIR / file).unlink()
        if settings.load()["profile"] == file:
            self.setActive(settings.DEFAULTS["profile"])
        return json.dumps({"ok": True, "profiles": self._profiles(), "active": settings.active_profile_path().name})

    @Slot(str)
    def setActive(self, file: str) -> None:
        current = settings.load()
        current["profile"] = file
        settings.save(current)

    @Slot(result=str)
    def getSettings(self) -> str:
        current = settings.load()
        return json.dumps({"current": {k: settings.clean(k, current[k]) for k in settings.HUD_KEYS},
                           "defaults": {k: settings.DEFAULTS[k] for k in settings.HUD_KEYS}})

    @Slot(str, result=str)
    def saveSettings(self, changes_json: str) -> str:
        """Merge HUD settings into settings.json; the overlay reloads the file on its own."""
        current = settings.load()
        for key, value in json.loads(changes_json).items():
            if key in settings.HUD_KEYS:
                current[key] = settings.clean(key, value)
        settings.save(current)
        return json.dumps({"ok": True, "current": {k: current[k] for k in settings.HUD_KEYS}})


class _Page(QWebEnginePage):
    def javaScriptConsoleMessage(self, level, message, line, source):
        print(f"[board] {message} ({source}:{line})", file=sys.stderr)


class BoardWindow(QWebEngineView):
    def __init__(self):
        super().__init__()
        self.setWindowTitle("Altar Tier Board")
        self.resize(1500, 950)
        self.setPage(_Page(self))
        self.api = BoardApi()
        self.channel = QWebChannel(self.page())
        self.channel.registerObject("backend", self.api)
        self.page().setWebChannel(self.channel)
        self.load(QUrl.fromLocalFile(str(BOARD_HTML)))


def main() -> int:
    app = QApplication(sys.argv)
    window = BoardWindow()
    window.show()
    return app.exec()


if __name__ == "__main__":
    sys.exit(main())
