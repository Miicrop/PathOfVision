# Plan C – Resolutions and First Release Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The altar module reads correctly at 1080p to 4K through one shared scale factor, and friends install Path of Vision from a public GitHub repo with signed auto-updates.

**Architecture:** All module code keeps working in **native pixels**; every pixel constant is defined at the 1440 px reference height and converted with a `vision::Scale` (`s = client_height / 1440`) per frame. Detection resizes by `COARSE/FINE_SCALE / s`, OCR upscales crops by `2 / s`, so text reaches the OCR at the same size as today with a single resample. A core `resolution` check decides verified / untested / unsupported per client size. Release: Tauri NSIS per-user installer with bundled data, `tauri-plugin-updater` against GitHub Releases, GitHub Actions for CI and draft releases.

**Tech Stack:** Rust 2024, Tauri 2 (`tauri-plugin-updater`, `tauri-plugin-process`), Windows.Media.Ocr, GitHub Actions (`tauri-apps/tauri-action@v0`), plain HTML/CSS/JS (no build step).

**Spec:** `docs/superpowers/specs/2026-11-04-plan-c-resolutions-release-design.md` (parent: `docs/superpowers/specs/2026-11-03-path-of-vision-port-design.md`). Read both before starting.

## Global Constraints

- ToS: passive screen reading only; never send input to the game, never read game memory.
- Windows 10/11 only; English game client.
- The only network traffic is the update check/download from GitHub Releases.
- Tests and tools never touch the user's real `settings.json` or profiles (`%APPDATA%\miicrop-pov`): use `tempfile` dirs and copies.
- Never launch the user's apps (games, editors, installers, `pov-app.exe`) from an agent; the user runs anything interactive.
- Internal identity stays `io.github.miicrop.pov` / `%APPDATA%\miicrop-pov`; display name "Path of Vision".
- Reference height: **1440 px**. Acceptance per real resolution: all altars found, no false detections, ≥ 95 % of readable options fully correct. 3440×1440 stays at exactly today's golden result (84/85, the one known miss `screen_10.png [856, 1152] bottom`).
- **1080p is a hard gate:** `v0.1.0` is not published before the real 1080p set passes.
- Screenshots stay local and git-ignored; label files go into git.
- No `Co-Authored-By` trailer in commits (user rule). No AI tool or vendor is named as author or contributor anywhere (commits, README, LICENSE, `Cargo.toml` `authors`, installer publisher, GitHub release text); the author is Miicrop.
- Cargo lives at `%USERPROFILE%\.cargo\bin` (not on bash PATH; use PowerShell or the full path).
- English only in code, data, UI and docs.

## Review Focus

1. **Windowed clients slightly smaller than a standard height** (a friend on a 1080 monitor in windowed mode gets e.g. 1920×1017): must count as verified, not unsupported. → Task 4 test `windowed_1080_monitor_is_verified`.
2. **The PoE window is resized while scanning** (scale changes between two frames): old tracks must not be matched across scales. → Task 3 test `scale_change_clears_tracks`.
3. **HUD on a monitor with Windows display scaling** (4K at 150 %): HUD physical size must follow `s` only, not `s × devicePixelRatio`. → Task 4 Step 3 fixes the rule in `hud.js` (CSS px = `font_size × scale / devicePixelRatio`); Task 4 Step 5 has the user check it in replay.
4. **Update found while PoE is running or offline / GitHub unreachable at start**: no install mid-game, no error dialog, app runs normally. → Task 10 test `auto_install_only_when_poe_is_closed`.
5. **Settings and profiles survive an update and a reinstall** (installer must not touch `%APPDATA%\miicrop-pov`). → Task 12 Step 5 (manual check by the user, written into the release checklist).

---

### Task 1: Shot list for the user (request early)

**Files:**
- Create: `docs/shot-list.md`
- Modify: `.gitignore`

- [ ] **Step 1: Write `docs/shot-list.md`** for a non-programmer: set PoE to **Windowed** mode and pick the resolution in Options → Graphics; sets wanted: `1920x1080` (main set, ≥ 25 screenshots, ≥ 40 altars), `1920x1200` (≥ 8 screenshots), `2304x1296` (≥ 8 screenshots; if PoE does not offer it, the nearest height between 1200 and 1400 it offers). Variety per set: both altar colors, tall boxes (3 downsides + 2 upsides), boxes near the top buff bar and near screen edges, an altar next to another altar, busy fights. Capture with the same tool as before (the user's usual screenshot key, whole window); files go into `tests/fixtures/screens_<w>x<h>/` named `screen_NN.png`. Also: "if you allocate the Exarch atlas passive, a few shots with an extra upside line help".
- [ ] **Step 2: Add `tests/fixtures/screens_*/` to `.gitignore`** (next to the existing `tests/fixtures/screens/*.png` line).
- [ ] **Step 3: Commit** `git add docs/shot-list.md .gitignore && git commit -m "Docs: shot list for resolution fixtures"`.
- [ ] **Step 4: Tell the user** (in German, one short message) where the shot list is and that the 1080p set is needed before Task 7; continue with Task 2 meanwhile.

### Task 2: Shipped files are found without the build folder

Today `detect::assets_dir()` and `data::repo_data_dir()` resolve through `CARGO_MANIFEST_DIR`, so an installed app on another PC finds no altar templates.

**Files:**
- Modify: `crates/core/src/paths.rs`, `modules/altar/src/module.rs`, `modules/altar/src/detect.rs`, `app/src/main.rs` (`app_paths`), `apps/scan/src/main.rs`
- Test: `crates/core/src/paths.rs` (unit), `modules/altar/tests/module.rs`

**Interfaces:**
- Produces: `AppPaths { data, modules, user }` with `pub modules: PathBuf`; `AppPaths::module_assets(&self, module: &str) -> PathBuf` = `modules/<module>/assets`. `installed()`: `data = <exe dir>/data`, `modules = <exe dir>/modules`. `dev()`: `<repo>/data`, `<repo>/modules`. `AppPaths::with(data, user)` keeps its signature and sets `modules = <repo>/modules` (tests); add `AppPaths::with_modules(data, modules, user)`.
- `AltarModule::new` loads templates from `paths.module_assets("altar")`. `assets_dir()` stays only for tests and the `evaluate` bin.

- [ ] **Step 1: Failing tests:** `installed_layout_is_next_to_the_exe` (exe dir `X` → `X/data`, `X/modules/altar/assets`), and in `tests/module.rs` `module_loads_assets_from_app_paths` (copy `modules/altar/assets` and the data files into a tempdir, `AppPaths::with_modules(tmp/data, tmp/modules, tmp/user)`, `AltarModule::new` succeeds; with an empty modules dir it returns an error mentioning `icon_eater.png`).
- [ ] **Step 2: Run** `cargo test -p pov-core paths` and `cargo test -p altar --test module` → FAIL.
- [ ] **Step 3: Implement** the fields and constructors; `app_paths()` in `app/src/main.rs` checks both `data/mods.json` and `modules/altar/assets/icon_eater.png` before falling back to `dev()`.
- [ ] **Step 4: Run** `cargo test --workspace` → PASS.
- [ ] **Step 5: Commit** `"Core: app paths for shipped module assets"`.

### Task 3: Scale-aware altar pipeline

**Files:**
- Create: `crates/vision/src/scale.rs` (export in `lib.rs`)
- Modify: `crates/vision/src/image.rs` (RGB resize), `modules/altar/src/detect.rs`, `modules/altar/src/read.rs`, `modules/altar/src/live.rs`, `modules/altar/src/module.rs`, `modules/altar/src/evaluate.rs`
- Test: `crates/vision/src/scale.rs` (unit), `modules/altar/tests/detect.rs`, `modules/altar/tests/read.rs`, `modules/altar/tests/live.rs`

**Interfaces:**
- Produces:
  - `pub const REFERENCE_HEIGHT: f64 = 1440.0;`
  - `#[derive(Clone, Copy, Debug, PartialEq)] pub struct Scale(pub f64);` with `Scale::for_client(w: usize, h: usize) -> Scale` (`h / 1440`; Task 7 may change the narrow-aspect case), `Scale::REFERENCE` (= 1.0), `fn px(self, reference_px: f64) -> f64` (reference → native), `fn px_i(self, reference_px: i32) -> i32` (rounded), `fn is_narrow(w: usize, h: usize) -> bool` (`w * 9 < h * 16`).
  - `Rgb::resize_area(&self, scale: f64) -> Rgb` (same area filter as `Gray::resize_area`, per channel).
  - `Detector::find_icons(&self, gray: &Gray, scale: Scale) -> Vec<Icon>` (icons in native px).
  - `altar_type(img: &Rgb, icon: &Icon, scale: Scale) -> AltarKind`.
  - `Reader::read_altar(&self, img: &Rgb, icon: &Icon, scale: Scale)`; `Reader::read_altars(&self, img: &Rgb)` computes `Scale::for_client(img.w, img.h)` itself.
  - `merge_rows(lines, scale)`, `box_lines(lines, cx, near_y, upward, scale)`.
  - `Tracker::scan(&mut self, now, icons, cards, scale, read)`: when `scale` differs from the previous scan's by more than 1 %, the tracker clears first.
  - `evaluate::evaluate_scaled(screens: &Path, labels: &Path, height: Option<u32>) -> Result<Report>`: with `Some(h)` each screen is resized to height `h` (`Rgb::resize_area(h / img.h)`) and label icon coords and `ICON_TOLERANCE` are multiplied by the same factor; `evaluate(screens, labels)` = `evaluate_scaled(.., None)`.

All existing constants (`RING`, `FINE_WINDOW`, `SEARCH_HALF_WIDTH`, `SEARCH_HEIGHT`, `CENTER_TOLERANCE`, `MAX_FRAGMENT_GAP`, `LINE_GAP`, `TRACK_DISTANCE`, `ICON_TOLERANCE`) stay as reference values and are converted with `scale.px*` where used. Detection resizes the native gray by `COARSE_SCALE / s` and `FINE_SCALE / s` (templates unchanged). OCR crops come from the native frame and are upscaled by `UPSCALE / s` in `preprocess`; line coords map back by dividing by that factor.

- [ ] **Step 1: Failing unit tests** in `scale.rs`: `for_client(3440, 1440) == Scale(1.0)`, `for_client(1920, 1080) == Scale(0.75)`, `for_client(3840, 2160) == Scale(1.5)`, `Scale(0.75).px_i(63) == 47`, `is_narrow(1920, 1200)`, `!is_narrow(1920, 1080)`, `!is_narrow(3440, 1440)`. In `image.rs`: `rgb_resize_matches_gray_resize` (resize of an RGB image with equal channels equals the gray resize).
- [ ] **Step 2: Failing pipeline tests** (skip when fixture screens are missing, like the existing tests): `detect.rs::icons_found_on_downscaled_screen` (screen_02 resized to height 1080, `find_icons(.., Scale(0.75))` finds icons within `40 × 0.75` px of the labels × 0.75); `read.rs::reads_downscaled_screen_02` (same image, `read_altars` returns the labeled first altar's bottom mods); `live.rs::scale_change_clears_tracks` (scan with `Scale(1.0)` then `Scale(0.75)` with the same icon list → second scan starts without old tracks).
- [ ] **Step 3: Run** `cargo test -p vision -p altar` → new tests FAIL.
- [ ] **Step 4: Implement** the interfaces above; `AltarModule::on_frame` computes `let scale = Scale::for_client(frame.w, frame.h)` and passes it to `find_icons`, `tracker.scan` and `read_altar`.
- [ ] **Step 5: Run** `cargo test --workspace` and `cargo test -p altar --release --test golden -- --ignored` → PASS, golden unchanged (84/85, same miss).
- [ ] **Step 6: Add the synthetic golden test** `golden.rs::synthetic_1080_reads_well` (`#[ignore]`, skips without screens): `evaluate_scaled(screens, labels, Some(1080))` → `altars_found == altars_total`, `false_altars == 0`, `options_ok / options_total ≥ 0.95`. Run it with `--ignored --release`. If it fails, iterate on the pipeline (filter choice, thresholds as reference values) until it passes without changing the 3440×1440 result. Record the percentage in the commit message.
- [ ] **Step 7: Extend the `evaluate` bin** to `evaluate [<screens> <labels>] [--height <px>]` (defaults: today's fixtures) and print the scale used.
- [ ] **Step 8: Commit** `"Altar: run the pipeline at any client height via one scale factor"`.

### Task 4: Resolution support check, HUD scaling, untested note

**Files:**
- Create: `crates/core/src/resolution.rs` (export in `lib.rs`)
- Modify: `crates/core/src/status.rs`, `crates/core/src/runner.rs`, `modules/altar/src/module.rs`, `modules/altar/src/hud.rs`, `app/src/commands.rs` (`hud_fit`), `ui/modules/altar/hud/hud.js`, `ui/shell/home.js`
- Test: `crates/core/src/resolution.rs`, `modules/altar/tests/hud.rs`, `crates/overlay/src/lib.rs`

**Interfaces:**
- Produces: `#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)] #[serde(rename_all = "lowercase")] pub enum Support { Verified, Untested, Unsupported }`; `pub fn support(w: u32, h: u32, try_anyway: bool) -> Support`; constants `VERIFIED_MIN_H: u32 = 1440`, `VERIFIED_MAX_H: u32 = 1440` (Task 7 widens them), `UNTESTED_MAX_H: u32 = 2160`, `WINDOWED_SLACK: f64 = 0.9`.
- Rules *(builder's call, spec 4.4)*: `Verified` if `!is_narrow` and `VERIFIED_MIN_H × 0.9 ≤ h ≤ VERIFIED_MAX_H`; `Untested` if `VERIFIED_MAX_H < h ≤ UNTESTED_MAX_H`, or narrow aspect inside `[min × 0.9, UNTESTED_MAX_H]`; otherwise `Unsupported`, which `try_anyway` turns into `Untested`. The 0.9 slack lets windowed clients on a monitor of a verified height count as verified.
- `RunnerStatus` gets `pub resolution: Option<Support>` (None without a PoE window), filled by the runner from `ctx.poe.client` and `general.try_unsupported_size`.
- `AltarModule::wants_frames` returns false when the support is `Unsupported`.
- `HudStyle` gets `scale: f64` and `untested: bool`; `hud_rect` callers pass `offset_x × s` and `bottom × s` (`s = client.h / 1440`).

- [ ] **Step 1: Failing tests** in `resolution.rs` (with Task 7's final constants in mind, written against the constants, not literals): `standard_heights` (`(3440,1440)` Verified, `(3840,2160)` Untested, `(1280,720)` Unsupported, same with `try_anyway` → Untested); `windowed_1080_monitor_is_verified` (after Task 7: `(1920,1017)` Verified — write it now against `VERIFIED_MIN_H` as `(w, (VERIFIED_MIN_H as f64 * 0.94) as u32)`); `narrow_is_untested` (`(2560, 1600)` Untested). In `hud.rs`: `hud_style_carries_scale_and_untested`. In overlay: `hud_rect_offsets_follow_scale`.
- [ ] **Step 2: Run** → FAIL.
- [ ] **Step 3: Implement** the interfaces; `hud.js` sizes everything from `style.font_size * style.scale` in **physical** px, i.e. CSS px = `font_size * scale / devicePixelRatio`, and shows a small "untested resolution" line when `style.untested`; Home shows "Unsupported game size W×H – enable 'Try unsupported resolutions' in Settings → General to try anyway" for `Unsupported` and "Untested resolution" for `Untested`. The settings schema label for `try_unsupported_size` reads "Try unsupported resolutions".
- [ ] **Step 4: Run** `cargo test --workspace` → PASS.
- [ ] **Step 5: Replay check** `cargo run -p pov-app --release -- --replay <tmp dir with screen_02 resized to 1080 height> --hold 2` is run **by the user** (agents never launch the app): ask the user to confirm the HUD is 75 % of its usual size and placed bottom-center. Give the exact command and prepare the folder with a small test helper (`evaluate --height 1080 --write <dir>` writes the resized screens).
- [ ] **Step 6: Commit** `"Core: resolution support check; HUD follows the client scale"`.

### Task 5: Carry-over backend fixes from Plan B2

**Files:**
- Modify: `crates/capture/src/lib.rs`, `crates/core/src/settings.rs`, `crates/core/Cargo.toml`, `modules/altar/src/board.rs`, `modules/altar/src/labels.rs`, `modules/altar/src/hud.rs`, `app/src/commands.rs`
- Test: the same files' test modules, `modules/altar/tests/board.rs`, `modules/altar/tests/hud.rs`

**Interfaces:**
- `needs_device_reset` gets a back-off: `pub struct ResetBackoff` with `fn allow(&mut self, now: Instant) -> bool` and `fn failed(&mut self, now: Instant)`; delays 1 s, 2 s, 4 s … capped at 60 s; a successful grab resets it.
- Module registry in `settings.rs`: `pub struct ModuleSettingsDef { id: &'static str, defaults: fn() -> Value, validate: fn(&Map<String, Value>) -> Value }` and `const MODULES: &[ModuleSettingsDef]`; the `match section { "altar" => … }` arms become lookups.
- `profile_file_name` appends `-profile` to Windows reserved stems (`con prn aux nul com1–com9 lpt1–lpt9`, any case).

- [ ] **Step 1: Failing tests:** `backoff_doubles_and_caps` (allow at 0 s, then not before 1 s, then 2 s, 4 s … ≤ 60 s; reset after success); `unknown_module_section_is_an_error` (existing behavior preserved through the registry); `reserved_names_get_a_suffix` (`"CON"` → `"con-profile.json"`, `"lpt1"` → `"lpt1-profile.json"`, `"console"` unchanged); `color_keywords_regex_is_cached` (two calls with the same keyword slice build one regex — expose a `KeywordMatcher` built once per keywords reload in `Watched` and used by `hud_view`); `hud_altar_recommendation_and_dimming` (pick gets glow, other option dimmed at `hud_other_opacity`, ADAPT - BRICK never dimmed); `open_settings_url_encodes_section` (`"a b&c"` → `settings.html?section=a%20b%26c`, extract the URL builder into a pure fn).
- [ ] **Step 2: Run** → FAIL.
- [ ] **Step 3: Implement**; add `serde_json = { version = "1", features = ["preserve_order"] }` to `crates/core/Cargo.toml`.
- [ ] **Step 4: Run** `cargo test --workspace` → PASS.
- [ ] **Step 5: Commit** `"Fix B2 review carry-over: capture back-off, module registry, reserved names, cached keywords"`.

### Task 6: Settings window fixes

**Files:**
- Modify: `ui/shell/settings.js`

- [ ] **Step 1: Implement:** Number and Path fields save on `change` and on Enter (`keydown` Enter → flush), not only via the debounce; `pagehide` calls the pending flush synchronously; every value read from settings is guarded (`value ?? field.default`), no `undefined` written back.
- [ ] **Step 2: Verify** with `node --check ui/shell/settings.js`; then ask the user to open Settings, type a HUD offset, press Enter, close the window immediately, reopen → value kept.
- [ ] **Step 3: Commit** `"Settings window: save on Enter and on close, guard undefined values"`.

### Task 7: Real fixtures, scale check, 1080p gate (needs the user's shots)

**Files:**
- Create: `tests/fixtures/labels_1920x1080.json`, `tests/fixtures/labels_1920x1200.json`, `tests/fixtures/labels_<w>x<h>.json` (mid height)
- Modify: `modules/altar/tests/golden.rs`, `crates/core/src/resolution.rs` (constants), `crates/vision/src/scale.rs` (narrow case only if needed), `modules/altar/src/*` (only if reads need work)

- [ ] **Step 1: Scale hypothesis check.** For one shot per set, measure the altar ring radius and a box's line height in native px (crop and compare against 3440×1440, e.g. by matching the icon template at scales 0.5–1.6 and taking the best). Expected: ratio = `h / 1440` within 3 % for 1080 and the mid height. For 1920×1200: ratio = `1200/1440` (height-limited) or `1920/2560` (width-limited). Write the numbers into the commit message. **If 16:9 does not scale with height, stop and report to the user** (spec fallback: small multi-scale search, needs a new decision).
- [ ] **Step 2: If 16:10 is width-limited,** change `Scale::for_client` to `min(h / 1440, w / 2560)` with test `narrow_is_width_limited` (`for_client(1920, 1200) == Scale(0.75)`); if height-limited, keep it and add the test `narrow_is_height_limited`.
- [ ] **Step 3: Label the sets.** Run `evaluate` on each set to get a draft, check every box against the image by eye, and write the label files in the existing `labels.json` format (`"mods": null` with a `note` for unreadable boxes).
- [ ] **Step 4: Golden tests** (all `#[ignore]`, skip without screens): `real_1920x1080`, `real_1920x1200`, `real_mid` with the acceptance bar from Global Constraints.
- [ ] **Step 5: Run** `cargo test -p altar --release --test golden -- --ignored`. **1080p gate:** while `real_1920x1080` fails, improve the reads (thresholds as reference values, OCR upscale factor, text brightness, matching tolerance) and rerun all golden tests including 3440×1440 and synthetic. This step is open-ended; report progress to the user after each attempt that does not pass.
- [ ] **Step 6: Widen the verified range** to the passing heights (`VERIFIED_MIN_H = 1080` once 1080 passes; `VERIFIED_MAX_H = 1440`) and run `cargo test --workspace`.
- [ ] **Step 7: Write `docs/modules.md`** (spec 4.5): a module defines pixel constants at 1440 px height and converts them with `vision::Scale`; it needs reference-size fixtures plus a few real shots at the smallest verified height (reuse `docs/shot-list.md`) and gets the synthetic test via `evaluate_scaled`-style downscaling.
- [ ] **Step 8: Commit** `"Altar: verified at 1080p, 1296p, 1440p and 16:10"` (with the percentages per set).

### Task 8: Remove the Python prototype and clean up locally

**Files:**
- Delete: `prototype/`, prototype-only tests in `tests/` (`test_live.py`, `test_match.py`, `test_recognition.py`, `test_settings.py`, `test_verdict.py`), `prototype` mentions in `.gitignore` if any
- Keep: `tools/`, `tests/test_gen_mods.py`, `tests/test_labels.py`, `tests/test_data_files.py`, `tests/fixtures/`
- Modify: `requirements.txt`, `plans/Plan.md`, `docs/` mentions of running the prototype

- [ ] **Step 1: Check what the kept Python files import** (`grep` over `tools/` and the kept tests); `requirements.txt` keeps only those packages plus `pytest` (expected: `numpy`, `opencv-python` for `gen_board_art`, `pytest`). Make sure the kept tests do not import from `prototype/` (move any needed helper into `tools/`).
- [ ] **Step 2: Delete** the files with `git rm -r`, run `.venv\Scripts\python -m pytest tests` → PASS.
- [ ] **Step 3: List untracked leftovers** (`git status --ignored --short`, excluding `target/`, `settings.json`, `data/profiles/*`, `tests/fixtures/screens*`, `.remember/`) and **show the list to the user before deleting anything**. Delete what the user confirms (expected: `.pytest_cache`, `__pycache__` folders, the old `.venv`).
- [ ] **Step 4: Rebuild the venv** `python -m venv .venv` and `.venv\Scripts\pip install -r requirements.txt`; rerun pytest → PASS.
- [ ] **Step 5: Update docs:** `plans/Plan.md` says the prototype was removed (available in git history before this commit), and adds the Direct2D HUD as a Phase 5 backlog item: "long-term optimization, only when RAM becomes a problem (WebView HUD ~380 MB working set with the main window closed, 2026-11-04)".
- [ ] **Step 6: Commit** `"Remove the Python prototype; slim Python requirements to the dev tools"`.

### Task 9: Installer with bundled data and clean uninstall

**Files:**
- Modify: `app/tauri.conf.json`, `.gitignore` (if needed)
- Create: `app/installer/hooks.nsh`
- Test: build output check

**Interfaces:**
- `bundle.active: true`, `bundle.targets: ["nsis"]`, `bundle.windows.nsis.installMode: "currentUser"`, `bundle.publisher: "miicrop"`, `bundle.licenseFile: "../LICENSE"`; this task creates `LICENSE` (MIT, copyright "2026 Miicrop").
- `bundle.resources` (map form) ship exactly: `../data/mods.json → data/mods.json`, `../data/labels.json`, `../data/keywords.json`, `../data/profiles/default.json`, `../data/profiles/empty.json`, `../modules/altar/assets/ → modules/altar/assets/`. Nothing from the user's ignored profiles.
- `hooks.nsh` defines `NSIS_HOOK_POSTUNINSTALL`: delete `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` and `…\Explorer\StartupApproved\Run` values named like the autostart entry (check the name `tauri-plugin-autostart` writes: the product name). User data in `%APPDATA%\miicrop-pov` is **not** deleted.

- [ ] **Step 1: Install tauri-cli if missing** (`cargo tauri --version`, else `cargo install tauri-cli --version "^2" --locked`).
- [ ] **Step 2: Configure** as above; build `cargo tauri build` from `app/`.
- [ ] **Step 3: Verify** the installer exists under `target/release/bundle/nsis/` and list its payload (`7z l` if available, or run the installer with `/S /D=<scratch dir>` — a silent install into the scratchpad is allowed, it is not the user's app session; do not start the installed exe). Check: `data/` holds exactly the five files, `modules/altar/assets/` exists, size reported.
- [ ] **Step 4: Ask the user** to install it normally, start it, confirm the HUD works in-game, then uninstall and confirm the autostart entry is gone (if autostart was on) and `%APPDATA%\miicrop-pov` is still there.
- [ ] **Step 5: Commit** `"App: per-user NSIS installer with bundled data, uninstall cleans autostart"`.

### Task 10: Updates

**Files:**
- Modify: `app/Cargo.toml`, `app/src/main.rs`, `app/tauri.conf.json`, `app/capabilities/*.json`, `ui/shell/home.html`, `ui/shell/home.js`
- Create: `app/src/update.rs`
- Test: `app/src/update.rs` unit tests

**Interfaces:**
- Deps: `tauri-plugin-updater = "2"`, `tauri-plugin-process = "2"`.
- Config: `bundle.createUpdaterArtifacts: true`; `plugins.updater.pubkey: <public key>`; `plugins.updater.endpoints: ["https://github.com/Miicrop/PathOfVision/releases/latest/download/latest.json"]` (confirm the GitHub user name with the user in Task 11 before the first release); `plugins.updater.windows.installMode: "passive"`.
- `update.rs`: `pub enum StartAction { Install, Offer, Nothing }`; `pub fn start_action(update_available: bool, poe_running: bool) -> StartAction` (update + PoE closed → Install; update + PoE running → Offer; else Nothing). At start, a background task checks once; on error it logs to stderr and does nothing else. `Install` downloads, installs and restarts before the scanner starts; `Offer` emits `app:update` `{version, notes}` to Home. Command `update_now` (download, install, `app.restart()`).
- Home shows "Update available: vX.Y.Z" with an "Update now" button while an offer is pending.
- Signing key: generated with `cargo tauri signer generate -w <path outside the repo>`; the private key and its password never enter the repo.

- [ ] **Step 1: Failing test** `auto_install_only_when_poe_is_closed` covering all four combinations of `start_action`.
- [ ] **Step 2: Run** `cargo test -p pov-app update` → FAIL.
- [ ] **Step 3: Implement** the interfaces; PoE detection at start uses `pov_game::find_poe_window().is_some()`.
- [ ] **Step 4: Generate the key with the user:** explain in German what the private key is for, store it at `%USERPROFILE%\.tauri\pov-updater.key` (outside the repo), ask the user to keep a backup copy (password manager or USB stick) and to add `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` as GitHub secrets in Task 11. Put the public key into `tauri.conf.json`.
- [ ] **Step 5: Run** `cargo test --workspace` and `cargo tauri build` → PASS, `latest.json`-compatible `.sig` file created next to the installer.
- [ ] **Step 6: Commit** `"App: update check at start, install only while PoE is closed or on click"`.

### Task 11: README, CI and the public repository

**Files:**
- Create: `README.md`, `.github/workflows/ci.yml`, `.github/workflows/release.yml`, `CHANGELOG.md`
- Modify: `.gitignore` (audit)

- [ ] **Step 1: README** (English, for players): what it does (reads Eldritch Altar options, recommends one, HUD), what it never does (no memory reading, no input to the game, no data leaves the PC; the only network traffic is the update check), supported setups (Windows 10/11, English client, windowed or borderless; verified heights from Task 7; 4K untested; narrower than 16:9 untested), install from Releases, the SmartScreen warning ("More info → Run anyway": unsigned because a certificate costs money), privacy, MIT license, how to report a bad read (screenshot + what the HUD showed, as a GitHub issue). Short development section: build, test, fixture tests run locally only.
- [ ] **Step 2: `CHANGELOG.md`** with a `0.1.0` entry.
- [ ] **Step 3: `ci.yml`** on push/PR, `windows-latest`: checkout, Rust stable with cache (`Swatinem/rust-cache@v2`), `cargo test --workspace` (fixture tests skip themselves; golden tests are `#[ignore]`).
- [ ] **Step 4: `release.yml`** on tags `v*`: `tauri-apps/tauri-action@v0` with `projectPath: app`, `tagName: ${{ github.ref_name }}`, `releaseDraft: true`, `includeUpdaterJson: true`, env `TAURI_SIGNING_PRIVATE_KEY(_PASSWORD)` from secrets, release body from the matching `CHANGELOG.md` section.
- [ ] **Step 5: Public-push audit:** `git ls-files` contains no `settings.json`, no `data/profiles/` except `default.json`/`empty.json`, no screenshots (`tests/fixtures/screens*`), no key files; history check `git log --all --name-only --format="" | sort -u` for the same patterns (only `Altar.png` expected, already cleared by the user).
- [ ] **Step 6: Commit** `"Release: README, changelog, CI and release workflows"`.
- [ ] **Step 7: User steps** (in German, numbered): create the public repo `PathOfVision` on GitHub (no README/license from GitHub), confirm the user name for the updater URL, add the two signing secrets, then the builder runs `git remote add origin …` and `git push -u origin main` after the user says so. Check that the CI run on GitHub passes (`gh run list`).

### Task 12: First release and update test

- [ ] **Step 1: Gate check:** Task 7's `real_1920x1080` passes; `cargo test --workspace` and all golden tests pass locally.
- [ ] **Step 2: When the user says "release":** check that `app/tauri.conf.json` and `app/Cargo.toml` say `0.1.0` (they do today), push tag `v0.1.0`; wait for `release.yml`; report the draft link. The user reviews and clicks **Publish**.
- [ ] **Step 3: The user installs `v0.1.0` from the Releases page** like a friend would (SmartScreen, install, start, HUD in a map).
- [ ] **Step 4: Update test:** bump to `0.1.1` (changelog entry "Update test"), tag, the user publishes; the user starts `v0.1.0` with PoE closed → it updates itself and restarts as `0.1.1`; then once more with PoE running → Home shows "Update available" and nothing installs until "Update now".
- [ ] **Step 5: The user checks** settings and profiles are unchanged after the update and after an uninstall + reinstall.
- [ ] **Step 6: Update `plans/Plan.md`** (Plan C done, measured results, verified resolutions) and commit `"Plans: Plan C done"`.
