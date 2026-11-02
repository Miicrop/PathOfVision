from types import SimpleNamespace

from prototype.detect import Icon
from prototype.live import _same_altar
from prototype.read import Altar, Option


def altar(top_ids, bottom_ids, x=0):
    def option(pos, ids):
        return Option(pos, "player", [], [SimpleNamespace(card=SimpleNamespace(id=i), partial=False, score=100) for i in ids], [])
    return Altar(Icon(x, 0, 1.0), "eater", [option("top", top_ids), option("bottom", bottom_ids)])


def test_same_altar_ignores_position_and_missing_lines():
    full = altar(["a", "b", "c"], ["d", "e"], x=100)
    partial = altar(["a", "b"], ["d"], x=900)   # read while running, one box partly hidden
    assert _same_altar(full, partial)


def test_different_altars_are_kept_apart():
    assert not _same_altar(altar(["a", "b"], ["c"]), altar(["x", "b"], ["y"]))
    assert not _same_altar(altar(["a"], ["b"]), altar(["b"], ["a"]))   # positions matter
