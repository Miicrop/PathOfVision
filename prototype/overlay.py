"""HUD overlay: a fixed, click-through, always-on-top panel that never takes focus."""

from __future__ import annotations

import ctypes
import html
import sys

from PySide6.QtCore import QObject, Qt, Signal
from PySide6.QtWidgets import QLabel, QVBoxLayout, QWidget

from .live import AltarView
from .verdict import ADAPT, AVOID, CONSIDER, NEUTRAL, TAKE, UNREADABLE

VERDICT_COLORS = {TAKE: "#4fbf6a", CONSIDER: "#e0a33a", ADAPT: "#d65db1", AVOID: "#e05a4f", NEUTRAL: "#9aa0a6", UNREADABLE: "#6f757d"}
TIER_LABELS = {"s": ("S", "#f2c84b"), "a": ("A", "#7fb2ff"), "b": ("B", "#8fd19e"),
               "dangerous": ("Dangerous", "#e0a33a"), "brick": ("Brick", "#e05a4f")}
TARGETS = {"player": "Player", "minions": "Eldritch Minions", "boss": "Map Boss", None: ""}
ALTARS = {"eater": ("Eater of Worlds", "#4fb3d9"), "exarch": ("Searing Exarch", "#e0643c")}
MAX_ALTARS = 2
MAX_LINE = 58


class Bridge(QObject):
    """Carries scan results from the scanner thread to the UI thread."""
    views = Signal(list)


class Hud(QWidget):
    def __init__(self, left: int, top: int):
        super().__init__(None, Qt.FramelessWindowHint | Qt.WindowStaysOnTopHint | Qt.Tool
                         | Qt.WindowTransparentForInput | Qt.WindowDoesNotAcceptFocus)
        self.setAttribute(Qt.WA_TranslucentBackground)
        self.setAttribute(Qt.WA_ShowWithoutActivating)
        self.label = QLabel(self)
        self.label.setTextFormat(Qt.RichText)
        self.label.setStyleSheet(
            "QLabel { background: rgba(12, 14, 20, 215); color: #d8d8d8; border: 1px solid #3a3f4b;"
            " border-radius: 6px; padding: 10px 14px; font-family: 'Segoe UI'; font-size: 13px; }")
        layout = QVBoxLayout(self)
        layout.setContentsMargins(0, 0, 0, 0)
        layout.addWidget(self.label)
        self.move(left, top)

    def showEvent(self, event):
        super().showEvent(event)
        if sys.platform == "win32":
            # Belt and braces: make the window ignore mouse input and never activate.
            GWL_EXSTYLE, WS_EX_LAYERED, WS_EX_TRANSPARENT, WS_EX_NOACTIVATE = -20, 0x80000, 0x20, 0x8000000
            hwnd = int(self.winId())
            style = ctypes.windll.user32.GetWindowLongW(hwnd, GWL_EXSTYLE)
            ctypes.windll.user32.SetWindowLongW(hwnd, GWL_EXSTYLE, style | WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE)

    def show_views(self, views: list[AltarView], screen_center: tuple[int, int]):
        if not views:
            self.hide()
            return
        cx, cy = screen_center
        views = sorted(views, key=lambda v: abs(v.altar.icon.x - cx) + abs(v.altar.icon.y - cy))[:MAX_ALTARS]
        self.label.setText("<br>".join(_render(v) for v in views))
        self.adjustSize()
        self.show()


def _span(text: str, color: str, bold: bool = False) -> str:
    weight = "font-weight:600;" if bold else ""
    return f'<span style="color:{color};{weight}">{html.escape(text)}</span>'


def _render(view: AltarView) -> str:
    name, color = ALTARS[view.altar.altar]
    parts = [_span(f"ALTAR · {name}", color, bold=True)]
    if not view.complete:
        parts[0] += " " + _span("(partly hidden, move closer)", "#9aa0a6")
    for option in view.altar.options:
        verdict = view.verdicts[option.position]
        arrow = "▲ TOP" if option.position == "top" else "▼ BOTTOM"
        parts.append(f"{_span(arrow, '#c8c8c8', True)}&nbsp;&nbsp;{_span(verdict.verdict, VERDICT_COLORS[verdict.verdict], True)}"
                     f"&nbsp;&nbsp;{_span(TARGETS[option.target], '#9aa0a6')}")
        for m in sorted(option.matches, key=lambda m: m.card.side != "upside"):
            text = " / ".join(m.display(option.lines))
            text = text if len(text) <= MAX_LINE else text[:MAX_LINE - 1] + "…"
            sign = _span("+", "#4fbf6a", True) if m.card.side == "upside" else _span("−", "#e05a4f", True)
            tier = verdict.tiers.get(m.card.id)
            label, tier_color = TIER_LABELS.get(tier, ("Unrated" if m.card.side == "upside" else "Fine", "#6f757d"))
            parts.append(f"&nbsp;&nbsp;{sign} {_span(text, '#d8d8d8')}&nbsp;&nbsp;{_span(label, tier_color, True)}")
    rec = view.recommendation
    rec_color = VERDICT_COLORS[view.verdicts[rec.choice].verdict] if rec.choice else "#e0a33a"
    parts.append(_span(rec.text, rec_color, bold=True))
    return "<br>".join(parts)
