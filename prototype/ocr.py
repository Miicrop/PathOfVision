"""OCR backend. Returns a list of lines: (text, (x, y, w, h)) in crop pixel coordinates.

Windows.Media.Ocr won the comparison against RapidOCR: ~15 ms vs ~1.4 s per box,
and clean output where RapidOCR split lines and glued words together.
"""

from __future__ import annotations

import asyncio

import cv2
import numpy as np

Line = tuple[str, tuple[int, int, int, int]]


class WindowsOcr:
    """Windows.Media.Ocr (built into Windows 10/11, en-US language pack required)."""

    def __init__(self, language: str = "en-US"):
        from winrt.windows.globalization import Language
        from winrt.windows.media.ocr import OcrEngine

        self._engine = OcrEngine.try_create_from_language(Language(language))
        if self._engine is None:
            raise RuntimeError(f"Windows OCR language {language} is not installed")

    def read(self, bgr: np.ndarray) -> list[Line]:
        return asyncio.run(self._read(bgr))

    async def _read(self, bgr: np.ndarray) -> list[Line]:
        from winrt.windows.graphics.imaging import BitmapPixelFormat, SoftwareBitmap
        from winrt.windows.storage.streams import DataWriter

        bgra = cv2.cvtColor(bgr, cv2.COLOR_BGR2BGRA)
        h, w = bgra.shape[:2]
        writer = DataWriter()
        writer.write_bytes(bgra.tobytes())
        bitmap = SoftwareBitmap.create_copy_from_buffer(writer.detach_buffer(), BitmapPixelFormat.BGRA8, w, h)
        result = await self._engine.recognize_async(bitmap)

        lines = []
        for line in result.lines:
            rects = [word.bounding_rect for word in line.words]
            x0 = min(r.x for r in rects)
            y0 = min(r.y for r in rects)
            x1 = max(r.x + r.width for r in rects)
            y1 = max(r.y + r.height for r in rects)
            lines.append((line.text, (int(x0), int(y0), int(x1 - x0), int(y1 - y0))))
        return lines

