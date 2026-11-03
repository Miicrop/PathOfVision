# Plan B2 – Tauri app (main window, tray, HUD, board, settings window) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The desktop app "Path of Vision": the user starts one exe, gets a main window (sidebar Home + Altars board), a tray icon, a separate settings window and the in-game altar HUD, all driven by the B1 runner, with feature parity to the Python prototype's UI.

**Architecture:** A new Tauri 2 crate `app` (package `pov-app`) hosts plain HTML/CSS/JS pages from `ui/` (no build step, `withGlobalTauri`). The B1 runner runs on its own scanner thread; every tick it emits the runner status to the main window and the altar HUD view model to the HUD window. All logic that can be tested without Tauri lives in the crates: settings schema and merging store (`crates/core`), board file operations, labels and HUD view model (`modules/altar`), Win32 overlay helpers (new `crates/overlay`). `app` is thin glue: windows, tray, commands, events.

**Tech Stack:** Rust stable (edition 2024), `tauri` 2.x (features `tray-icon`, `image-png`), `tauri-build` 2.x, `tauri-plugin-single-instance` 2.x, `tauri-plugin-autostart` 2.x, `windows` 0.61, `serde`/`serde_json`, `anyhow`, `tempfile` (tests). No Node, no bundler, no Tauri CLI needed: `cargo run -p pov-app`.

**Spec:** `docs/superpowers/specs/2026-11-03-path-of-vision-port-design.md` (sections 2, 3, 4, 5, 6, 7, 9). Carry-over: `plans/Plan.md`, Phase 4, "Carry-over into Plan B" (item 4) and "Carry-over into Plan B2". Out of scope (Plan C): resolution scaling and `try_unsupported_size`, installer, updater ("update available" on Home), README, CI, bundling `data/` as resources and dropping the dev-data fallback (B2 carry-over 4), `FrameArrived` capture (B2 carry-over 5, latency only).

## Global Constraints

- ToS: passive screen reading only; never send input to the game, no memory reading, no injection. The HUD is a separate click-through, topmost window that never takes focus.
- Privacy: no network access in this plan at all (no updater yet). No remote URLs in the pages: every script, style, font and image is a local file.
- In-game only overlays are visible: no popups, no focus stealing (spec 5, anti-goals).
- No capture while no module is active; PoE not running or player not in a map: ~0 % CPU. Mapping with the altar module active: ≤ 3 % total CPU, no measurable FPS loss (spec 9).
- Permanent internal ID `io.github.miicrop.pov` (Tauri `identifier`); user data folder `%APPDATA%\miicrop-pov\`; display name "Path of Vision" (`productName`, window titles, tray tooltip).
- User data layout unchanged from B1: `settings.json` with `general` and `modules.<id>`, profiles in `profiles/altar/*.json`. Shipped `data/` stays read-only. Tests use temp dirs and never touch the repo's `settings.json`, `data/profiles/*` (other than reading `default.json`/`empty.json`) or `%APPDATA%`.
- The board's look and behavior stay as they are (spec 7); the only board changes are the backend adapter, the removed "Settings" button/panel, live card-style/target-tag updates and the sidebar.
- UI pages are plain HTML/CSS/JS, English, styled with the board's color tokens (`ui/board/board.css` `:root`).
- Plan A/B1 results must not regress: `cargo test -p altar --release --test golden -- --ignored` stays at 44/44, +0 false, exactly the one known miss.
- cargo is at `%USERPROFILE%\.cargo\bin` (not on PATH in bash). Commit messages plain, no `Co-Authored-By` trailer.

## Review Focus

1. **HUD steals focus or eats clicks** (the player types in chat or clicks while the HUD shows): the game keeps focus and every click goes through → Task 5 tests `show_does_not_take_focus`, `overlay_window_is_click_through`.
2. **Settings written from several places** (settings window, board, Home switch, hand edits): unknown keys and other sections survive, bad values are clamped, never a crash → Task 1 test `update_keeps_unknown_keys_and_clamps`, `update_rejects_unknown_key`.
3. **Profile edge cases** (duplicate to an existing name, names with only symbols, delete the active profile, save to the read-only default, file names with `..`): nothing outside the profile folders is touched, read-only stays read-only → Task 4 tests `duplicate_picks_a_free_file_name`, `delete_active_profile_falls_back_to_default`, `read_only_profiles_cannot_be_saved_or_deleted`, `file_names_cannot_escape_the_profile_folder`.
4. **PoE window on another monitor, moved, or at a different DPI**: the HUD sits bottom-center of the PoE client area with the user's offsets → Task 5 test `hud_rect_follows_the_client_rect` (negative monitor coordinates, offsets, HUD wider than the client).
5. **Missing or broken `labels.json`/`keywords.json`, or a mod without alias/icon**: the HUD shows the full mod text without icon instead of failing → Task 3 tests `broken_label_files_give_empty_maps`, `mod_without_alias_shows_full_text`.

---

### Task 1: Core: merging settings store, settings schema, runner status

**Files:**
- Modify: `crates/core/src/settings.rs`, `crates/core/src/runner.rs`, `crates/core/src/lib.rs`, `crates/core/Cargo.toml`, `apps/scan/src/main.rs`
- Create: `crates/core/src/schema.rs`, `crates/core/src/status.rs`
- Move: `apps/scan/src/stats.rs` → `crates/core/src/stats.rs` (unchanged apart from visibility and imports; `pov-scan` imports it from `pov_core::stats`)

**Interfaces:**
- Produces:
  - `SettingsStore` keeps the raw JSON of the file next to the typed `Settings`. `save` is replaced by `update(&mut self, section: &str, key: &str, value: serde_json::Value) -> anyhow::Result<Settings>` (`section` = `"general"` or a module id, stored under `modules.<id>`): merges into the raw JSON, re-parses with `Settings::from_value` + `clean_altar`, writes the cleaned value of that key back into the raw JSON, writes atomically, returns the new settings. Unknown section or key → `Err`. `reset(&mut self, section: &str, keys: &[&str]) -> anyhow::Result<Settings>` sets the keys to their defaults. `save` stays only as `pub(crate)` for `import_prototype`.
  - `schema.rs`: `#[derive(Serialize)] pub struct SettingsSection { pub id: &'static str, pub name: &'static str, pub groups: Vec<SettingsGroup> }`, `pub struct SettingsGroup { pub name: &'static str, pub reset: bool, pub fields: Vec<Field> }`, `pub struct Field { pub key: &'static str, pub label: &'static str, pub help: &'static str, pub kind: FieldKind }`, `#[serde(tag = "type", rename_all = "lowercase")] pub enum FieldKind { Toggle, Choice { options: Vec<(&'static str, &'static str)> }, Slider { min: f64, max: f64, step: f64, scale: f64, unit: &'static str }, Number { min: i64, max: i64, step: i64, unit: &'static str }, Path { placeholder: &'static str } }`, `pub fn settings_schema() -> Vec<SettingsSection>`.
  - `status.rs`: `#[derive(Clone, Debug, PartialEq, Serialize)] #[serde(rename_all = "snake_case")] pub enum PoeState { NotFound, Background, Foreground }`, `#[derive(Clone, Debug, PartialEq, Serialize)] pub struct RunnerStatus { pub poe: PoeState, pub area: Option<String>, pub area_kind: Option<String>, pub active: Vec<String>, pub text: String }` (`area` = area id, `area_kind` = `"map" | "hideout" | "town" | "other"`, `text` = `describe()`), `RunnerStatus::from_context(ctx: &GameContext, active: Vec<String>) -> RunnerStatus`, `RunnerStatus::describe(&self) -> String`.
  - `runner.rs`: `pub struct Tick { pub status: RunnerStatus, pub poe: Option<PoeWindow>, pub outputs: Vec<ModuleOutput> }` (`poe` = the context's window, for the HUD position); `Runner::tick(&mut self) -> Tick`; `Runner::run(&mut self, stop: &AtomicBool, sink: &mut dyn FnMut(&Tick))` calls `sink` on every tick. `Ok(None)` from the frame source is silent (B2 carry-over 9); `Err` is still logged once.

Schema content (labels and help copied from today's board panel, `ui/board/index.html` lines 31–78):
- `general` "General", group "App" (`reset: false`): `autostart` Toggle "Start with Windows"; `client_log` Path "Game log (Client.txt)", placeholder "found automatically", help "Only needed if Path of Exile is installed somewhere unusual."
- `altar` "Altars", group "Board cards" (`reset: false`): `board_cards` Choice [("short","Short + text"),("full","Full text"),("compact","Compact")] "Text"; `board_target_tag` Toggle "Target tag". Group "Overlay" (`reset: true`): `hud_font_size` Slider 10–24 step 1 scale 1 unit "px" "Text size", help "icons scale with the text"; `hud_tiers` Choice [("highlight","Highlight"),("off","Off")] "Tiers"; `hud_values` Toggle "Values"; `hud_other_opacity` Slider 0.1–1.0 step 0.05 scale 100 unit "%" "Alpha", help "of the option that is not the pick (ADAPT - BRICK is never dimmed)"; `hud_offset_x` Number −3000–3000 step 10 "px" "Position X"; `hud_bottom` Number 0–2000 step 5 "px" "Position Y", help "Anchor: bottom center of the game window. X moves right (negative: left), Y moves up."

`describe()` texts: NotFound → "PoE not found"; Background → "PoE in background – idle"; Foreground with active modules → "in map – scanning" (area kind map) / "area unknown – scanning" (no area); Foreground without active modules → "in hideout – idle", "in town – idle", "outside maps – idle", "in map – idle (no module enabled)", "area unknown – idle (no module enabled)". `pov-scan` prints `describe()` on change instead of its own status logic (B2 carry-over 6).

- [ ] **Step 1: Write the failing tests** in `settings.rs`, `schema.rs`, `status.rs`, `runner.rs`:
  - `update_keeps_unknown_keys_and_clamps`: file `{"general":{"autostart":false,"x":1},"modules":{"altar":{"hud_bottom":60,"future":"keep"},"other":{"a":2}},"top":true}`; `update("altar","hud_font_size",json!(99))` → returned `altar.hud_font_size == 24`; the file afterwards still has `general.x == 1`, `modules.altar.future == "keep"`, `modules.other.a == 2`, `top == true`, and `modules.altar.hud_font_size == 24`.
  - `update_rejects_unknown_key`: `update("altar","nope",json!(1))` and `update("ghost","enabled",json!(true))` are `Err`, file unchanged.
  - `update_general_client_log`: `update("general","client_log",json!("D:/PoE/logs/Client.txt"))` → `general.client_log == Some(..)`; `json!("")` or `null` → `None`.
  - `reset_restores_defaults`: after `update("altar","hud_bottom",json!(300))`, `reset("altar",&["hud_bottom"])` → 60.
  - `schema_keys_exist_in_settings`: every `Field.key` of section `altar` is a key of `serde_json::to_value(AltarSettings::default())`, same for `general`; every Choice option value passes `clean_altar` unchanged.
  - `status_describes_every_state` (one assert per text above, contexts built from `PoeWindow`/`Area` values like the runner tests).
  - existing runner tests use `tick().outputs`; new `tick_reports_status_even_when_idle`: no PoE → `tick().status.poe == PoeState::NotFound`, `active` empty.
- [ ] **Step 2: Run `cargo test -p pov-core`**, expected FAIL (compile).
- [ ] **Step 3: Implement** the interfaces above; move `stats.rs`; adapt `pov-scan` (`--replay`, `--stats` output unchanged except the status texts).
- [ ] **Step 4: Run `cargo test --workspace --release`**, expected PASS; `cargo run -p pov-scan --release -- --replay tests/fixtures/screens --hold 1` still prints altar lines.
- [ ] **Step 5: Commit** `git commit -m "Core: merging settings store, settings schema, runner status"`

### Task 2: Live robustness: log path from settings, capture device recovery

**Files:**
- Modify: `crates/game/src/context.rs`, `crates/capture/src/lib.rs`, `crates/core/src/lib.rs`
- Create: `crates/core/src/context.rs`

**Interfaces:**
- Consumes: `SettingsStore` (Task 1).
- Produces:
  - `pov_game::LogOverride` trait `{ fn set_log_override(&mut self, path: Option<PathBuf>); }`, implemented by `LiveContext`: a changed path drops the watcher and the area, the next `current()` builds a watcher for the new file; the same path is a no-op (B2 carry-over 7).
  - `pov_core::SettingsContext<C: ContextSource + LogOverride>` with `SettingsContext::new(inner: C, settings: Arc<Mutex<SettingsStore>>) -> Self`, implementing `ContextSource`: each `current()` calls `reload_if_changed`, passes `general.client_log` to `set_log_override` when it differs from the last value, then returns `inner.current()`.
  - `pov_capture::needs_device_reset(hresult: Option<i32>, consecutive_failures: u32) -> bool`: true for `DXGI_ERROR_DEVICE_REMOVED` (`0x887A0005u32 as i32`), `DXGI_ERROR_DEVICE_RESET` (`0x887A0007u32 as i32`) or `consecutive_failures >= 3`. `WindowCapture::reset_device(&mut self) -> anyhow::Result<()>` recreates the D3D11 device, the WinRT `IDirect3DDevice`, frame pool, session and staging texture. `grab` counts consecutive errors and calls `reset_device` when `needs_device_reset` says so, logging once per reset (B2 carry-over 1).
  - Doc comment on `WindowCapture::new`: construct and use it on the same thread (the app constructs it on the scanner thread, B2 carry-over 2).

- [ ] **Step 1: Write the failing tests:**
  - `context.rs` (game, with the existing finder seam): `log_override_switches_files`: log A ends with a `MapWorldsDunes` line, log B with a `HideoutFelled` line; start with override A → `in_map()`; `set_log_override(Some(B))` → next `current()` has `area.kind == AreaKind::Hideout`; `set_log_override(Some(B))` again does not re-read (watcher `bytes_read()` unchanged).
  - `crates/core/src/context.rs`: `settings_context_forwards_log_path_changes` with a fake `ContextSource + LogOverride` that records calls: temp settings without `client_log` → one call with `None`; after `store.update("general","client_log",json!("X"))` the next `current()` records `Some("X")`; a third `current()` records nothing new.
  - `crates/capture` unit test `device_reset_policy`: removed/reset HRESULTs → true; other HRESULT with 2 failures → false, with 3 → true; `None` with 0 → false.
  - `crates/capture/tests/capture.rs` `capture_survives_device_reset`: grab a frame of the test window, `reset_device()`, the next grabs (retry up to 1 s) return a frame of the same size.
- [ ] **Step 2: Run `cargo test -p pov-game -p pov-core -p pov-capture`**, expected FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run `cargo test --workspace --release`**, expected PASS.
- [ ] **Step 5: Commit** `git commit -m "Live: log path from settings, capture device recovery"`

### Task 3: Altar labels and HUD view model

**Files:**
- Create: `modules/altar/src/labels.rs`, `modules/altar/src/hud.rs`, `modules/altar/tests/hud.rs`
- Modify: `modules/altar/src/lib.rs`, `modules/altar/src/module.rs`, `modules/altar/src/fill.rs` (make `VALUE_SLOT_RE` `pub(crate)`), `modules/altar/tests/module.rs`

**Interfaces:**
- Consumes: `AltarView`, `Card`, `Match::display`, `OptionVerdict.tiers`, `Recommendation { choice, text, about }`.
- Produces (port of `prototype/labels.py` and the view logic of `prototype/overlay.py`):
  - `labels.rs`: `pub struct Label { pub alias: String, pub icon: String }`; `pub fn load_labels(path: &Path) -> HashMap<String, Label>` (`{"labels": {id: {alias, icon}}}`, trimmed; missing/broken file → empty); `pub fn load_keywords(path: &Path) -> Vec<(String, String)>` (`{"keywords": {word: color}}`, longest word first; missing/broken → empty); `pub fn render_alias(alias: &str, display: &[String]) -> String`; `#[derive(Serialize, Clone, Debug, PartialEq)] pub struct Segment { pub text: String, pub color: Option<String> }`; `pub fn color_keywords(text: &str, keywords: &[(String, String)]) -> Vec<Segment>` (whole words, any case, adjacent plain text merged; no HTML: the page escapes by using `textContent`); `#[derive(Serialize, Clone, Debug, PartialEq)] pub struct RolledValue { pub text: String, pub read: bool }`; `pub fn rolled_values(data: &[String], display: &[String]) -> Vec<RolledValue>`; `pub struct Watched<T> { .. }` with `Watched::new(path: PathBuf, load: fn(&Path) -> T) -> Watched<T>`, `refresh(&mut self) -> bool` (reload when the mtime changed), `get(&self) -> &T`.
  - `hud.rs` (all `#[derive(Serialize)]`): `HudStyle { font_size: i64, tiers: bool, other_opacity: f64, values: bool }` with `HudStyle::from_settings(s: &AltarSettings)` (`tiers` = `hud_tiers == "highlight"`); `HudView { style: HudStyle, altars: Vec<HudAltar> }`; `HudAltar { altar: &'static str /* "eater" | "exarch" */, complete: bool, recommendation: HudRec, options: Vec<HudOption> /* top, bottom */ }`; `HudRec { text: String, color: &'static str, arrow: &'static str /* "▲" | "▼" | "⚠" | "" */ }`; `HudOption { position: &'static str, target_icon: Option<String>, verdict: &'static str, color: &'static str, chosen: bool, dimmed: bool, mods: Vec<HudMod> }`; `HudMod { upside: bool, icon: Option<String>, text: Vec<Segment>, values: Vec<RolledValue>, tint: Option<&'static str> }`; `pub fn hud_view(views: &[AltarView], cards: &[Card], labels: &HashMap<String, Label>, keywords: &[(String, String)], style: HudStyle, center: (i32, i32)) -> HudView`; pure helpers `pub fn pick_altars(icons: &[(i32, i32)], center: (i32, i32)) -> Vec<usize>` and `pub fn is_dimmed(pick: Option<Pos>, position: Pos, verdict: Verdict, leave_alone: bool) -> bool`.
- `AltarModule` loads `Watched` labels (`paths.data/labels.json`) and keywords (`paths.data/keywords.json`), refreshes them each `on_frame`, and sets `ModuleOutput.view = serde_json::to_value(hud_view(.., HudStyle::from_settings(..), (frame.w / 2, frame.h / 2)))`. `view_json` is removed; `lines` stay as they are.

Rules copied from `prototype/overlay.py`: colors TAKE `#4fbf6a`, CONSIDER `#e0a33a`, ADAPT - BRICK `#d65db1`, AVOID `#e05a4f`, NEUTRAL `#9aa0a6`, UNREADABLE `#5d6168`; tints Top `#f2c84b`, Rated `#b48cf0`, Dangerous `#e0a33a`, Brick `#e05a4f`; at most 2 altars, the 2 closest to `center` (Manhattan distance), then sorted by x; `pick = choice.or(about)`; `chosen = pick == position`; dimmed = `(pick.is_some() && !chosen && verdict != Adapt) || leave_alone` where `leave_alone` = both verdicts AVOID; recommendation color = color of the picked option's verdict, `#e0a33a` without a pick; arrow ▲/▼ toward the pick, `""` without a pick, and `leave_alone` → AVOID color with `"⚠"`; mods upsides first, otherwise in match order; text = `render_alias(alias, display)` if the label has an alias, else `display.join(" / ")`, cut to 64 chars as 63 + "…"; keyword colors on downsides only; `target_icon` = `"target_{player|minions|boss}_{top|bottom}_{eater|exarch}.png"` when the target is known; `icon` = `"mods/{label.icon}.png"` when the label has an icon; `values` = `rolled_values(card lines of the variant, display)`.

- [ ] **Step 1: Write the failing tests:**
  - `labels.rs`: port `tests/test_labels.py` 1:1 (same inputs and expected strings; the keyword test asserts the segments `fire`→`#f00`, `Cold`→`#00f`, `Firestorm` plain, exactly 2 colored segments, `<x>` and `&` kept as plain text); `broken_label_files_give_empty_maps` (missing file, invalid JSON, JSON without `labels`/`keywords`); `watched_reloads_on_change`.
  - `hud.rs` unit tests: `pick_two_closest_then_left_to_right` (icons `[(100,500),(1700,700),(1800,720),(3000,700)]`, center `(1720,720)` → indices `[1, 2]`); `dimming_rules` (picked other + NEUTRAL → dimmed; picked other + ADAPT → not; chosen → not; leave_alone → dimmed; no pick → not).
  - `tests/hud.rs` (skip if `tests/fixtures/screens/screen_02.png` is absent): read with `Reader::read_altars`, track once, `views` with `data/profiles/default.json`, `hud_view` with the repo's labels/keywords → 2 altars, each with options `["top","bottom"]`, non-empty recommendation text, every option's `color` from the color table, all upside mods before downside mods; `mod_without_alias_shows_full_text`: same view with an empty label map → each mod text equals the joined display lines (≤ 64 chars) and `icon` is `None`.
  - `tests/module.rs`: the screen_02 test additionally asserts `out.view["altars"].as_array().len() == 2` and `out.view["style"]["font_size"] == 15`.
- [ ] **Step 2: Run `cargo test -p altar`**, expected FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run `cargo test --workspace --release`** and the golden test, expected PASS / unchanged.
- [ ] **Step 5: Commit** `git commit -m "Altar: labels and HUD view model"`

### Task 4: Altar board API (profiles and state, no Tauri)

**Files:**
- Create: `modules/altar/src/board.rs`, `modules/altar/tests/board.rs`
- Modify: `modules/altar/src/lib.rs`

**Interfaces:**
- Consumes: `AppPaths`, `SettingsStore::update` (Task 1), `load_labels`, `load_keywords` (Task 3).
- Produces (port of `prototype/board.py` `BoardApi`; all results are `serde_json::Value` with the same JSON shapes the board uses today):
  - `pub struct BoardApi`, `BoardApi::new(paths: AppPaths, settings: Arc<Mutex<SettingsStore>>) -> BoardApi`
  - `get_state(&self) -> anyhow::Result<Value>` → `{gameVersion, mods: [{id, side, target, altars, lines, weights, icon, alias}], keywords: {word: color}, profiles, active}` (`mods` from `paths.data/mods.json`, `icon`/`alias` from labels, `""` when absent)
  - `load_profile(&self, file: &str) -> anyhow::Result<Value>` (raw JSON, `upsides` upgraded, `downsides` defaulted to `{}`)
  - `save_profile(&self, file: &str, profile: Value) -> anyhow::Result<Value>` → `{"ok": true}` or `{"ok": false, "error": "read-only profile"}`; writes the raw JSON (unknown keys and key order survive, Plan A carry-over 4) with `readOnly: false`, 2-space indent and a trailing newline, into the user profile folder
  - `duplicate_profile(&self, file: &str, name: &str) -> anyhow::Result<Value>` → `{file, profiles}`; the copy gets `name`, `readOnly: false`, lands in the user folder and becomes active
  - `delete_profile(&self, file: &str) -> anyhow::Result<Value>` → `{"ok": true, profiles, active}` or the read-only error; deleting the active profile makes `default.json` active
  - `set_active(&self, file: &str) -> anyhow::Result<()>` via `update("altar", "profile", ..)`
  - `pub fn profile_file_name(name: &str, taken: impl Fn(&str) -> bool) -> String` ("My Scarab Farm" → `my-scarab-farm.json`, then `-2`, `-3`, …; only symbols → `profile.json`)
  - `pub fn upgrade_upsides(upsides: &Value) -> Value` (port of `prototype/verdict.py` `upgrade_upsides`)
- Profile files: a name resolves to `profiles_dir("altar")/<file>` first, then `shipped_profiles_dir("altar")/<file>`. Shipped files are always read-only; user files are read-only only if `readOnly` is true. `profiles` = union of both folders as `{file, name, readOnly}`, read-only first, then by lowercase name. A file name that is not a bare `*.json` file name → `Err`. `profile_file_name` treats names in both folders as taken.

- [ ] **Step 1: Write the failing tests** in `tests/board.rs` (temp user dir, the repo `data/` as shipped data, settings in the temp dir):
  - `state_lists_mods_profiles_and_active`: `mods.len()` equals the card count of `mods.json`, every mod has the 8 keys, `profiles` starts with the read-only `default.json`, `active == "default.json"`.
  - `duplicate_picks_a_free_file_name`: duplicating `default.json` as "My Farm" twice gives `my-farm.json` then `my-farm-2.json`, both in the user folder, the last one active; `profile_file_name("!!!", |_| false) == "profile.json"`.
  - `save_keeps_unknown_keys`: save `{..., "extra": {"a": 1}}` to a user profile, file contains `"extra"`, `readOnly` false, ends with `\n`.
  - `read_only_profiles_cannot_be_saved_or_deleted`: `save_profile("default.json", ..)` and `delete_profile("default.json")` return `ok: false` and the shipped file is byte-identical afterwards.
  - `delete_active_profile_falls_back_to_default`.
  - `file_names_cannot_escape_the_profile_folder`: `load_profile("../settings.json")`, `save_profile("..\\x.json", ..)`, `delete_profile("a/b.json")` are `Err`.
  - `old_upsides_are_upgraded`: `upgrade_upsides(&json!({"s":["a"],"a":["b"],"b":["c"]}))` == columns `[{"name":"B","ids":["c"]},{"name":"A","ids":["b"]}]`, top `{"name":"S","ids":["a"]}`; a column with `"key": true` keeps it, without it has no `key` field.
- [ ] **Step 2: Run `cargo test -p altar --test board`**, expected FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run `cargo test --workspace --release`**, expected PASS.
- [ ] **Step 5: Commit** `git commit -m "Altar: board API for profiles and state"`

### Task 5: Overlay service (Win32 helpers for the HUD window)

**Files:**
- Create: `crates/overlay/Cargo.toml` (package `pov-overlay`), `crates/overlay/src/lib.rs`, `crates/overlay/tests/overlay.rs`
- Modify: `Cargo.toml` (workspace member)

**Interfaces:**
- Consumes: `pov_game::Rect`.
- Produces:
  - `pub fn hud_rect(client: Rect, size: (i32, i32), offset_x: i64, bottom: i64) -> Rect`: bottom-center of the client area, `x = client.x + (client.w - w) / 2 + offset_x`, `y = client.y + client.h - h - bottom`, all in physical pixels.
  - `pub fn make_overlay(hwnd: isize) -> anyhow::Result<()>`: adds `WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TOPMOST` to the extended style.
  - `pub fn show_at(hwnd: isize, rect: Rect) -> anyhow::Result<()>`: `SetWindowPos(HWND_TOPMOST, rect, SWP_NOACTIVATE | SWP_SHOWWINDOW)`; never `SW_SHOW`/`SetForegroundWindow`.
  - `pub fn hide(hwnd: isize) -> anyhow::Result<()>`: `ShowWindow(SW_HIDE)`.

- [ ] **Step 1: Write the failing tests:**
  - unit `hud_rect_follows_the_client_rect`: client `{x:0,y:0,w:3440,h:1440}`, size `(800,300)`, offsets `(0,60)` → `{x:1320,y:1080,w:800,h:300}`; client on a left monitor `{x:-2560,y:0,w:2560,h:1440}`, offsets `(-100,0)` → `x == -2560 + 880 - 100`, `y == 1140`; HUD wider than the client (size `(1000,100)`, client w 800) → `x == client.x - 100`.
  - `tests/overlay.rs` (real windows, like `crates/capture/tests/capture.rs`): `show_does_not_take_focus`: create and focus window A, create window B, `make_overlay(B)`, `show_at(B, ..)` → `GetForegroundWindow() == A` and B is visible; `overlay_window_is_click_through`: after `make_overlay(B)` its extended style contains `WS_EX_TRANSPARENT | WS_EX_NOACTIVATE | WS_EX_LAYERED`.
- [ ] **Step 2: Run `cargo test -p pov-overlay`**, expected FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run `cargo test -p pov-overlay`**, expected PASS.
- [ ] **Step 5: Commit** `git commit -m "Overlay: Win32 helpers for click-through HUD windows"`

### Task 6: Tauri app shell: main window, tray, single instance, scanner thread, Home

**Files:**
- Create: `app/Cargo.toml` (package `pov-app`, binary `pov-app`), `app/build.rs`, `app/tauri.conf.json`, `app/capabilities/default.json`, `app/icons/icon.ico`, `app/icons/icon.png`, `app/src/main.rs`, `app/src/state.rs`, `app/src/scanner.rs`, `app/src/commands.rs`, `ui/shell/shell.css`, `ui/shell/shell.js`, `ui/shell/home.html`, `ui/shell/home.js`
- Modify: `Cargo.toml` (workspace member), `.gitignore` (`app/gen/`)

**Interfaces:**
- Consumes: Tasks 1–5, `AltarModule`, `LiveContext`, `WindowCapture`, `ReplaySource`, `FixedContext`, `pov_core::stats`.
- Produces:
  - `tauri.conf.json`: `productName` "Path of Vision", `identifier` "io.github.miicrop.pov", `build.frontendDist` "../ui", `app.withGlobalTauri` true, no windows in the config (all created in Rust), `app.security.csp` `"default-src 'self'; img-src 'self' data:; style-src 'self' 'unsafe-inline'; connect-src ipc: http://ipc.localhost"`, `bundle.active` false (installer is Plan C).
  - `state.rs`: `pub struct AppState { pub paths: AppPaths, pub settings: Arc<Mutex<SettingsStore>>, pub status: Arc<Mutex<RunnerStatus>>, pub poe: Arc<Mutex<Option<PoeWindow>>>, pub board: BoardApi, pub stop: Arc<AtomicBool> }` (`poe` is updated from `Tick.poe` every tick).
  - `scanner.rs`: `pub fn spawn(app: AppHandle, state: &AppState, mode: Mode) -> JoinHandle<()>` with `pub enum Mode { Live, Replay { dir: PathBuf, hold: Duration } }`; on the scanner thread it builds `SettingsContext<LiveContext>` + `WindowCapture` (live) or `FixedContext` with a foreground fake window covering the primary monitor's top-left `image size` + `ReplaySource` (replay), the `AltarModule`, and a `Runner`; each tick it stores the status, emits `app:status` (only on change) to window `main`, and emits `altar:hud` (the altar output's `view`, or `{"altars": []}` once when the altar module stops being active) to window `hud`. `--stats` wraps the module with `pov_core::stats` and prints the stats line to stderr.
  - `commands.rs`: `app_status() -> RunnerStatus`, `open_settings(section: Option<String>)`, `settings_schema() -> Vec<SettingsSection>`, `settings_get() -> Value`, `settings_set(section: String, key: String, value: Value) -> Result<Value, String>`, `settings_reset(section: String, keys: Vec<String>) -> Result<Value, String>`. `settings_set`/`settings_reset` emit `settings:changed` (the full settings JSON) to all windows; a change of `general.autostart` enables/disables autostart via `tauri-plugin-autostart`.
  - `main.rs`: `#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]`; flags `--replay <dir>`, `--hold <secs>` (default 3), `--stats`, `--dev` (default in debug builds; release falls back to dev data like `pov-scan`, removed in Plan C); single-instance plugin registered first (a second start shows and focuses `main`); the main window (`ui/shell/home.html`, title "Path of Vision", 90 % of the primary monitor's work area, centered); close request on `main` → hide instead of quit; tray icon (tooltip "Path of Vision", menu "Open Path of Vision", "Settings", "Quit"; left click opens `main`); "Quit" sets `stop`, joins the scanner thread, then exits.
  - `ui/shell/shell.js`: injects the left sidebar (entries "Home" → `/shell/home.html`, "Altars" → `/modules/altar/board/index.html`, a gear button at the bottom → `open_settings`), marks the current page, exposes `window.pov = { invoke, listen }` wrapping `window.__TAURI__.core.invoke` and `window.__TAURI__.event.listen`. `shell.css`: sidebar 200 px wide, page content to its right, board color tokens.
  - Home (`home.html`/`home.js`): status card from `app_status` + `app:status` ("Path of Exile": not running / in background / in front, "Area": area id or "unknown", status line = `status.text`), module card "Altars" with an on/off switch bound to `modules.altar.enabled` through `settings_set` and `settings:changed`.
  - `app/icons`: generated once from `prototype/assets/icon_eater.png` with the repo's `.venv` Python + Pillow (`icon.ico` 16/32/48/256, `icon.png` 32×32 for the tray).

- [ ] **Step 1: Write the failing test** `app/src/scanner.rs` unit test `replay_mode_emits_status_and_hud_view`: drive one replay `Runner` (no Tauri) through a `fn tick_events(tick: &Tick, last: &mut Option<RunnerStatus>, altar_was_active: &mut bool) -> Vec<(&'static str, Value)>` helper on `screen_02.png` (skip if absent) → first call yields `("app:status", ..)` and `("altar:hud", ..)` with 2 altars; an idle tick after an active one yields `("altar:hud", {"altars": []})` once; an unchanged status yields no `app:status`.
- [ ] **Step 2: Run `cargo test -p pov-app`**, expected FAIL.
- [ ] **Step 3: Implement** the crate, config, icons and the Home/sidebar pages.
- [ ] **Step 4: Verify:** `cargo test --workspace --release` PASS; `cargo run -p pov-app --release -- --replay tests/fixtures/screens --hold 2` opens the main window with sidebar and Home showing "area unknown – scanning" or "in map – scanning"; closing it leaves the tray icon; a second `cargo run -p pov-app --release` focuses the first instead of starting twice; tray "Quit" ends the process (check `tasklist | findstr pov-app` is empty). Take a screenshot (PowerShell, `System.Drawing` `CopyFromScreen`) into the scratchpad and view it to confirm the layout.
- [ ] **Step 5: Commit** `git commit -m "App: Tauri shell with main window, tray and scanner thread"`

### Task 7: HUD window

**Files:**
- Create: `ui/modules/altar/hud/index.html`, `ui/modules/altar/hud/hud.css`, `ui/modules/altar/hud/hud.js`
- Move (`git mv`): `prototype/assets/target_*.png` and `prototype/assets/mods/` → `ui/modules/altar/img/` (keeping `mods/`); update `prototype/overlay.py` `ASSETS` and `ui/board/board.js`/`index.html` icon paths so nothing breaks before Task 8
- Modify: `app/src/main.rs`, `app/src/commands.rs`, `tests/test_data_files.py` (icon path)

**Interfaces:**
- Consumes: `altar:hud` events (Task 6 / `HudView` of Task 3), `pov_overlay` (Task 5), `RunnerStatus`/game context for the PoE client rect.
- Produces:
  - HUD window `hud` created at startup: url `/modules/altar/hud/index.html`, `transparent(true)`, `decorations(false)`, `shadow(false)`, `always_on_top(true)`, `skip_taskbar(true)`, `focused(false)`, `visible(false)`, `resizable(false)`; after creation `make_overlay(hwnd)` and `set_ignore_cursor_events(true)`.
  - Command `hud_fit(width: u32, height: u32)` (physical pixels): when `width * height > 0` and the PoE window is known, `show_at(hwnd, hud_rect(client, (w, h), hud_offset_x, hud_bottom))`; size 0 → `hide`. The scanner thread also hides the HUD whenever it emits an empty view. Never use Tauri's `show()` for this window (it activates).
  - `hud.js`: on `altar:hud`, skip if the JSON equals the last one; otherwise rebuild the DOM: one column per altar side by side (gap 16 px, aligned to the bottom), each column = top option box, recommendation box, bottom option box; all text via `textContent`; then measure `document.body` and call `hud_fit(ceil(width * devicePixelRatio), ceil(height * devicePixelRatio))`; empty view → clear and `hud_fit(0, 0)`.
  - Look copied from `prototype/overlay.py`: font "Segoe UI", text `#d8d8d8`, size = `style.font_size` px; box background `rgba(12,14,20,0.894)` (opaque `rgb(12,14,20)` when chosen), border 2 px (3 px when chosen) in the verdict color, radius 6 px, padding 8 px 12 px 8 px 10 px; chosen box glow `box-shadow: 0 0 24px <color>` inside a 12 px margin; verdict legend on the top-left border (font size − 3, background `rgb(12,14,20)`, padding 0 5 px, radius 3 px, 12 px from the left); target icon height `font_size × 3.6`, column width `× 2.1`; mod rows as a grid `sign | icon | text | values`, row min height and icon size `font_size × 1.6`; sign `+` `#4fbf6a` / `−` `#e05a4f`, bold; tier tint background `<tint>` at alpha 60/255 with radius 3 px only if `style.tiers`; values column only if `style.values`, read values `#d8d8d8`, unread `#6f757d`, joined by " / "; dimmed options at `opacity: style.other_opacity`; recommendation box border `#3a3f4b`, text bold in `recommendation.color`, arrows on both sides in "Segoe UI Symbol" with two non-breaking spaces between; incomplete altar adds a second line "partly hidden, verdict may change" in `#9aa0a6`; missing icon images are hidden (`onerror`).

- [ ] **Step 1: Verify the moved assets:** `.venv` `python -m pytest tests -q` PASS (the data-file test checks the new icon folder); `python -m prototype.app --replay tests/fixtures/screens` still shows the Qt HUD with icons (start, look, quit via tray).
- [ ] **Step 2: Implement** the HUD window, `hud_fit` and the page.
- [ ] **Step 3: Verify:** `cargo test --workspace --release` PASS; `cargo run -p pov-app --release -- --replay tests/fixtures/screens --hold 3`: the HUD appears bottom-center of the primary monitor over the replayed screenshots and changes with them; screenshot it next to the prototype HUD on the same screenshot (`python -m prototype.app --replay` with only `screen_02.png` in a temp folder) and compare: same boxes, colors, icons, texts, values, dimming; clicking through the HUD area reaches the window below, and the HUD never shows in the taskbar or takes focus (type into Notepad while it updates).
- [ ] **Step 4: Commit** `git commit -m "App: altar HUD window"`

### Task 8: Board in the app

**Files:**
- Move (`git mv`): `ui/board/` → `ui/modules/altar/board/`
- Modify: `ui/modules/altar/board/board.js`, `ui/modules/altar/board/index.html`, `ui/modules/altar/board/board.css`, `app/src/commands.rs`, `app/src/main.rs`, `prototype/board.py` (`BOARD_HTML` path; the Python board no longer has a backend and says so), `tools/gen_board_art.py` (output path)

**Interfaces:**
- Consumes: `BoardApi` (Task 4), `settings_get`, `settings:changed` (Task 6), `window.pov` (Task 6).
- Produces:
  - Commands wrapping `BoardApi` 1:1: `altar_get_state`, `altar_load_profile(file)`, `altar_save_profile(file, profile)`, `altar_duplicate_profile(file, name)`, `altar_delete_profile(file)`, `altar_set_active(file)`; errors as `Err(String)`.
  - `connectBackend()` returns the same object as today, backed by `window.pov.invoke` (`getState`, `loadProfile`, `saveProfile`, `duplicateProfile`, `deleteProfile`, `setActive`); `getSettings`/`saveSettings` are removed.
  - The "Settings" button, the `#hud` panel and their JS (`renderSettings`, `changeSetting`, `showSettingsPanel`, the `s-*` handlers) are removed. `board_cards`/`board_target_tag` come from `settings_get` at start and from `settings:changed` (sets `document.body.dataset.cards` / `dataset.targetTag`).
  - `ICON_DIR` = `"../img/mods/"`, filter-bar icons in `index.html` point there; the board includes `../../../shell/shell.css` and `shell.js` so the sidebar shows; everything else unchanged.

- [ ] **Step 1: Implement** the commands and the board changes.
- [ ] **Step 2: Verify:** `cargo test --workspace --release` PASS; in `cargo run -p pov-app --release`: sidebar "Altars" opens the board with all 201 cards and reward icons; drag a card between columns in a duplicated profile → the file in `%APPDATA%\miicrop-pov\profiles\altar\` changes, Ctrl+Z restores it; the read-only notice shows on "Default"; Duplicate/Delete work; the filter bar (Altar, Side, Target, Reward, Search) works and stays sticky; switching the profile in the board changes the HUD verdicts in replay mode within a second; screenshot before/after to confirm the board looks as before apart from the sidebar and the missing "Settings" button. Restore the user's active profile and any test profiles afterwards.
- [ ] **Step 3: Commit** `git commit -m "App: tier board over Tauri IPC"`

### Task 9: Settings window

**Files:**
- Create: `ui/shell/settings.html`, `ui/shell/settings.js`
- Modify: `app/src/commands.rs` (`open_settings` creates or focuses the window), `app/src/main.rs` (tray "Settings")

**Interfaces:**
- Consumes: `settings_schema`, `settings_get`, `settings_set`, `settings_reset`, `settings:changed` (Task 6).
- Produces:
  - Window `settings`: url `/shell/settings.html`, title "Path of Vision – Settings", 900×640, centered, created on demand, focused if it exists; `open_settings(Some("altar"))` selects that section.
  - `settings.html`: left list of sections ("General", "Altars"), right side the selected section's groups as forms built only from the schema: Toggle → On/Off segmented buttons, Choice → segmented buttons, Slider → range input with live value text (`value × scale` + unit), Number → number input + unit, Path → text input with placeholder (empty saves `null`). Help text under the control. Groups with `reset: true` get a "Reset to defaults" button (`settings_reset` with that group's keys). Sliders save 150 ms after the last input; a "Saved" note shows for 1.5 s. External changes (`settings:changed`) re-render the values. Same look as the old board panel (segmented buttons and labels from `board.css`).

- [ ] **Step 1: Implement** the window and the page.
- [ ] **Step 2: Verify:** `cargo test --workspace --release` PASS; in replay mode: change "Text size", "Tiers", "Values", "Alpha", "Position X/Y" → the HUD follows within a second; "Text"/"Target tag" → an open board updates without reload; "Reset to defaults" under Overlay restores the HUD defaults; `settings.json` keeps keys it does not know (add `"zzz": 1` by hand first); "Start with Windows" On creates `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` entry for the app, Off removes it (leave it Off); the gear in the sidebar and tray "Settings" both open the same single window. Screenshot General and Altars.
- [ ] **Step 3: Commit** `git commit -m "App: settings window built from the settings schema"`

### Task 10: Parity check, numbers and plan notes

**Files:**
- Modify: `plans/Plan.md` (Phase 4 entry "Plan B2 done"), `docs/superpowers/plans/2026-11-03-plan-b2-tauri-app.md` (status)

- [ ] **Step 1: Measure in replay** (`cargo run -p pov-app --release -- --replay tests/fixtures/screens --hold 1 --stats`, 60 s): stats lines with CPU % of total; with the main window closed (tray only) note the app's total RAM from `Get-Process pov-app, msedgewebview2 | Where-Object { ... }` (the WebView2 processes started by the app: match by parent process id) for "main window closed, HUD only".
- [ ] **Step 2: Update `plans/Plan.md`** Phase 4 with "Plan B2 done": what the app does, how to start it (`cargo run -p pov-app --release`), the replay numbers (CPU, RAM main window closed) and the Python board no longer working (the Tauri board replaces it; the Python scanner + HUD still run). Commit `git commit -m "Plans: Plan B2 done"`.
- [ ] **Step 3 (user, after the branch review):** the controller asks the user to start `target\release\pov-app.exe` and map for a while: HUD look and verdicts match the prototype, no focus loss while typing in chat, clicks go through the HUD, FPS unchanged, CPU ≤ 3 % (Home or `--stats`), RAM with the main window closed, board scrolling smooth at 144 Hz, tray/close/second start behave as described. Results go into `plans/Plan.md`; if the WebView HUD is clearly heavier than expected, the Direct2D HUD (spec 4) becomes a Plan C item.
