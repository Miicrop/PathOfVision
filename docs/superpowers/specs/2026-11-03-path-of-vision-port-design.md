# Path of Vision – Phase 4 port design

Date: 2026-11-03. Status: reviewed, waiting for user approval.
Background and history: [plans/Plan.md](../../../plans/Plan.md) (sections 2–8, Phase 2–4, Phase 6). This document is what the port plan is built from.

## 1. Intent

The Python prototype proved that a passive screen-reading overlay can read Eldritch Altars and recommend an option with ~1–2 % CPU and no FPS loss. Phase 4 turns it into a **product other players can download and trust**: friends first, then a public post (e.g. Reddit).

The app is no longer "an altar scanner" but a **multi-module PoE 1 helper**, working name **Path of Vision** (final name later). The altar is its first and, in this phase, only module. The user has more ideas (Plan.md Phase 6: atlas/passive tree overlays, travel wheel, stash prices, Faustus helper, Ritual and Betrayal helpers, crafting help). They are not built now; they only shape the architecture so that adding a module later is easy.

The user does not read or write the code. Implementation choices are left to the builder; language and structure are chosen for the end result only.

## 2. Goals, non-goals, anti-goals

**Goals**
1. Everything the prototype does works in the new app (feature parity, section 7).
2. Works for other players: common resolutions and aspect ratios (section 8).
3. Easy to get: one small installer from GitHub Releases, auto-updates, no runtime to install, no admin rights.
4. Minimal impact on the game: idle cost near zero, scanning only when a module's context applies.
5. Adding a module later means adding one self-contained folder and one registration line, without changing the core (unless a new shared service is needed, which then goes into the core for everyone).
6. A trustworthy product: free, open source, no account, no telemetry, a clear README.

**Non-goals (this phase)**
- Any module other than the altar.
- Shared services only needed by future modules: prices (poe.ninja, trade API), global hotkeys, mouse gestures, PoB/poeplanner importers, input sent to the game.
- A plugin system for third-party modules. Modules are compiled in; it stays the user's own code.
- Linux/macOS, game languages other than English, a paid code-signing certificate.
- Items from the Plan.md Phase 5 backlog, except resolution support (pulled into this phase).

**Anti-goals (would be a failure even if it technically works)**
- Wrong recommendations on unsupported setups. When the app cannot read reliably, it shows "incomplete" or nothing, never a confident wrong verdict.
- Anything that looks shady: hidden network traffic, data leaving the PC, installers with extras, unclear terms.
- Noticeable load while playing, or windows that steal focus in-game.
- A core that has to be reworked for the second module.

## 3. Constraints

- **ToS ground rules (Plan.md section 2) stay:** passive screen reading only; no input to the game, no memory reading, no injection or render hooks; overlays are separate click-through topmost windows; PoE runs windowed or borderless fullscreen. Chat commands and pasting (e.g. `/hideout`, stash regex) are a known future wish; the user expects them to be tolerated as in other popular tools. Not built now, and the rule is not relaxed in this phase.
- **Windows 10/11 only** (WebView2, Windows.Graphics.Capture, Windows.Media.Ocr).
- **English game client.**
- **Privacy:** screenshots never leave the PC. The only network traffic in this phase is the update check/download from GitHub.
- **Open source** on GitHub. License: MIT *(builder's call)*.
- **Tests and dev tools never touch the user's real `settings.json` or profiles** (they edit them while work happens): use temporary copies.

## 4. Tech stack (decided)

- **Rust + Tauri 2.** Rationale in Plan.md Phase 4: smallest download, no runtime, low idle RAM, built-in installer and signed updater, strict compiler for a growing AI-written code base.
- Native Windows APIs through Microsoft's `windows` crate: **Windows.Graphics.Capture** (replaces `mss`) and **Windows.Media.Ocr** (as in the prototype).
- **No OpenCV.** Color masks, downscaling and template matching are written in Rust (small, well-tested functions; SIMD or `rayon` only if measurements ask for it).
- **All UI is web UI** in WebView2: main window, settings window, module pages, and the HUD (transparent, always-on-top, click-through, never focused). If the HUD window costs noticeable RAM or FPS (measure!), it moves to a native Direct2D window behind the same view-model interface, without touching the rest.

## 5. User experience

- The user starts the exe; the **main window opens**.
- Closing the main window keeps the app running in the **tray**. Tray menu: open window, settings, quit. Only "quit" stops the app.
- **Single instance:** starting the exe again brings the existing window to the front.
- **Autostart with Windows:** optional, off by default (general setting).
- **Main window:** sidebar on the left (Home on top, then one entry per module); the right side shows the selected page.
  - Home: status (PoE detected or not, active modules, update available), module on/off switches.
  - Altar page: the existing tier board (`ui/board/`), unchanged in look and behavior.
- **Settings:** a **separate window** (gear button in the main window, or the tray menu). Left list: "General", then one entry per module. No settings bar inside the main window.
- **In-game:** only overlays are visible (altar HUD). No popups, no focus stealing.
- **Updates:** checked in the background at start; installed on the next start or when the user clicks "update now", never mid-game.

## 6. Architecture

### 6.1 Layers

```
app (Tauri shell: windows, tray, IPC, updater)
 ├─ core            module contract, registry, settings & profiles store,
 │                  game context, scheduler, event bus
 ├─ services        capture, vision, ocr, overlay (HUD windows)
 └─ modules/altar   detect → read → match → track → verdict → HUD view model
ui/
 ├─ shell/          main window (sidebar, Home), settings window
 ├─ hud/            HUD window(s)
 └─ modules/altar/  board (from ui/board/), altar HUD view
```

Suggested Cargo workspace (names are the builder's call): `crates/core`, `crates/capture`, `crates/vision`, `crates/ocr`, `modules/altar`, `app` (Tauri). Pure logic (vision math, matching, verdict) has no Windows or Tauri dependency, so it is unit-testable on its own.

### 6.2 Module contract

Each module is a self-contained folder (Rust crate + web page + its data) that registers with the core. Through a `Module` trait (exact shape is the builder's call) it declares:

- **Identity:** id, display name, sidebar icon, web page entry.
- **Settings:** a schema with defaults. The settings window builds the module's form from it (no per-module settings UI code needed for plain options). A module may also provide a custom settings view.
- **Activation:** when it should run, as a condition on the game context (e.g. altar: "PoE running and the player is in a map"). Inactive and disabled modules cost nothing.
- **Work:** start/stop, and requests to shared services (capture region + rate, OCR, HUD output).
- **Commands:** its own IPC namespace for its page (the board's seven calls become altar commands).
- **Data:** its shipped data files and its user files (profiles).

Adding a module = new folder + one registration line. A module needing a service that does not exist adds that service to the core so later modules can reuse it (e.g. prices, hotkeys, input).

### 6.3 Shared services in this phase

Only what the altar needs, shaped so later ones fit beside them:

- **Game context:** finds the PoE window (process/window class), its client rect, monitor, DPI and focus; watches `Client.txt` for area changes to know map / hideout / town. If `Client.txt` cannot be found, fall back to "active whenever PoE is running" and let the user set the path in General settings. *(Map detection from `Client.txt` area lines is builder's call; verify the exact log lines.)*
- **Capture:** one Windows.Graphics.Capture session for the PoE window or its monitor (builder's call; watch for the yellow capture border on Windows 10), shared by all modules. Modules request regions and rates; frames are cropped/downscaled without full-frame copies where possible. No capture while no module is active.
- **Vision:** image type, color masks, scaling, template matching with scores, helpers to place regions relative to a hit.
- **OCR:** Windows.Media.Ocr wrapper with the prototype's preprocessing (bright pixels black on white, 2× upscale).
- **Overlay:** creates and positions HUD windows over the PoE window (anchored, scaled with resolution), shows/hides them with PoE focus and module activity.
- **Settings & profiles store:** general + per-module settings, profiles, validation (port of `settings.clean()`), change events so edits apply instantly (as today).

### 6.4 Settings and file locations

- Shipped data (`mods.json`, `labels.json`, `keywords.json`, default profile, icons, board art) is read-only inside the app.
- User data in `%APPDATA%\PathOfVision\`: `settings.json` (sections `general` and `modules.<id>`; the prototype keys move under `modules.altar`), `profiles/altar/*.json`. The read-only "Default" profile and the "Default (copy)" behavior stay as in the prototype.
- First start: create user data from shipped defaults. The user's existing prototype profiles are copied over once by hand (or a small dev-only import); no general import feature in this phase.
- Dev mode: data files are read from the repo and reloaded on change (today's live editing of `labels.json`/`keywords.json` keeps working while developing).

## 7. Altar module: feature parity

Everything in Plan.md Phases 2–3 and the board round, ported behavior-for-behavior. Notably:

- Stage 1 detection (ring template, coarse-to-fine, altar type from icon color), ~4 Hz while active.
- Stage 2 reading: OCR regions relative to the icon, line joining, box assignment, header handling, dropping lines before the header.
- Matching: numbers → `#`, fuzzy per line, exact-cover per box, duplicate mods, other-altar mods, target inference when the header is hidden.
- Tracking: by position and by content, better read wins, complete altars not re-read, incomplete ones re-read every 0.5 s.
- Verdict engine with all rules (incl. ADAPT - BRICK, CONSIDER beats TAKE when its upside is further right, key columns, `UNREADABLE`). Port the Python unit tests 1:1.
- HUD: layout mirroring the game (top box, recommendation, bottom box), verdict colors, target icon halves, up to two altars, aliases and icons from `labels.json`, keyword colors, all HUD settings (font size, tiers style, values, other-option opacity, position). Mod text always from `mods.json`, never raw OCR.
- Board: all current features unchanged; only the backend adapter `connectBackend()` in `board.js` is replaced by Tauri IPC. **The board's "Settings" panel moves out (user decision):** everything under today's "Settings" button (HUD options: font size, tier style, values column, other-option opacity, HUD position; board options: card style, target tag) goes to the settings window, entry "Altars", and the button disappears from the board. The filter bar stays in the board as it is: Altar, Side, Target, Reward, Search. The board applies card style and target tag changes from the settings window live (settings change event), so this is the one board change beyond the adapter. Reward icons move from `prototype/assets/mods/` next to the module's UI. Smooth scrolling must hold at high refresh rates (WebView2 has no 60 fps cap like Qt WebEngine, but verify).
- Dev tools: replay mode (run on a folder of screenshots instead of the live screen) and a stats mode (scan timings, CPU) stay available in dev builds.

Python dev tools (`tools/gen_mods`, `tools/gen_board_art`) may stay in Python; they are not shipped *(builder's call)*. The Python prototype stays in the repo as the reference until parity is shown, then is removed.

## 8. Resolution support

- Supported: 16:9 at 1920×1080, 2560×1440, 3840×2160; 21:9 at 2560×1080 and 3440×1440; 32:9 if fixtures exist. Windowed (any size within these) and borderless fullscreen. Multi-monitor: wherever the PoE window is.
- Working hypothesis *(to verify first, before building on it)*: PoE 1's UI scales with the client height, so the scale factor is `client_height / 1440` and detection runs at a known scale instead of searching many. If this is wrong, fall back to a small multi-scale search.
- OCR regions, box sizes and HUD size all follow the same scale factor.
- **Fixtures:** the user captures altar screenshots at the resolutions that fit the user's own 3440×1440 monitor (windowed mode); friends with other monitors provide 4K, 32:9 and other native setups (user decision). The builder provides an exact shot list written so friends can follow it without help. Screenshots stay local and git-ignored; labels go in git as today.
- **Acceptance per resolution:** all altars found, no false detections, ≥ 95 % of readable options fully correct *(builder's threshold; 3440×1440 must stay at the prototype's 98.8 %)*.
- Unknown or failed reads never produce a confident verdict (anti-goal 1).
- **Other sizes** (16:10, 1600×900, odd window sizes) *(builder's call)*: if the scale hypothesis holds, the module runs at any client height inside the verified range (lowest to highest height with passing fixtures). Outside that range the game context reports "unsupported size"; the altar module does not show verdicts and Home explains why. A general setting "try anyway" lets players opt in, with a small "untested resolution" note in the HUD.

## 9. Performance budget

Measured with the same method as Phase 3 (`--stats`, CPU total, in-game FPS with/without tool):

- PoE not running: ~0 % CPU, no capture.
- PoE running, no active module (hideout/town): ~0 % CPU, no capture.
- Mapping, altar module active: ≤ prototype (≤ 3 % total CPU), no measurable FPS loss.
- RAM with the main window closed (HUD only): measure and report; if the WebView HUD is clearly heavier than a native window would be, switch the HUD to Direct2D (section 4).

## 10. Distribution and release

- Tauri bundler: per-user NSIS installer (no admin), Tauri updater with a signing key, update feed from GitHub Releases, changelog per release.
- **Name-independent identity** *(builder's call)*: the final name may come after friends have installed the app. Bundle identifier, install folder, user data folder and updater key use a permanent neutral internal ID chosen once at the start; only the display name, window titles and README change on a rename. (Section 6.4's `%APPDATA%\PathOfVision\` stands for that ID's folder.)
- README: what it does, what it never does (no memory reading, no input to the game, no data leaves the PC), supported setups and resolutions, install, the SmartScreen warning on first start and why, privacy, license, how to report a bad read.
- **Gates before a public release** (not before friends): GGG terms of use re-checked (Plan.md section 2) and the use of extracted game art (icons, altar art) cleared or replaced.
- **Repository:** the project is local only for now. The user creates the GitHub repo once the app is testable (or earlier if the builder needs it, on request). Since releases and the updater feed live there, the game-art question has to be settled before that repo is made public *(builder's note)*.
- CI on GitHub Actions (Windows runner): build, unit tests, release artifacts. Fixture-based tests run locally only, since screenshots are not in git.

## 11. Testing

- Unit tests for pure logic (vision math, matching, verdict, settings validation); the Python verdict and matching tests are ported as the starting set.
- Golden tests: the Rust pipeline on `tests/fixtures/screens` must reproduce the labeled results (≥ the Python results); skipped when screenshots are absent.
- Per-resolution fixture sets (section 8).
- Tests use temporary settings and profile copies, never the user's files.

## 12. Decided by the user

- Rust + Tauri 2; the prototype code can be changed or replaced freely.
- Working name Path of Vision.
- Main window opens on start, closes to tray, reopen from tray.
- Main window with a left sidebar of modules; settings in a separate window with "General" plus one entry per module.
- Compiled-in modules, no plugin system.
- This phase builds only the core and the altar module; the other ideas only inform the structure.
- First version is for other players (friends, likely Reddit): multiple resolutions required; the user takes the screenshots.
- Free, no account, no telemetry, screenshots stay local, open source on GitHub, a good README.
- GitHub repo is created by the user when the app becomes testable.
- Everything under the board's current "Settings" button moves to the settings window under "Altars"; the board keeps its filter bar (Altar, Side, Target, Reward, Search).
- Friends provide screenshots for monitors the user does not have.

## 13. Left to the builder

Crate names and boundaries, the exact `Module` trait, event bus and threading model, capture of window vs. monitor, template matching algorithm details, frontend tooling for the new shell and settings pages (the board is plain HTML/CSS/JS without a build step; staying build-free is preferred unless it hurts), settings schema format, IPC naming, installer details.
