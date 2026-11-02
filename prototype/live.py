"""Live scanning: frame sources, altar tracking and verdicts, independent of the UI."""

from __future__ import annotations

import time
from collections import Counter
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
SAME_ALTAR_OVERLAP = 0.6   # share of mods two reads must have in common to be the same altar


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
        unclaimed = list(self.tracks)
        for icon in find_icons(img):
            track = min(unclaimed, default=None, key=lambda t: _distance(t.altar, icon))
            if track and _distance(track.altar, icon) > TRACK_DISTANCE:
                track = None
            if track:
                unclaimed.remove(track)
                track.last_seen = now
                track.altar.icon = icon   # follow the camera
                if track.complete or now - track.last_read < REREAD_INTERVAL:
                    continue
            altar = read_altar(img, icon, self.ocr, self.cards)
            if altar is None:
                continue
            # Running moves the icon further than TRACK_DISTANCE between scans, so an altar
            # is also recognized by its content: the same mods on the same positions.
            if track is None:
                track = next((t for t in self.tracks if _same_altar(t.altar, altar)), None)
            if track is None:
                self.tracks.append(TrackedAltar(altar, now, now, _complete(altar)))
                continue
            track.last_seen, track.last_read = now, now
            if _quality(altar) >= _quality(track.altar):
                track.altar, track.complete = altar, _complete(altar)
            else:
                track.altar.icon = altar.icon
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


def _distance(altar: Altar, icon) -> int:
    return abs(altar.icon.x - icon.x) + abs(altar.icon.y - icon.y)


def _complete(altar: Altar) -> bool:
    return all(o.complete for o in altar.options)


def _quality(altar: Altar) -> tuple:
    """Which of two reads of the same altar is better: complete, more mods, fewer partial ones."""
    matches = [m for o in altar.options for m in o.matches]
    return _complete(altar), len(matches), -sum(m.partial for m in matches), sum(m.score for m in matches)


def _same_altar(a: Altar, b: Altar, min_overlap: float = SAME_ALTAR_OVERLAP) -> bool:
    """Same mods on the same positions; values are ignored since partly hidden reads differ."""
    sig = lambda altar: Counter((o.position, m.card.id) for o in altar.options for m in o.matches)
    sa, sb = sig(a), sig(b)
    smaller = min(sum(sa.values()), sum(sb.values()))
    return smaller > 0 and sum((sa & sb).values()) / smaller >= min_overlap


def frames(scanner: Scanner, hz: float) -> Iterator[list[AltarView]]:
    """Scan at a fixed rate; yields the current altar views after every scan."""
    period = 1.0 / hz
    while True:
        start = time.monotonic()
        yield scanner.scan()
        time.sleep(max(0.0, period - (time.monotonic() - start)))
