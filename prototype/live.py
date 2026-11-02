"""Live scanning: frame sources, altar tracking and verdicts, independent of the UI."""

from __future__ import annotations

import time
from dataclasses import dataclass, field
from pathlib import Path
from typing import Callable, Iterator

import cv2
import numpy as np

from .detect import find_icons
from .match import Card
from .ocr import WindowsOcr
from .read import Altar, read_altar
from .verdict import OptionInput, OptionVerdict, Profile, Recommendation, judge, recommend

TRACK_DISTANCE = 250   # px an icon may move between scans (camera follows the character)
TRACK_TIMEOUT = 1.5    # s without seeing an icon before its altar is dropped
REREAD_INTERVAL = 0.5  # s between reads of an altar that is not completely readable yet


class ScreenSource:
    """Captures one monitor with mss (primary monitor by default)."""

    def __init__(self, monitor: int | None = None):
        import mss

        self._mss = mss.MSS()
        monitors = self._mss.monitors[1:]
        self.monitor = monitors[monitor] if monitor is not None else next(
            (m for m in monitors if m.get("is_primary")), monitors[0])

    def grab(self) -> np.ndarray:
        return np.asarray(self._mss.grab(self.monitor))   # BGRA; consumers convert only what they need


class ReplaySource:
    """Cycles through screenshots, each shown for `hold` seconds; for testing without the game."""

    def __init__(self, paths: list[Path], hold: float = 3.0):
        self._frames = [cv2.imread(str(p)) for p in paths]
        self._hold = hold
        self._start = time.monotonic()
        self.monitor = {"left": 0, "top": 0, "width": self._frames[0].shape[1], "height": self._frames[0].shape[0]}

    def grab(self) -> np.ndarray:
        index = int((time.monotonic() - self._start) / self._hold) % len(self._frames)
        return self._frames[index]


@dataclass
class TrackedAltar:
    altar: Altar
    last_seen: float
    last_read: float
    complete: bool


@dataclass
class AltarView:
    """What the HUD shows for one altar."""
    altar: Altar
    complete: bool
    options: dict[str, OptionInput]
    verdicts: dict[str, OptionVerdict]
    recommendation: Recommendation


@dataclass
class Scanner:
    source: object
    cards: list[Card]
    profile_path: Callable[[], Path]   # asked on every scan, so switching profiles applies instantly
    ocr: WindowsOcr = field(default_factory=WindowsOcr)
    tracks: list[TrackedAltar] = field(default_factory=list)
    _profile: Profile | None = None
    _profile_key: tuple = ()

    def profile(self) -> Profile:
        """Reloads the profile when the file changed, so board edits apply instantly."""
        path = self.profile_path()
        key = (path, path.stat().st_mtime)
        if self._profile is None or key != self._profile_key:
            self._profile, self._profile_key = Profile.load(path), key
        return self._profile

    def scan(self) -> list[AltarView]:
        img = self.source.grab()
        now = time.monotonic()
        for icon in find_icons(img):
            track = min(self.tracks, default=None,
                        key=lambda t: abs(t.altar.icon.x - icon.x) + abs(t.altar.icon.y - icon.y))
            if track and abs(track.altar.icon.x - icon.x) + abs(track.altar.icon.y - icon.y) > TRACK_DISTANCE:
                track = None
            if track and (track.complete or now - track.last_read < REREAD_INTERVAL):
                track.last_seen = now
                track.altar.icon = icon   # follow the camera
                continue
            altar = read_altar(img, icon, self.ocr, self.cards)
            if altar is None:
                continue
            complete = all(o.complete for o in altar.options)
            if track:
                track.altar, track.last_seen, track.last_read, track.complete = altar, now, now, complete
            else:
                self.tracks.append(TrackedAltar(altar, now, now, complete))
        self.tracks = [t for t in self.tracks if now - t.last_seen <= TRACK_TIMEOUT]
        return [self._view(t) for t in self.tracks]

    def _view(self, track: TrackedAltar) -> AltarView:
        profile = self.profile()
        options = {
            o.position: OptionInput(
                o.target,
                [m.card.id for m in o.matches if m.card.side == "upside"],
                [m.card.id for m in o.matches if m.card.side == "downside"],
            )
            for o in track.altar.options
        }
        verdicts = {pos: judge(opt, profile) for pos, opt in options.items()}
        return AltarView(track.altar, track.complete, options, verdicts, recommend(options, verdicts))


def frames(scanner: Scanner, hz: float) -> Iterator[list[AltarView]]:
    """Scan at a fixed rate; yields the current altar views after every scan."""
    period = 1.0 / hz
    while True:
        start = time.monotonic()
        yield scanner.scan()
        time.sleep(max(0.0, period - (time.monotonic() - start)))
