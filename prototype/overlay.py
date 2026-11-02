"""HUD overlay: a click-through, always-on-top panel at the bottom center that never takes focus.

Per altar it mirrors the in-game layout: top option box, recommendation box (where the
altar icon sits in game), bottom option box. Box borders carry the verdict color, and each
option shows its target as the matching half of the altar icon. Mods are listed in columns
(sign, icon, alias or full text, tier); aliases and icons come from data/labels.json.
"""

from __future__ import annotations

import ctypes
import html
import sys
from pathlib import Path

from PySide6.QtCore import QObject, Qt, Signal
from PySide6.QtGui import QPixmap
from PySide6.QtWidgets import QFrame, QGridLayout, QHBoxLayout, QLabel, QVBoxLayout, QWidget

from .labels import LABELS_PATH, Label, load_labels, render_alias
from .live import AltarView
from .match import Match
from .verdict import ADAPT, AVOID, CONSIDER, NEUTRAL, TAKE, UNREADABLE

ASSETS = Path(__file__).resolve().parent / "assets"
VERDICT_COLORS = {TAKE: "#4fbf6a", CONSIDER: "#e0a33a", ADAPT: "#d65db1", AVOID: "#e05a4f",
                  NEUTRAL: "#9aa0a6", UNREADABLE: "#5d6168"}
TIER_LABELS = {"s": ("S", "#f2c84b"), "a": ("A", "#b48cf0"), "b": ("B", "#6fa4dc"),
               "dangerous": ("Dangerous", "#e0a33a"), "brick": ("Brick", "#e05a4f")}
MAX_ALTARS = 2
MAX_LINE = 64
ICON_HEIGHT = 54
MOD_ICON_SIZE = 24
FONT_SIZE = 15
LEGEND_OVERLAP = 9   # px the verdict legend sticks out above the box border
BOX_STYLE = "QFrame#{name} {{ background: rgba(12, 14, 20, 228); border: 2px solid {color}; border-radius: 6px; }}"


class Bridge(QObject):
    """Carries scan results from the scanner thread to the UI thread."""
    views = Signal(list)


def _span(text: str, color: str, bold: bool = False) -> str:
    weight = "font-weight:600;" if bold else ""
    return f'<span style="color:{color};{weight}">{html.escape(text)}</span>'


def _label(rich: str, align=Qt.AlignLeft, size: int = FONT_SIZE) -> QLabel:
    label = QLabel(rich)
    label.setTextFormat(Qt.RichText)
    label.setAlignment(align | Qt.AlignVCenter)
    label.setStyleSheet(f"QLabel {{ background: transparent; color: #d8d8d8; font-family: 'Segoe UI'; font-size: {size}px; }}")
    return label


class _Icons:
    """Target icons: the game's altar icon halves (top/bottom), colored by altar type.

    Source: Art/2DArt/UIImages/InGame/AtlasAltar/4k/{Top,Bottom}Icon* plus the matching
    *IconFrame sprite, extracted from the game bundles.
    """

    def __init__(self):
        self._cache: dict[tuple[str, str, str], QPixmap] = {}

    def get(self, target: str | None, position: str, altar: str) -> QPixmap | None:
        if target is None:
            return None
        key = (target, position, altar)
        if key not in self._cache:
            pixmap = QPixmap(str(ASSETS / f"target_{target}_{position}_{altar}.png"))
            self._cache[key] = pixmap.scaledToHeight(ICON_HEIGHT, Qt.SmoothTransformation)
        return self._cache[key]

    def mod(self, name: str) -> QPixmap | None:
        """Mod icon from assets/mods/<name>.png; None if the file does not exist."""
        key = ("mod", name, "")
        if key not in self._cache:
            pixmap = QPixmap(str(ASSETS / "mods" / f"{name}.png"))
            self._cache[key] = None if pixmap.isNull() else pixmap.scaled(
                MOD_ICON_SIZE, MOD_ICON_SIZE, Qt.KeepAspectRatio, Qt.SmoothTransformation)
        return self._cache[key]


def mod_text(match: Match, ocr_lines: list[str], labels: dict[str, Label]) -> str:
    """The alias with the read values filled in, or the full mod text if the card has no alias."""
    display = match.display(ocr_lines)
    label = labels.get(match.card.id)
    text = render_alias(label.alias, display) if label and label.alias else " / ".join(display)
    return text if len(text) <= MAX_LINE else text[:MAX_LINE - 1] + "…"


def _option_box(view: AltarView, position: str, icons: _Icons, labels: dict[str, Label]) -> QWidget:
    option = next(o for o in view.altar.options if o.position == position)
    verdict = view.verdicts[position]
    color = VERDICT_COLORS[verdict.verdict]
    box = QFrame()
    box.setObjectName("opt")
    box.setStyleSheet(BOX_STYLE.format(name="opt", color=color))
    row = QHBoxLayout(box)
    row.setContentsMargins(10, 8, 12, 8)
    row.setSpacing(10)

    icon = QLabel()
    icon.setStyleSheet("background: transparent;")
    icon.setFixedWidth(int(ICON_HEIGHT * 2.1))
    icon.setAlignment(Qt.AlignCenter)
    pixmap = icons.get(option.target, position, view.altar.altar)
    if pixmap:
        icon.setPixmap(pixmap)
    row.addWidget(icon, 0, Qt.AlignVCenter)

    # One row per mod in columns: sign | mod icon | alias or text | tier
    grid = QGridLayout()
    grid.setHorizontalSpacing(6)
    grid.setVerticalSpacing(1)
    for r, m in enumerate(sorted(option.matches, key=lambda m: m.card.side != "upside")):
        sign = _span("+", "#4fbf6a", True) if m.card.side == "upside" else _span("−", "#e05a4f", True)
        grid.addWidget(_label(sign), r, 0)
        label = labels.get(m.card.id)
        mod_icon = icons.mod(label.icon) if label and label.icon else None
        if mod_icon:
            pic = QLabel()
            pic.setStyleSheet("background: transparent;")
            pic.setPixmap(mod_icon)
            grid.addWidget(pic, r, 1)
        grid.addWidget(_label(_span(mod_text(m, option.lines, labels), "#d8d8d8")), r, 2)
        tier, tier_color = TIER_LABELS.get(verdict.tiers.get(m.card.id),
                                           ("Unrated" if m.card.side == "upside" else "Fine", "#6f757d"))
        grid.addWidget(_label(_span(tier, tier_color, True), Qt.AlignRight), r, 3)
        grid.setRowMinimumHeight(r, MOD_ICON_SIZE)   # rows with and without icon look the same
    grid.setColumnMinimumWidth(1, MOD_ICON_SIZE)     # texts line up across boxes
    grid.setColumnStretch(2, 1)
    row.addLayout(grid, 1)
    return _with_legend(box, _span(verdict.verdict, color, True))


def _with_legend(box: QFrame, rich: str) -> QWidget:
    """Put a small label on the box's top-left border, like a fieldset legend."""
    wrapper = QWidget()
    wrapper.setStyleSheet("background: transparent;")
    layout = QVBoxLayout(wrapper)
    layout.setContentsMargins(0, LEGEND_OVERLAP, 0, 0)
    layout.addWidget(box)
    legend = _label(rich, size=FONT_SIZE - 3)
    legend.setParent(wrapper)
    legend.setStyleSheet(legend.styleSheet().replace("background: transparent", "background: rgb(12, 14, 20)")
                         + " QLabel { padding: 0 5px; border-radius: 3px; }")
    legend.adjustSize()
    legend.move(12, LEGEND_OVERLAP - legend.height() // 2 + 1)
    legend.raise_()
    return wrapper


def _recommendation_box(view: AltarView) -> QFrame:
    rec = view.recommendation
    color = VERDICT_COLORS[view.verdicts[rec.choice].verdict] if rec.choice else "#e0a33a"
    text = _span(rec.text, color, True)
    if not view.complete:
        text += "<br>" + _span("partly hidden, verdict may change", "#9aa0a6")
    box = QFrame()
    box.setObjectName("rec")
    box.setStyleSheet(BOX_STYLE.format(name="rec", color="#3a3f4b"))
    col = QVBoxLayout(box)
    col.setContentsMargins(12, 6, 12, 6)
    col.addWidget(_label(text, Qt.AlignHCenter))
    return box


def _altar_column(view: AltarView, icons: _Icons, labels: dict[str, Label]) -> QWidget:
    column = QWidget()
    column.setStyleSheet("background: transparent;")
    layout = QVBoxLayout(column)
    layout.setContentsMargins(0, 0, 0, 0)
    layout.setSpacing(6)
    layout.addWidget(_option_box(view, "top", icons, labels))
    layout.addWidget(_recommendation_box(view), 0, Qt.AlignHCenter)
    layout.addWidget(_option_box(view, "bottom", icons, labels))
    return column


def _signature(view: AltarView) -> tuple:
    """Everything the HUD shows; the widgets are rebuilt only when this changes."""
    return (view.altar.altar, view.complete, view.recommendation.text,
            tuple((o.position, o.target, view.verdicts[o.position].verdict,
                   tuple((m.card.id, tuple(m.display(o.lines)), view.verdicts[o.position].tiers.get(m.card.id))
                         for m in o.matches))
                  for o in view.altar.options))


class Hud(QWidget):
    def __init__(self, monitor: dict, bottom: int, offset_x: int = 0):
        super().__init__(None, Qt.FramelessWindowHint | Qt.WindowStaysOnTopHint | Qt.Tool
                         | Qt.WindowTransparentForInput | Qt.WindowDoesNotAcceptFocus)
        self.setAttribute(Qt.WA_TranslucentBackground)
        self.setAttribute(Qt.WA_ShowWithoutActivating)
        self._monitor, self._bottom, self._offset_x = monitor, bottom, offset_x
        self._icons = _Icons()
        self._labels: dict[str, Label] = {}
        self._labels_mtime: float | None = None
        self._shown: tuple = ()
        self._layout = QHBoxLayout(self)
        self._layout.setContentsMargins(0, 0, 0, 0)
        self._layout.setSpacing(16)

    def showEvent(self, event):
        super().showEvent(event)
        if sys.platform == "win32":
            # Belt and braces: make the window ignore mouse input and never activate.
            GWL_EXSTYLE, WS_EX_LAYERED, WS_EX_TRANSPARENT, WS_EX_NOACTIVATE = -20, 0x80000, 0x20, 0x8000000
            hwnd = int(self.winId())
            style = ctypes.windll.user32.GetWindowLongW(hwnd, GWL_EXSTYLE)
            ctypes.windll.user32.SetWindowLongW(hwnd, GWL_EXSTYLE, style | WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE)

    def show_views(self, views: list[AltarView]):
        if not views:
            self._shown = ()
            self.hide()
            return
        cx, cy = self._monitor["width"] // 2, self._monitor["height"] // 2
        # Closest to the screen center (the character) first, then left to right.
        views = sorted(views, key=lambda v: abs(v.altar.icon.x - cx) + abs(v.altar.icon.y - cy))[:MAX_ALTARS]
        views.sort(key=lambda v: v.altar.icon.x)
        self._reload_labels()
        signature = tuple(_signature(v) for v in views)
        if signature != self._shown:
            self._shown = signature
            while self._layout.count():
                old = self._layout.takeAt(0).widget()
                old.setParent(None)   # out of the layout now, so adjustSize ignores it
                old.deleteLater()
            for view in views:
                self._layout.addWidget(_altar_column(view, self._icons, self._labels), 0, Qt.AlignBottom)
            self._layout.activate()
            self.resize(self._layout.sizeHint())
        self.show()
        self._anchor()

    def _reload_labels(self):
        """Pick up edits to labels.json while running, like profile changes."""
        try:
            mtime = LABELS_PATH.stat().st_mtime
        except FileNotFoundError:
            mtime = None
        if mtime != self._labels_mtime:
            self._labels_mtime, self._labels = mtime, load_labels()
            self._shown = ()   # force a rebuild with the new texts

    def resizeEvent(self, event):
        # Qt may finish sizing the new boxes only after they are shown; re-anchor on every
        # size change, otherwise the panel grows to the right and down from its old corner.
        super().resizeEvent(event)
        self._anchor()

    def _anchor(self):
        mon = self._monitor
        self.move(mon["left"] + (mon["width"] - self.width()) // 2 + self._offset_x,
                  mon["top"] + mon["height"] - self.height() - self._bottom)
