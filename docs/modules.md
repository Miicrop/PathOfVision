# Writing a module: resolutions and test fixtures

How a screen-reading module (like `modules/altar`) stays correct across client sizes, and what it needs to be tested.

## One scale, one reference height

Path of Exile lays its UI out relative to the client height. A module therefore writes every pixel constant (template sizes, offsets, search regions, gaps, thresholds in pixels) **as a reference value for a 1440 px tall client** and converts it where it is used with `vision::Scale`:

- `Scale::for_client(w, h)` is `h / 1440`. This was measured on real shots at 1080, 1132, 1200, 1353 and 1440 px, 16:9, 16:10 and 21:9: icons and text sizes follow the height within 3 %. At 16:10 the UI is still height-limited, not width-limited. A client wider than PoE's maximum aspect ratio is pillarboxed (black bars left and right), which does not change the scale.
- `scale.px(v)` / `scale.px_i(v)` turn a reference value into native pixels.
- Image work that compares against reference-size templates resizes the native frame by `factor / scale` (see `COARSE_SCALE` and `FINE_SCALE` in `modules/altar/src/detect.rs`), so templates are made once, at 1440 px.
- OCR upscales each native crop once by `UPSCALE / scale` (see `modules/altar/src/read.rs`), so the text reaches the OCR at the same size at every resolution.
- Frames below `vision::MIN_CLIENT_HEIGHT` (480 px) are not scanned.

Which sizes count as verified is decided in one place, `pov_core::resolution` (`VERIFIED_MIN_H`, `VERIFIED_MAX_H`, `VERIFIED_MIN_ASPECT`, `WINDOWED_SLACK`). Widen it only after a real-shot acceptance run at the new size.

## Fixtures a module needs

1. **Reference-size fixtures:** labeled screenshots at 1440 px height (the altar module uses `tests/fixtures/screens` with `tests/fixtures/labels.json`). These are what the constants are tuned on.
2. **A few real shots at the smallest verified height** (1080 px today), plus any other size you want to call verified. Capture them with [shot-list.md](shot-list.md). Full-desktop screenshots of a windowed game are fine: `tools/crop_client.py <folder> --expect 1920x1080` cuts the client area out of each `Screenshot (N).png` into `<folder>/client/screen_NN.png` and checks the size.
3. **A synthetic test:** downscale the reference fixtures to a smaller height and run the same accuracy check (the altar module's `evaluate_scaled`, or `evaluate --height <px>`; `--write <dir>` saves the resized frames for `pov-app --replay`). This catches scale bugs cheaply, but it does not replace real shots: the game renders text differently at small sizes than a downscaled 1440 px frame shows it.

Screenshots stay local and git-ignored; label files go into git. Label every box by eye, never from the module's own output alone (`evaluate <screens> --draft <file>` writes a draft to check against the image), and mark unreadable boxes with `"mods": null` and a `note`.

## Acceptance bar per real resolution

All objects found, no false detections, and at least 95 % of the readable options fully correct. Add one `#[ignore]` golden test per set (see `modules/altar/tests/golden.rs`) that skips itself when the screenshots are missing, and run them with:

```
cargo test -p <module> --release --test golden -- --ignored
```
