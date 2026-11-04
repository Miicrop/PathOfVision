# Path of Vision – project guide

The starting point for anyone (person or coding agent) who works on this repository. It describes the project as it is now. Open work and ideas are in [backlog.md](backlog.md); the history of how it got here is in git (`git log`; the old `plans/Plan.md` and the design specs and plans under `docs/superpowers/` were removed on 2026-11-04 and are in the history before that date).

## What it is

A Windows desktop app for Path of Exile 1 that reads Eldritch Altar options from the screen, rates them against the player's profile and shows a verdict in a small HUD next to the game. Players decide without pausing. Released on GitHub (`Miicrop/PathOfVision`), installed by friends of the author, updates itself.

The altar scanner is the first **module** of a generic "PoE helper": find a trigger on screen, read text, match it against game data, rate it against a profile, show a verdict. Further module ideas are in the backlog.

## Ground rules (not negotiable)

- Passive screen reading only (screen capture + OCR). **Never** send input to the game (no clicks, keys, mouse moves), no memory reading, no DLL injection, no hooks into the game's rendering.
- The HUD is a separate topmost, click-through window that never takes focus. The game runs windowed or borderless.
- The only network traffic is the update check/download from GitHub Releases. No data leaves the PC.
- English only: game client, app UI, code, data, docs.
- Author is Miicrop. No AI tool is named as author or co-author anywhere (commits, docs, release text).

## Architecture

Rust workspace (`Cargo.toml`) plus a Tauri 2 app; all UI is HTML/JS.

| Path | Role |
|---|---|
| `crates/vision` | Image type (`Rgb`), template matching, resizing, `Scale` (client height / 1440) |
| `crates/ocr` | Windows.Media.Ocr wrapper |
| `crates/capture` | Windows.Graphics.Capture of the PoE window (device-lost recovery, reset back-off) |
| `crates/game` | Finds the PoE window, follows the current area via `Client.txt` |
| `crates/core` | Paths (`AppPaths`), settings + schema, profiles, module contract, runner (scan loop), status, `resolution::support` |
| `crates/overlay` | HUD window placement helpers |
| `modules/altar` | The altar module: `detect` (icon ring), `read` (OCR boxes), `matching` (lines → cards), `verdict`, `live` (tracker), `hud`, `board` (board backend), `evaluate` (+ `bin/evaluate`) |
| `app/` | `pov-app`, the Tauri app "Path of Vision": main window (Home, Altars board), tray, settings window, HUD window, `update.rs` (updater) |
| `apps/scan` | `pov-scan`, headless console scanner (debugging) |
| `ui/shell/` | Main window, Home (status, version/update card, module switches), settings window |
| `ui/modules/altar/` | `board/` (tier board), `hud/`, `img/` (icons, card art) |
| `data/` | `mods.json` (generated), `labels.json`, `keywords.json`, `profiles/default.json`, `profiles/empty.json` |
| `tools/` | Python dev tools: `gen_mods` (builds `mods.json`), `gen_board_art`, `crop_client.py` (cuts the game client out of desktop screenshots) |
| `tests/fixtures/` | Label files (in git) and screenshots (`screens*/`, local only, git-ignored) |

Scan pipeline: capture (4 Hz, only while PoE is in front and in a map or unknown area) → altar detection on a downscaled frame → on a hit: locate the boxes, OCR, numbers → `#`, match against `mods.json` → verdict per option → HUD. Altars are tracked across scans so they are read once.

User data lives in `%APPDATA%\miicrop-pov` (`settings.json`, `profiles/`). Internal identity `io.github.miicrop.pov`. Installed app: per-user NSIS install in `%LOCALAPPDATA%\Path of Vision`, data shipped next to the exe (`data/`, `modules/altar/assets/`).

## Resolutions

All pixel constants are reference values at **1440 px** client height; modules convert with `vision::Scale` (see [modules.md](modules.md)). `pov_core::resolution::support(w, h, try_anyway)`:

- **Verified:** 1080–1440 px height (windowed down to 90 % of 1080), aspect 16:10 up to 2.6:1.
- **Untested** (runs, HUD shows a note): up to 2160 px (4K), narrower than 16:10, wider than 2.6:1.
- **Unsupported:** smaller; the setting "Try unsupported resolutions" opts in.

Golden results (real screenshots): 1920x1080 28 altars, 55/56 options (98.2 %); 1920x1200 18/18; 2560x1353 8/8; 3440x1440 84/85 (known miss `screen_10.png [856, 1152] bottom`); synthetic 1080 84/85; hard test 3435x1132 (pillarboxed ultrawide window) 7/8. Acceptance bar per resolution: all altars found, no false detections, ≥ 95 % of readable options fully correct.

## Domain model

- **Altar:** `eater` (Eater of Worlds, blue) or `exarch` (Searing Exarch, red), detected from the icon color. Generic mods appear on both; mods of the other altar type do appear, so matching does not filter by altar.
- **Side:** upside or downside. **Target:** `player`, `minions` (Eldritch Minions), `boss` (Map Boss), from the box header; if the header is hidden, the target that explains the most lines.
- **Option:** one of the two choices (top/bottom box): a target plus one or more upsides and downsides (up to 2 upsides and 3 downsides seen). Lines of different mods interleave, so a box is matched as a set of lines; the same mod can appear twice.
- **Card:** one entry in `mods.json` = one card on the board; it groups game mods (variants) with the same effect (same text on both altars, `HardMode*` variants with lower rolls). Card id = id of the first normal variant; profiles store card ids.

## Verdict model

Per option, using the best upside column and the worst downside:

| Downside (worst) | Upside (best) | Verdict |
|---|---|---|
| Brick | top column or a key column (★) | **ADAPT - BRICK** |
| Brick | other column or Unrated | **AVOID** |
| Dangerous | any rated column | **CONSIDER** |
| Dangerous | Unrated | **AVOID** |
| Fine | any rated column | **TAKE** |
| Fine | Unrated | **NEUTRAL** |

No mod of an option readable → `UNREADABLE`, the recommendation waits.

Recommendation:
1. One or two TAKE → the better one (column further right, then position in the column). Exception: a CONSIDER whose best upside sits in a column further right beats the TAKE.
2. Otherwise CONSIDER or ADAPT - BRICK → "your call" (no firm pick; HUD points at the better risky option; "Top/Bottom is safe." when the other is NEUTRAL).
3. Both AVOID → "Leave this altar alone."
4. Otherwise NEUTRAL → "Nothing rated. Take the … option.", preferring a minions option.

## Tier board (Altars page)

- Filters (sticky): altar, side, target, reward group, search (mod text and short label).
- Upsides: `Unrated | own columns … | top column` (further right = higher priority; order inside a column = priority). Own columns can be key columns (★). Downsides: `Fine | Dangerous | Brick`; unrated downsides count as Fine.
- Cards: altar as frame color (generic = split frame with gold seam), faded target scene, reward icon, short label from `labels.json` plus mod text (card style setting). Downside keywords colored from `keywords.json`.
- Drag & drop, Ctrl/Shift multi-select, ← / → move the selection, Ctrl+Z / Ctrl+Y.
- Profiles: read-only **Default**, **Duplicate** for an editable copy; every change is saved and applies to the HUD at once.
- **Export** copies the profile as a one-line text code (`POV1:` + base64url of deflated compact JSON: name, columns with key flags, top column, Dangerous, Brick; no settings, no `readOnly`/`modsVersion`) to the clipboard, or shows it in a dialog when the clipboard fails. The Default profile is about 1.1k characters. **Import** opens a dialog with a live preview; it always writes a new user profile (a taken name gets " (2)", " (3)" …), drops ids unknown to this `mods.json` and makes the profile active. Invalid codes, codes of a newer format version and codes inflating beyond 1 MB are refused (`BoardApi::export_code` / `import_preview` / `import_code`).

## Data files

- `data/mods.json`: generated by `python -m tools.gen_mods` from the RePoE export (domain `primordial_altar`), never edited by hand. Prints a diff (added/removed/changed cards). `templates` = text with values as `#` (the same normalization is applied to OCR text); `ranges` are base values (atlas passives raise in-game values above them).
- `data/labels.json`: hand-made per card: `alias` (HUD short label, `#`/`#2` = read values), `icon`. `gen_mods` adds empty entries for new cards and never touches aliases or icons.
- `data/keywords.json`: keyword colors for downsides.
- `data/profiles/<name>.json`: `upsides.columns[] {name, ids, key?}`, `upsides.top`, `downsides.dangerous`, `downsides.brick`. Missing mods count as Unrated/Fine. Saving keeps unknown keys and order.

## Commands

- Tests: `cargo test --workspace` (Rust; cargo is in `%USERPROFILE%\.cargo\bin`, on PATH in PowerShell). Python tools: `.venv\Scripts\python -m pytest tests`.
- Golden tests (need the local screenshots): `cargo test -p altar --release --test golden -- --ignored`.
- Evaluate a screenshot set: `cargo run -p altar --release --bin evaluate` (`--height <px>` synthetic downscale, `--write <dir>`, `--draft` label draft).
- Run the app from source: `cargo run -p pov-app --release` (`--replay <dir> --hold <s>` cycles screenshots instead of the game, `--stats`, `--hidden`). Headless: `cargo run -p pov-scan --release`.
- Installer locally: `cargo tauri build` in `app/` → `target/release/bundle/nsis/`.
- New screenshots: game windowed at the wanted size, altars with both colors and boxes near HUD and screen edges; full-desktop shots are cut with `tools/crop_client.py <folder> --expect WxH`.

## Release flow

1. Bump the version in `app/tauri.conf.json`, `app/Cargo.toml`, `Cargo.lock` (`pov-app`) and add a `CHANGELOG.md` section (its text becomes the release notes). `tauri.conf.json` must be saved without BOM.
2. Commit on `main`, push, push tag `vX.Y.Z`. CI (`.github/workflows/release.yml`) checks tag = version, builds and signs the installer and creates a **draft** release with installer, `.sig` and `latest.json`.
3. The author checks the draft and clicks Publish. GitHub's "latest" redirect takes ~2 minutes to switch; updaters see the release after that.

Updater: checks `releases/latest/download/latest.json` at start; installs only while PoE is closed, otherwise Home offers "Update now". Home's version card (next to Status) shows the running version (`app_version`) and "Up to date"; with a pending offer it turns into the update card (`v<old> → v<new>`, gold "Update now", spinner while installing, "Try again" with the error on failure). The signing key's private half is outside the repo (`%USERPROFILE%\.tauri`, backed up by the author; GitHub secrets `TAURI_SIGNING_PRIVATE_KEY(_PASSWORD)`). CI (`ci.yml`) runs `cargo test --workspace` on every push.

## Working rules

- Tests and tools never touch the real `%APPDATA%\miicrop-pov` (`settings.json`, profiles): use temp dirs and copies.
- Screenshots stay local and git-ignored; label files go into git. Labels describe what is in the image, never what makes a test pass.
- Branch per larger piece of work, merge to `main` fast-forward.
