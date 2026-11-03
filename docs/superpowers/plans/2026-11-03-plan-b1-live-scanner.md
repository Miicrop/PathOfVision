# Plan B1 – Live scanner (headless) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [x]`) syntax for tracking.

**Status:** Tasks 1–8 done and reviewed on branch `port/plan-b1`; final branch review done. Awaiting merge and the user's in-game test (Task 8 Step 6).

**Goal:** A console program `pov-scan` that finds the running PoE window, knows from the game log whether the player is in a map, captures the window with Windows.Graphics.Capture only then, and prints the altar recommendations live, within the CPU budget.

**Architecture:** New crates `crates/core` (app paths, settings and profile store, module contract, runner), `crates/game` (game log watcher, PoE window tracker), `crates/capture` (Windows.Graphics.Capture of one window). The altar module gets a live tracker (port of `prototype/live.py`) and implements the module contract. `apps/scan` is a thin console binary on top. Plan B2 later puts Tauri (windows, tray, HUD, board, settings window) on the same runner.

**Tech Stack:** Rust stable (edition 2024), crates from Plan A, `windows` 0.61 (Win32 + WinRT capture), `serde`/`serde_json`, `anyhow`, `tempfile` (tests only).

**Spec:** `docs/superpowers/specs/2026-11-03-path-of-vision-port-design.md` (sections 3, 5, 6, 7, 9). Carry-over from Plan A: `plans/Plan.md`, Phase 4, "Carry-over into Plan B".

## Global Constraints

- ToS: passive screen reading only; never send input to the game, no memory reading, no injection.
- Privacy: no network access in this plan at all.
- No capture while no module is active (spec 6.3). PoE not running or player not in a map: ~0 % CPU (spec 9).
- Mapping with the altar module active: ≤ 3 % total CPU, no measurable FPS loss (spec 9).
- Permanent internal ID: `io.github.miicrop.pov`; user data folder `%APPDATA%\miicrop-pov\` (spec 10, name-independent identity). Display name "Path of Vision".
- User data layout: `settings.json` with sections `general` and `modules.<id>`; profiles in `profiles/altar/*.json`. Shipped data (`data/`) is read-only. Tests use temp dirs and never touch the repo's `settings.json`, `data/profiles/*` (other than reading `default.json`) or `%APPDATA%`.
- Plan A results must not regress: `cargo test -p altar --release --test golden -- --ignored` stays at 44/44, +0 false, exactly the one known miss.
- cargo is at `%USERPROFILE%\.cargo\bin` (not on PATH in bash). Commit messages plain, no `Co-Authored-By` trailer.
- Game log format (verified on this PC): `2026/11/03 17:12:44 1678793843 1186a8a3 [DEBUG Client 47376] Generating level 83 area "MapWorldsDunes" with seed 3772491032`. Files: `<PoE folder>\logs\LatestClient.txt` (current session, small) and `Client.txt` (can be > 500 MB: never read it whole).

## Review Focus

1. **PoE closed, minimized or alt-tabbed:** no capture, no CPU load, no errors spammed; scanning resumes by itself when the player returns → Task 7 test `runner_pauses_capture_when_no_module_wants_frames`.
2. **Huge or missing game log:** a 500 MB `Client.txt` is never read whole; a missing log means "area unknown" (altar module still runs while PoE is focused) → Task 3 tests `reads_only_the_tail_of_a_large_log`, `missing_log_gives_unknown_area`.
3. **Broken or hand-edited user files:** invalid `settings.json` or a profile with bad JSON falls back to defaults / the last good or shipped default profile instead of crashing → Task 2 test `invalid_settings_file_gives_defaults`, Task 6 test `profile_cache_reloads_on_change_and_survives_broken_files`.
4. **Capture failure mid-session** (window closed, resized, display mode change): the frame source returns an error or `None`, the runner logs once and retries next tick, never panics → Task 7 test `runner_survives_frame_errors`.
5. **OCR error on one altar:** the altar is skipped for this frame, other altars and later frames still work → Task 6 test `read_error_skips_only_that_altar`.

---

### Task 1: Detection fast enough for the live loop

**Files:**
- Modify: `crates/vision/src/image.rs` (`resize_area` fast path), `crates/vision/src/template.rs` (`match_masked` loop order, no rayon)
- Modify: `crates/vision/Cargo.toml` (drop `rayon` if unused)
- Test: existing tests in both files, plus new ones below

**Interfaces:**
- Consumes/Produces: unchanged public API (`Gray::resize_area`, `match_masked`, `ScoreMap`).

Measured in Plan A: `to_gray` + `find_icons` on a 3440×1440 frame = 141 ms CPU single-threaded, `resize_area(0.25)` alone 12.5 ms. Target after this task: **≤ 40 ms CPU per `to_gray` + `find_icons` on screen_02, single-threaded**, identical detection results.

- `resize_area`: when `1/scale` is an integer `k` and both dimensions are divisible by `k`, average exact `k×k` blocks with integer sums (result identical to the general path, which stays for other cases; rounding as today).
- `match_masked`: restructure so the inner loop runs along x and auto-vectorizes: for each mask offset `(dx, dy, t'_k)`, for each output row `y`, `acc[x] += t'_k * I[y+dy][x+dx]` over a row slice (same for `Σ I` and `Σ I²` with the unweighted mask). Run single-threaded; remove the rayon parallelism from this function so a scan never fans out over all cores while the game runs. Scores must stay within 1e-4 of the current implementation.

- [x] **Step 1: Write the failing tests**
  - `image.rs`: `resize_area_fast_path_matches_general_path` — a seeded random 64×48 image at scale 0.25 and 0.5 gives exactly the same pixels as a private `resize_area_general` (keep the old code under that name, `#[cfg(test)]`-visible).
  - `template.rs`: `vectorized_matches_reference` — a seeded random 120×80 image, two 11×11 templates sharing a sparse ring mask: every score within 1e-4 of a brute-force f64 Pearson reference written in the test.
  - `template.rs`: `#[ignore]` bench `bench_detection_cpu` stays/updates to report single-threaded time for an 860×360 image with two 35×35 ring templates.
- [x] **Step 2: Run `cargo test -p vision`**, expected: the reference test passes already (current code is correct), the fast-path test fails to compile (missing `resize_area_general`). That is the RED state.
- [x] **Step 3: Implement** the fast path and the restructured `match_masked`.
- [x] **Step 4: Run** `cargo test -p vision --release`, then `cargo test -p altar --release --test golden -- --ignored` (unchanged result), then measure: a small `#[ignore]` test in `modules/altar/tests/detect.rs` named `bench_find_icons_screen_02` that loads `tests/fixtures/screens/screen_02.png` (skip if absent), runs `to_gray` + `find_icons` 20× and prints the median. Expected ≤ 40 ms.
- [x] **Step 5: Commit** `git commit -am "Live: single-threaded vectorized detection, exact integer downscale"` (add new files first).

### Task 2: Core crate: app paths, settings and profiles

**Files:**
- Create: `crates/core/Cargo.toml` (package `pov-core`), `crates/core/src/lib.rs`, `crates/core/src/paths.rs`, `crates/core/src/settings.rs`, `crates/core/src/profiles.rs`
- Modify: `Cargo.toml` (workspace member)

**Interfaces:**
- Produces:
  - `pub const APP_ID: &str = "io.github.miicrop.pov";`, `pub const USER_DIR_NAME: &str = "miicrop-pov";`
  - `pub struct AppPaths { pub data: PathBuf /* shipped, read-only */, pub user: PathBuf }` with `AppPaths::installed() -> anyhow::Result<AppPaths>` (`data` = folder `data` next to the exe, `user` = `%APPDATA%\miicrop-pov`), `AppPaths::dev() -> AppPaths` (`data` = repo `data/`, `user` = `%APPDATA%\miicrop-pov`), `AppPaths::with(data, user)`; `settings_file()`, `profiles_dir(module: &str)` (= `user/profiles/<module>`), `shipped_profiles_dir(module)` (= `data/profiles` for `"altar"`).
  - `pub struct Settings { pub general: General, pub altar: AltarSettings }` (serde; JSON shape `{"general": {...}, "modules": {"altar": {...}}}`), `General { autostart: bool /* false */, client_log: Option<PathBuf> /* None */, try_unsupported_size: bool /* false */ }`, `AltarSettings { enabled: bool /* true */, profile: String /* "default.json" */, hud_bottom: i64 /* 60 */, hud_offset_x: i64 /* 0 */, hud_font_size: i64 /* 15 */, hud_tiers: String /* "highlight" */, hud_other_opacity: f64 /* 0.6 */, hud_values: bool /* true */, board_cards: String /* "short" */, board_target_tag: bool /* true */ }`.
  - `pub struct SettingsStore` with `open(paths: &AppPaths) -> SettingsStore`, `get(&self) -> &Settings`, `reload_if_changed(&mut self) -> bool` (by file mtime), `save(&self, s: &Settings) -> anyhow::Result<()>` (pretty JSON, atomic write via temp file + rename).
  - `pub fn clean_altar(s: AltarSettings) -> AltarSettings` — port of `prototype/settings.py` `clean()` with the same ranges: font 10–24, tiers `off`/`highlight` (`text`/`both` → `highlight`), opacity 0.1–1.0 rounded to 2 decimals, bottom 0–2000, offset −3000–3000, cards `full`/`short`/`compact`; invalid values → defaults.
  - `pub fn active_profile_path(paths: &AppPaths, s: &Settings) -> PathBuf`: `user/profiles/altar/<profile>` if it exists, else `data/profiles/<profile>` if it exists, else `data/profiles/default.json`.
  - `pub fn import_prototype(paths: &AppPaths, repo: &Path) -> anyhow::Result<ImportReport>`: copies `repo/data/profiles/*.json` except `default.json` and `empty.json` into `user/profiles/altar/` (never overwriting existing files), and if `user/settings.json` does not exist, creates it from `repo/settings.json` (prototype keys become `modules.altar.*`). `ImportReport { profiles: Vec<String>, settings: bool }`.

- [x] **Step 1: Write the failing tests** (`crates/core/src/*.rs`, all on `tempfile::tempdir()` paths via `AppPaths::with`):
  - `missing_settings_file_gives_defaults`, `invalid_settings_file_gives_defaults` (file contains `{not json`), `partial_settings_keep_other_defaults` (`{"modules": {"altar": {"hud_font_size": 20}}}` → font 20, rest default).
  - `clean_altar_clamps_like_the_prototype`: font 99 → 24, font "x" → 15 (via JSON with a string), tiers "both" → "highlight", opacity 0.04 → 0.1, opacity 0.678 → 0.68, cards "weird" → "short".
  - `save_then_reload_round_trips` and `reload_if_changed_sees_external_edits` (write the file directly after `open`, bump mtime, `reload_if_changed()` returns true and `get()` shows the new value).
  - `active_profile_prefers_user_dir`, `active_profile_falls_back_to_shipped`, `missing_profile_name_falls_back_to_default` (path resolution only; a profile file with broken JSON is handled by `ProfileCache` in Task 6).
  - `import_prototype_copies_user_profiles_and_settings`: fake repo dir with `data/profiles/{default,empty,maps-eater}.json` and `settings.json` `{"profile": "maps-eater.json", "hud_font_size": 15}` → only `maps-eater.json` copied, settings created with `modules.altar.profile == "maps-eater.json"`; second run copies nothing and leaves settings untouched.
- [x] **Step 2: Run `cargo test -p pov-core`**, expected FAIL (compile).
- [x] **Step 3: Implement** the three modules.
- [x] **Step 4: Run `cargo test -p pov-core`**, expected PASS.
- [x] **Step 5: Commit** `git add crates/core Cargo.toml Cargo.lock && git commit -m "Live: core crate with app paths, settings and profiles"`

### Task 3: Game log watcher

**Files:**
- Create: `crates/game/Cargo.toml` (package `pov-game`), `crates/game/src/lib.rs`, `crates/game/src/log.rs`
- Modify: `Cargo.toml` (workspace member)

**Interfaces:**
- Produces:
  - `#[derive(Clone, Debug, PartialEq)] pub enum AreaKind { Map, Hideout, Town, Other }`, `#[derive(Clone, Debug, PartialEq)] pub struct Area { pub id: String, pub level: u32, pub kind: AreaKind }`
  - `pub fn parse_area_line(line: &str) -> Option<Area>` — matches `Generating level (\d+) area "([^"]+)"`.
  - `pub fn classify(id: &str) -> AreaKind`: `Hideout` if it contains `Hideout`; `Map` if it starts with `Map` (e.g. `MapWorldsDunes`); `Town` if it contains `_town` or `town` at the end (e.g. `2_11_endgame_town`); else `Other`.
  - `pub fn log_files(poe_dir: &Path) -> Vec<PathBuf>`: existing `logs/LatestClient.txt` first, then `logs/Client.txt`.
  - `pub struct LogWatcher` with `LogWatcher::new(path: PathBuf) -> LogWatcher`, `poll(&mut self) -> Option<Area>` (returns the current area: on the first poll reads only the last 256 KiB to find the newest area line, then only new bytes since the last poll; handles truncation/rotation by restarting from the end; a missing file returns `None` without error).

- [x] **Step 1: Write the failing tests** (`log.rs`):
  - `parses_the_real_line_format`: the verified line from Global Constraints → `Area { id: "MapWorldsDunes", level: 83, kind: Map }`; a hideout line `HideoutWorldTurtle` → `Hideout`; `2_11_endgame_town` → `Town`; a chat line containing the word "area" → `None`.
  - `watcher_follows_appended_lines` (temp file: write a hideout line, poll → Hideout; append a map line, poll → Map; poll again without changes → still Map).
  - `reads_only_the_tail_of_a_large_log`: temp file with 50 MiB of filler lines, then one map line → first poll returns Map, and the watcher read at most 256 KiB + appended bytes (expose `bytes_read()` for the test).
  - `missing_log_gives_unknown_area`: path that does not exist → `None`, no panic; creating the file later and appending a line → next poll returns that area.
  - `truncated_log_restarts`: file shrinks (rewrite with one town line) → poll returns Town.
- [x] **Step 2: Run `cargo test -p pov-game`**, expected FAIL (compile).
- [x] **Step 3: Implement** `log.rs` (`regex` crate is enough here; no look-around needed).
- [x] **Step 4: Run `cargo test -p pov-game`**, expected PASS.
- [x] **Step 5: Commit** `git add crates/game Cargo.toml Cargo.lock && git commit -m "Live: game log watcher"`

### Task 4: PoE window tracker and game context

**Files:**
- Create: `crates/game/src/window.rs`, `crates/game/src/context.rs`

**Interfaces:**
- Consumes: `LogWatcher`, `log_files`, `Area` (Task 3).
- Produces:
  - `#[derive(Clone, Copy, Debug, PartialEq)] pub struct Rect { pub x: i32, pub y: i32, pub w: i32, pub h: i32 }`
  - `#[derive(Clone, Debug)] pub struct PoeWindow { pub hwnd: isize, pub pid: u32, pub exe: PathBuf, pub client: Rect /* screen coords, physical px */, pub foreground: bool, pub minimized: bool }`
  - `pub fn find_poe_window() -> Option<PoeWindow>`: `FindWindowW("POEWindowClass", None)`; pid via `GetWindowThreadProcessId`; exe via `QueryFullProcessImageNameW`; client rect via `GetClientRect` + `ClientToScreen`; `foreground = GetForegroundWindow() == hwnd`; `minimized = IsIconic`.
  - `pub fn enable_dpi_awareness()`: `SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2)`, errors ignored (already set).
  - `#[derive(Clone, Debug, Default)] pub struct GameContext { pub poe: Option<PoeWindow>, pub area: Option<Area> }` with `in_map(&self) -> bool` and `poe_active(&self) -> bool` (`poe` is some, foreground, not minimized).
  - `pub trait ContextSource: Send { fn current(&mut self) -> GameContext; }`
  - `pub struct LiveContext` (implements `ContextSource`): `LiveContext::new(log_override: Option<PathBuf>)`; looks up the window at most every 1 s (cached in between, refreshed every call for `foreground`/`client` cheaply), creates its `LogWatcher` from `log_override` or the PoE exe folder (`log_files(exe.parent())`, first existing) when the window is first found, and polls it every call.
  - `pub struct FixedContext(pub GameContext)` (implements `ContextSource`, for replay and tests).

- [x] **Step 1: Write the failing tests** (`context.rs`): `in_map_and_poe_active_rules` (table over: no window; window minimized; window not foreground; foreground + area Map; foreground + area None (unknown); foreground + Hideout) checking `poe_active()` and `in_map()`; `fixed_context_returns_its_value`. Window functions are Win32 calls without a deterministic test target: add one test `find_poe_window_is_none_or_consistent` that only asserts consistency if a window is found (`client.w > 0`, `exe` ends with `.exe`), so it passes with or without the game running.
- [x] **Step 2: Run `cargo test -p pov-game`**, expected FAIL (compile).
- [x] **Step 3: Implement** `window.rs` and `context.rs` (`windows` features: `Win32_Foundation`, `Win32_UI_WindowsAndMessaging`, `Win32_System_Threading`, `Win32_UI_HiDpi`, `Win32_Graphics_Gdi`).
- [x] **Step 4: Run `cargo test -p pov-game`**, expected PASS.
- [x] **Step 5: Commit** `git commit -m "Live: PoE window tracker and game context"` (after `git add`).

### Task 5: Window capture (Windows.Graphics.Capture)

**Files:**
- Create: `crates/capture/Cargo.toml` (package `pov-capture`), `crates/capture/src/lib.rs`, `crates/capture/tests/capture.rs`
- Modify: `Cargo.toml` (workspace member)

**Interfaces:**
- Consumes: `vision::Rgb::from_bgra(w, h, stride, bytes)`, `pov_game::PoeWindow`.
- Produces:
  - `pub trait FrameSource: Send { fn grab(&mut self, window: &PoeWindow) -> anyhow::Result<Option<Rgb>>; fn pause(&mut self); }`
  - `pub struct WindowCapture` (implements `FrameSource`), `WindowCapture::new() -> anyhow::Result<WindowCapture>` (fails if `GraphicsCaptureSession::IsSupported()` is false).
  - `pub struct ReplaySource` (implements `FrameSource`): `ReplaySource::new(paths: Vec<PathBuf>, hold: Duration) -> anyhow::Result<ReplaySource>`; ignores the window and cycles the images, each shown for `hold`.

Behavior of `WindowCapture`:
- Session starts lazily on the first `grab` for an `hwnd` (D3D11 device, `IGraphicsCaptureItemInterop::CreateForWindow`, `Direct3D11CaptureFramePool::CreateFreeThreaded` with 2 buffers, `IsCursorCaptureEnabled(false)`, `IsBorderRequired(false)` where available, errors from these two ignored). A different `hwnd` or a changed window size recreates the pool/session.
- `grab` takes the newest available frame (drain `TryGetNextFrame` and keep the last), copies it to a CPU-readable staging texture, maps it, and returns the **client area only** (window capture includes the frame of a windowed-mode window: crop by the offset between the window's extended frame bounds (`DwmGetWindowAttribute(DWMWA_EXTENDED_FRAME_BOUNDS)`) and `PoeWindow.client`), via `Rgb::from_bgra` with the mapped row pitch. No new frame yet → return the last frame again; none ever → `Ok(None)`.
- `pause` closes the session and frame pool (no capture while no module is active).
- Window capture never contains our own overlay windows, which is why the window is captured instead of the monitor.

- [x] **Step 1: Write the failing tests** (`crates/capture/tests/capture.rs`):
  - `captures_a_solid_window`: create a top-level Win32 test window (300×200 client, class background brush of solid RGB(200, 30, 60)), show it, pump messages ~300 ms, build a `PoeWindow` for it with `find`-like helpers (`client` from `GetClientRect`+`ClientToScreen`), call `grab` up to 20× with 50 ms pauses until `Some`, then assert the size is 300×200 and the center pixel is `[200, 30, 60]` ±3. Skip (print and return) if `GraphicsCaptureSession::IsSupported()` is false.
  - `pause_then_grab_restarts`: after `pause`, `grab` returns a frame again.
  - `replay_cycles_images`: two 4×4 PNGs written to a temp dir, `hold` 50 ms: first grab returns image A, after 60 ms image B.
- [x] **Step 2: Run `cargo test -p pov-capture`**, expected FAIL (compile).
- [x] **Step 3: Implement** `lib.rs` (`windows` features for `Graphics_Capture`, `Graphics_DirectX_Direct3D11`, `Win32_Graphics_Direct3D11`, `Win32_Graphics_Dxgi`, `Win32_System_WinRT_Direct3D11`, `Win32_System_WinRT_Graphics_Capture`, `Win32_Graphics_Dwm`, plus those needed by the test window).
- [x] **Step 4: Run `cargo test -p pov-capture`**, expected PASS; report the time of one `grab` on a 3440×1440 window if the game or another large window is available (informational).
- [x] **Step 5: Commit** `git add crates/capture Cargo.toml Cargo.lock && git commit -m "Live: window capture with Windows.Graphics.Capture"`

### Task 6: Altar live tracker

**Files:**
- Create: `modules/altar/src/live.rs`
- Modify: `modules/altar/src/read.rs` (add `Reader::with_paths`), `modules/altar/src/lib.rs`

**Interfaces:**
- Consumes: `Reader`, `AltarRead`, `OptionRead`, `Icon`, `Match`, `Card`, `Side`, `Profile`, `OptionInput`, `OptionVerdict`, `Recommendation`, `judge`, `recommend`, `Pos`, `Verdict` (Plan A).
- Produces:
  - `Reader::with_paths(assets: &Path, mods_json: &Path, language: &str) -> anyhow::Result<Reader>` (`Reader::new()` becomes a call to it with the repo paths).
  - `pub struct AltarView { pub altar: AltarRead, pub complete: bool, pub options: [OptionInput; 2], pub verdicts: [OptionVerdict; 2], pub recommendation: Recommendation }`
  - `pub struct Tracker { /* tracks */ }` with `Tracker::new()`, `scan(&mut self, now: f64, icons: &[Icon], cards: &[Card], read: &mut dyn FnMut(&Icon) -> anyhow::Result<Option<AltarRead>>) -> Vec<&AltarRead>` returning the current tracked altars, `views(&self, cards: &[Card], profile: &Profile) -> Vec<AltarView>`, `clear(&mut self)`.
  - `pub struct ProfileCache` with `ProfileCache::new(fallback: PathBuf /* shipped default.json */)`, `get(&mut self, path: &Path) -> &Profile` (reloads when path or mtime changes; a file that fails to parse keeps the last good profile, or loads `fallback` if there never was one).

Port `prototype/live.py` `Scanner.scan` with its constants `TRACK_DISTANCE = 250`, `TRACK_TIMEOUT = 1.5`, `REREAD_INTERVAL = 0.5`, `SAME_ALTAR_OVERLAP = 0.6`, Manhattan icon distance, `_quality` ordering `(complete, match count, -partials, score sum)`, `_same_altar` multiset overlap over `(position, card id)`. Difference from the prototype (spec anti-goal: one bad read must not stop the scanner): an `Err` from `read` for one icon is logged once per altar (to stderr) and that icon is skipped for this scan.

- [x] **Step 1: Write the failing tests** (`modules/altar/tests/live.rs`, with hand-made `AltarRead` values built from real card ids in `data/mods.json`):
  - `new_altar_is_read_once_when_complete` (the read closure counts calls; three scans 0.25 s apart at the same icon → 1 read).
  - `incomplete_altar_is_reread_every_half_second` (calls at t = 0, 0.25 (no read), 0.5 (read)).
  - `moving_icon_keeps_its_track` (icon moves 200 px → same track, no new read).
  - `far_jump_is_matched_by_content` (icon moves 600 px, read returns the same mods → still one track).
  - `better_read_replaces_worse` (second read has more matches → kept; a worse later read is ignored but the icon position is updated).
  - `altar_is_dropped_after_timeout` (not seen for 1.6 s → gone).
  - `read_error_skips_only_that_altar` (two icons; read returns `Err` for the first → the second is tracked, the next scan reads the first again).
  - `views_apply_the_profile` (one altar with an upside in the profile's top column → `Verdict::Take` and "Take the top option.").
  - `profile_cache_reloads_on_change_and_survives_broken_files` (temp profile file; edit → new content; overwrite with `{broken` → still the last good profile; cache built on a broken file from the start → the fallback default).
- [x] **Step 2: Run `cargo test -p altar --test live`**, expected FAIL (compile).
- [x] **Step 3: Implement** `live.rs` and `Reader::with_paths`.
- [x] **Step 4: Run `cargo test -p altar`** (normal suite) and the golden test command, expected PASS / unchanged.
- [x] **Step 5: Commit** `git commit -m "Live: altar tracker and profile cache"` (after `git add`).

### Task 7: Module contract and runner

**Files:**
- Create: `crates/core/src/module.rs`, `crates/core/src/runner.rs`
- Modify: `crates/core/Cargo.toml` (depend on `pov-game`, `pov-capture`, `vision`), `crates/core/src/lib.rs`

**Interfaces:**
- Consumes: `GameContext`, `ContextSource` (Task 4), `FrameSource` (Task 5), `vision::Rgb`.
- Produces:
  - `pub struct ModuleOutput { pub module: &'static str, pub lines: Vec<String>, pub view: serde_json::Value }`
  - `pub trait Module: Send { fn id(&self) -> &'static str; fn name(&self) -> &'static str; fn interval(&self) -> Duration; fn wants_frames(&self, ctx: &GameContext) -> bool; fn on_frame(&mut self, frame: &Rgb, ctx: &GameContext) -> anyhow::Result<ModuleOutput>; fn on_inactive(&mut self); }`
  - `pub struct Runner` with `Runner::new(modules: Vec<Box<dyn Module>>, context: Box<dyn ContextSource>, frames: Box<dyn FrameSource>)`, `tick(&mut self) -> Vec<ModuleOutput>` (one step, for tests), `run(&mut self, stop: &AtomicBool, sink: &mut dyn FnMut(&[ModuleOutput]))` (loops `tick` at the smallest active module interval; sleeps 500 ms between context checks while no module is active).

`tick` rules: read the context; a module is active when `wants_frames(ctx)`; modules that became inactive get `on_inactive()` once; if none is active, call `frames.pause()` (once per transition) and return an empty list; otherwise grab one frame (`ctx.poe` must be `Some`, else treat as inactive) and give it to every active module whose interval has elapsed. A grab error or `Ok(None)` returns an empty list and is logged once until the next success. A module `Err` is logged once until its next success and yields no output for that module.

- [x] **Step 1: Write the failing tests** (`runner.rs`, with a fake module counting calls, a scripted `FixedContext`-like source and a fake `FrameSource` counting `grab`/`pause` calls):
  - `runner_pauses_capture_when_no_module_wants_frames` (context without PoE → no grab, one pause; then PoE active → grab; then PoE gone again → `on_inactive` once, pause once, no grab).
  - `runner_survives_frame_errors` (fake source returns `Err` twice, then a frame → no panic, output on the third tick).
  - `module_error_yields_no_output_but_keeps_running`.
  - `interval_is_respected` (module interval 1 h → `on_frame` once over several ticks).
- [x] **Step 2: Run `cargo test -p pov-core`**, expected FAIL (compile).
- [x] **Step 3: Implement** `module.rs` and `runner.rs`.
- [x] **Step 4: Run `cargo test -p pov-core`**, expected PASS.
- [x] **Step 5: Commit** `git commit -m "Live: module contract and runner"` (after `git add`).

### Task 8: Altar module and the `pov-scan` console app

**Files:**
- Create: `modules/altar/src/module.rs`, `apps/scan/Cargo.toml` (package `pov-scan`, binary `pov-scan`), `apps/scan/src/main.rs`, `apps/scan/src/stats.rs`
- Modify: `modules/altar/Cargo.toml` (depend on `pov-core`, `pov-game`), `modules/altar/src/lib.rs`, `Cargo.toml` (workspace member), `plans/Plan.md` (Phase 4 entry)

**Interfaces:**
- Consumes: everything above.
- Produces: `pub struct AltarModule` implementing `Module`, `AltarModule::new(paths: &AppPaths, settings: Arc<Mutex<SettingsStore>>) -> anyhow::Result<AltarModule>` (`Reader::with_paths(<altar assets>, paths.data/mods.json, "en-US")`; in dev the assets come from `altar::assets_dir()`).

`AltarModule` rules: `id() = "altar"`, `name() = "Altars"`, `interval() = 250 ms`; `wants_frames(ctx) = settings.altar.enabled && ctx.poe_active() && (ctx.area is None || ctx.in_map())`; `on_frame` = `to_gray` → `find_icons` → `Tracker::scan` (read via `Reader::read_altar`) → views with the profile from `ProfileCache` (path from `active_profile_path`, reloaded settings via `reload_if_changed`) → `ModuleOutput { lines, view }` where `lines` has one line per altar: `"<altar> top: <verdict label> [<mod ids>] | bottom: <verdict label> [<mod ids>] -> <recommendation text>"`, and `view` is `serde_json::json!` of the same data (ids, labels, recommendation text, complete flag); `on_inactive` clears the tracker.

`pov-scan` flags: `--replay <dir>` (uses `ReplaySource` with all `*.png` and a `FixedContext` with a fake foreground PoE window sized to the first image and area `Map`), `--hold <secs>` (default 3), `--stats` (every 10 s: scans/s, scan avg/max ms, process CPU % of total and of one core via `GetProcessTimes`, number of altars), `--import-prototype` (runs `import_prototype` with the repo root and prints the report, then exits), `--dev` (use `AppPaths::dev()`; default in debug builds). Live mode: `enable_dpi_awareness()`, `LiveContext::new(settings.general.client_log)`, `WindowCapture`. Prints the module lines only when they change, plus a status line when the context changes ("PoE not found", "in hideout – idle", "in map – scanning"). Ctrl+C stops cleanly (stop flag).

- [x] **Step 1: Write the failing test** (`modules/altar/tests/module.rs`): `module_reads_altars_on_a_screenshot` — skip if `tests/fixtures/screens/screen_02.png` is absent; temp `AppPaths` with the repo `data/` as `data`; a `FixedContext` with a foreground fake window and area Map; `wants_frames` true; `on_frame` with screen_02 → two lines, each containing `top:` and `->`. Also `module_is_idle_outside_maps` (Hideout → `wants_frames` false; area None + foreground → true; not foreground → false).
- [x] **Step 2: Run `cargo test -p altar --test module`**, expected FAIL (compile).
- [x] **Step 3: Implement** `module.rs` and `apps/scan`.
- [x] **Step 4: Verify:** `cargo test --workspace --release` green; golden test unchanged; `cargo run -p pov-scan --release -- --replay tests/fixtures/screens --hold 1 --stats` prints altar lines and, after 10 s, a stats line with CPU ≤ 3 % of total (report the numbers); `cargo run -p pov-scan --release` without the game prints "PoE not found" and the process stays at ~0 % CPU (check with the stats line).
- [x] **Step 5: Update `plans/Plan.md`** Phase 4 with a short "Plan B1 done" entry (what `pov-scan` does, measured CPU in replay, the `--import-prototype` step) and commit: `git add modules/altar apps/scan Cargo.toml Cargo.lock plans/Plan.md && git commit -m "Live: altar module and pov-scan console app"`
- [ ] **Step 6 (user, after the branch review):** the controller asks the user to run `pov-scan --import-prototype` once and then `pov-scan --stats` during a real mapping session (borderless 3440×1440): verdicts correct, CPU ≤ 3 % total, FPS unchanged. Results go into `plans/Plan.md`.
