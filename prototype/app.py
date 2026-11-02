"""Live altar scanner with HUD overlay and a tray icon (settings board, quit).

Usage:
    python -m prototype.app                                  # scan the primary monitor
    python -m prototype.app --replay tests/fixtures/screens  # cycle through screenshots instead
    python -m prototype.app --stats                          # print scan time and CPU load every 10 s

Quit via the tray icon ("Quit") or Ctrl+C in the terminal.
Passive screen reading only: the app never sends input to the game.
"""

from __future__ import annotations

import argparse
import os
import signal
import sys
import threading
import time
from pathlib import Path

from PySide6.QtCore import QTimer
from PySide6.QtGui import QAction, QIcon
from PySide6.QtWidgets import QApplication, QMenu, QSystemTrayIcon

from . import settings
from .live import ReplaySource, Scanner, ScreenSource, frames
from .match import load_cards
from .overlay import Bridge, Hud

ICON = Path(__file__).resolve().parent / "assets" / "icon_eater.png"


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--profile", type=Path, help="profile file (default: the one selected in the board)")
    parser.add_argument("--monitor", type=int, help="monitor index (default: primary)")
    parser.add_argument("--replay", type=Path, help="folder of screenshots to cycle through instead of the screen")
    parser.add_argument("--hold", type=float, default=3.0, help="seconds per screenshot in replay mode")
    parser.add_argument("--hz", type=float, default=4.0, help="scans per second")
    parser.add_argument("--board", action="store_true", help="open the settings board on start")
    parser.add_argument("--stats", action="store_true", help="print scan time and CPU load every 10 seconds")
    args = parser.parse_args(argv)

    app = QApplication(sys.argv)
    app.setQuitOnLastWindowClosed(False)   # closing the board keeps the overlay running
    # Ctrl+C in the terminal quits too. Python handles signals only when it runs on the main
    # thread, which the Qt event loop blocks, so a timer hands control back regularly.
    signal.signal(signal.SIGINT, lambda *_: app.exit(0))
    wake = QTimer(app, interval=200, timeout=lambda: None)
    wake.start()
    source = ReplaySource(sorted(args.replay.glob("*.png")), args.hold) if args.replay else ScreenSource(args.monitor)
    mon = source.monitor
    hud = Hud(mon)   # reads its position and style from settings.json, live
    bridge = Bridge()
    bridge.views.connect(hud.show_views)

    board = None

    def open_board():
        nonlocal board
        from .board import BoardWindow   # WebEngine starts only when needed
        if board is None:
            board = BoardWindow()
        board.show()
        board.raise_()
        board.activateWindow()

    tray = QSystemTrayIcon(QIcon(str(ICON)), app)
    tray.setToolTip("Altar Scanner")
    menu = QMenu()
    menu.addAction(QAction("Settings board", menu, triggered=open_board))
    menu.addAction(QAction("Quit", menu, triggered=app.quit))
    tray.setContextMenu(menu)
    tray.activated.connect(lambda reason: open_board() if reason == QSystemTrayIcon.DoubleClick else None)
    tray.show()
    if args.board:
        open_board()

    profile_path = (lambda: args.profile) if args.profile else settings.active_profile_path

    def run():
        scanner = Scanner(source, load_cards(), profile_path)   # OCR engine lives on this thread
        if args.stats:
            scanner.scan = _timed(scanner.scan)
        for views in frames(scanner, args.hz):
            bridge.views.emit(views)

    threading.Thread(target=run, daemon=True).start()
    return app.exec()


def _timed(scan, every: float = 10.0):
    """Wrap Scanner.scan to print scan rate, scan time and the CPU load of the whole process."""
    durations: list[float] = []
    window = [time.monotonic(), time.process_time()]   # process_time covers all threads (scanner and HUD)

    def wrapper():
        start = time.perf_counter()
        views = scan()
        durations.append(time.perf_counter() - start)
        now, cpu = time.monotonic(), time.process_time()
        if now - window[0] >= every:
            share = (cpu - window[1]) / (now - window[0])
            print(f"[stats] {len(durations) / (now - window[0]):.1f} scans/s, "
                  f"scan avg {1000 * sum(durations) / len(durations):.0f} ms / max {1000 * max(durations):.0f} ms, "
                  f"CPU {100 * share / os.cpu_count():.1f}% of total ({100 * share:.0f}% of one core), "
                  f"{len(views)} altar(s) on screen", flush=True)
            durations.clear()
            window[:] = [now, cpu]
        return views

    return wrapper


if __name__ == "__main__":
    sys.exit(main())
