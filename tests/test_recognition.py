"""End-to-end recognition accuracy on the labeled screenshots (not in git; skipped if absent)."""

import io
import re
import sys
from contextlib import redirect_stdout

import pytest

from prototype.evaluate import LABELS, SCREENS, evaluate

pytestmark = pytest.mark.skipif(
    sys.platform != "win32" or not any(SCREENS.glob("screen_*.png")) or not LABELS.exists(),
    reason="needs Windows OCR and the local screenshots in tests/fixtures/screens",
)


def test_recognition_accuracy():
    out = io.StringIO()
    with redirect_stdout(out):
        evaluate()
    report = out.getvalue()
    found, total = map(int, re.search(r"altars detected : (\d+)/(\d+)", report).groups())
    accuracy = float(re.search(r"options correct : \d+/\d+\s+\(([\d.]+) %\)", report).group(1))
    false_altars = int(re.search(r"\(\+(\d+) false\)", report).group(1))
    assert found == total, report
    assert false_altars == 0, report
    assert accuracy >= 95.0, report
