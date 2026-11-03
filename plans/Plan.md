# PoE Altar Scanner – Project Plan

## 1. Goal

A Windows tool for Path of Exile 1 that detects Eldritch Altars on screen, reads both options, rates them against a user profile and shows the result in a fixed HUD panel. Players decide faster and safer without pausing the game. Long-term goal: a distributable `.exe` that other players can use.

Settings mockup: https://claude.ai/artifact/YJLU2dYUSPTziV5WrNxDKv

### Long-term vision: a generic PoE helper

The altar scanner is meant as the first module of a bigger "PoE helper". Other mechanics follow the same pattern: find a trigger on screen, read text, match it against game data, rate it against a user profile (e.g. "is this a brick?") and show a verdict. So the code is kept generic where that costs little:

- **Shared core:** screen capture, trigger detection by template, OCR with preprocessing, fuzzy matching against a generated data file, tier profiles, verdict engine, HUD, tier board.
- **Per mechanic (a module):** trigger template(s), where the text sits relative to the trigger, the data generator for its mods, the verdict rules, HUD layout tweaks.
- The Python prototype stays a single altar module; it only proves the approach and shows what is needed. The core/mechanic split is designed into the final version (Phase 4 port).

## 2. Ground rules (ToS)

- Passive screen reading only (screen capture + OCR).
- **Never** send input to the game: no clicks, no mouse movement, no key presses.
- No memory reading, no DLL injection, no hooks into the game's rendering.
- The overlay is a separate topmost window that lets clicks pass through.
- The game must run in windowed or borderless fullscreen mode.
- Before a public release, check GGG's current ToS and their forum statements on third-party tools again.

## 3. Decisions so far

| Topic | Decision |
|---|---|
| Language | English only: game client, app UI, code, data |
| Trigger | Continuous scan, no hotkey required |
| Scan pipeline | Two stages: cheap detection (3–5 Hz) → OCR + matching only on a hit, cached per altar |
| Display | Fixed HUD panel, no frames around the in-game boxes (characters move too fast) |
| Altar type | Detected automatically from the icon color, no user setting. Only informational: mods of the other altar type do appear (see Phase 2) |
| Mod data | Auto-generated `mods.json` with stable IDs; weight 0 mods excluded |
| Settings UI | Tier board modeled on poedb's altar table, profiles, instant apply |
| Tech path | Python prototype first, then port to a distributable app: **Rust + Tauri 2** (decided 2026-11-03, see Phase 4) |
| Shared logic | All knowledge lives in data files (mods, profiles), so it survives the port |

## 4. Domain model

- **Altar:** `eater` (Eater of Worlds, blue) or `exarch` (Searing Exarch, red). Mods without an altar icon on poedb are **generic** and appear on both.
- **Side:** `upside` or `downside`.
- **Target:** `player`, `minions` (Eldritch Minions), `boss` (Map Boss). The in-box header ("Player gains:", …) gives it directly.
- **Option:** one of the two choices on an altar: a target plus one or more upsides and downsides.
- **Weight:** spawn weight, per altar. Weight 0 = cannot spawn, so it is not included in the data at all.

## 5. Settings UI (tier board)

Filter bar, default selection always leftmost:

```
Altar:  [ Both ] [ Eater of Worlds ] [ Searing Exarch ]
Side:   [ All ]  [ Upsides ] [ Downsides ]
Target: [ All ]  [ Player ] [ Eldritch Minions ] [ Map Boss ]
Reward: [ All ]  [ Currency ] [ Eldritch ] [ Scarabs ] [ Div Cards ] [ Duplicates ] [ Stats ]
Search: _______
```

- The filter bar stays at the top while scrolling (sticky). Search matches the mod text and the short label from `labels.json` (community names like "Turbo"). Filters combine (AND). Choosing an altar also shows generic mods. The reward group comes from the card's icon name in `labels.json` (`*-scarab`, `div-card-*`, `dup-*`, `stat-*`, `*eldritch-*`, everything else = Currency; each button shows a representative icon); downsides have no reward, so a reward filter hides the downside board.
- **Upside board:** `Unrated | own columns … | top column`. Unrated (fixed, left) means NEUTRAL. The user adds, names, moves and deletes the columns in between; all of them count as rated, further right = higher priority. The top column (fixed, right, renamable; "S" by default) can make a Brick workable; an own column marked as **key column** (★ in its header, `"key": true` in the profile) gets the same treatment (ADAPT - BRICK, gold in the HUD) but keeps its rank by position. Order inside a column = priority (top = best). Old profiles (`s`/`a`/`b`) are upgraded to columns B, A and top S.
- **Downside board:** `Fine | Dangerous | Brick`. Unrated downsides count as Fine.
- New mods after a data update land in Unrated/Fine and are flagged as new.
- Cards show the altar as the frame color (Eater teal, Exarch orange; generic mods get a frame split along a slanted gold seam), the target as its altar scene faded in behind the text (split along the same seam for generic mods), the reward icon from `labels.json` on the right, a target chip (can be switched off) and the text. Card styles ("Settings" panel, saved as `board_cards` in `settings.json`): **Short + text** (default; the HUD short label from `labels.json` as headline, the mod text with value range small below, always both for a consistent look), **Full text** (mod text only) and **Compact** (short label only, full text on hover). Downside texts color the keywords from `keywords.json` (board and HUD); upsides stay plain so "Chaos Orb" is not colored as Chaos. Spawn weight and the "rare" marker are not shown (not relevant for sorting). Short label mockup: https://claude.ai/artifact/Y7EtSi6yKoYHVq7rwh6yQC Tile mockups (variant B4 chosen): https://claude.ai/artifact/WjrZ5yiFshReKwPzZxKsTd
- Drag & drop between and within columns. Click selects a card, Ctrl+click adds or removes one, Shift+click selects a range within a column; dragging a selected card moves the whole selection in order. ← / → move the selection one column (the keyboard fallback that replaced the arrow buttons), Esc clears it. Ctrl+Z undoes moves and column edits, Ctrl+Y / Ctrl+Shift+Z redoes them (up to 100 steps, history cleared on profile switch; the undo is saved like any other change).
- Profiles: read-only **Default** profile, **Duplicate** to create an editable copy, switch via dropdown. Every change is saved and applied to the overlay immediately.

## 6. Verdict model

Per option, using the best upside tier and the worst downside tier:

| Downside (worst) | Upside (best) | Verdict |
|---|---|---|
| Brick | top or key column | **ADAPT - BRICK** (a brick by default, workable with build adjustments) |
| Brick | any other column or Unrated | **AVOID** |
| Dangerous | any rated column | **CONSIDER** |
| Dangerous | Unrated | **AVOID** |
| Fine | any rated column | **TAKE** |
| Fine | Unrated | **NEUTRAL** |

Assumption: users sort strictly, so anything rated B or better is worth considering. Thresholds can be changed later (see Future).

Recommendation line under the two options:
1. One or two TAKE → pick the better one (higher tier, then higher position in the column). Exception (2026-11-03): if the other option is CONSIDER and its best upside sits in a column further right, it is the pick instead ("Take the … option."; its box still says CONSIDER). A Dangerous downside only warns, so a clearly better upside wins; same column or lower → the TAKE stays.
2. Otherwise CONSIDER or ADAPT - BRICK present → "your call", no firm pick; the HUD arrows point at the better risky option. Text: "Be careful." (CONSIDER) or "Brick downside, good upside." (ADAPT), plus "Top/Bottom is safe." when the other option is NEUTRAL.
3. Both AVOID → "Leave this altar alone." (red, warning signs, both options dimmed).
4. Otherwise (NEUTRAL) → "Nothing rated. Take the … option.", preferring a minions option (safe default for new players; the text no longer mentions minions).

HUD panel (fixed position):

```
ALTAR · Eater of Worlds
▲ TOP      NEUTRAL
  Eldritch Minions gain
  + 2.2% chance for additional Orb of Alteration   Unrated
  − Hits have 61% chance to ignore Phys DR         Fine
▼ BOTTOM   CONSIDER
  Player gains
  + 21% increased Rarity                           B
  − Hits can Sap you                               Dangerous
Your call: check the Dangerous downside against your build.
```

## 7. Data files

`data/mods.json` (generated by `python -m tools.gen_mods`, never edited by hand). Source: the RePoE export (`repoe-fork.github.io/mods.min.json`, domain `primordial_altar`), which carries the game's own mod IDs and the in-game text.

```json
{
  "gameVersion": "3.29.3.3",
  "source": "https://repoe-fork.github.io/mods.min.json",
  "mods": [
    {
      "id": "TangledAltarUpsidePlayerIncreasedRarity",
      "altars": ["eater", "exarch"],
      "side": "upside",
      "target": "player",
      "lines": ["(15-30)% increased Rarity of Items found in this Area"],
      "weights": { "eater": 166100, "exarch": 75225 },
      "variants": [
        {
          "id": "TangledAltarUpsidePlayerIncreasedRarity",
          "altar": "eater",
          "hardMode": false,
          "lines": ["(15-30)% increased Rarity of Items found in this Area"],
          "templates": ["#% increased Rarity of Items found in this Area"],
          "ranges": [[15, 30]],
          "weight": 165600
        }
      ]
    }
  ]
}
```

- **Card vs. variant:** one entry in `mods` is one card on the board. A card groups game mods ("variants") that are the same effect: the same text on both altars (generic mods) and `HardMode*` variants of a mod (lower rolls, sometimes worded slightly differently).
- **HardMode variants** are not Ruthless-only: they show up in normal maps, on both altar types (e.g. "1.9% chance to drop an additional Lesser Eldritch Ember" on an Eater altar). When exactly they spawn is unknown.
- **Card id** = id of the first normal variant (sorted by HardMode, altar, id). Profiles store card ids. If a card id ever changes, a loader can still resolve an old id through `variants[].id`.
- **Altar / side / target** come from the spawn tags (`tangle_altar` = eater, `cleansing_fire_altar` = exarch; the blocked `primordial_altar_upside/downside` tag gives the side) and the `Upside|Downside` + `Boss|Monster|Player` part of the id. The rest of the game ids is inconsistent (`TangledAltar`, `TangleAltar`, `TangledAtlar`, `CleansingAltar`) and not used.
- **templates:** every value or range replaced by `#`, signs kept (`-(60-40)%` → `-#%`). The same function normalizes OCR text, so matching is a template comparison.
- **ranges:** one `[low, high]` per value in display order, sorted and signed (`(20-10)% reduced` → `[10, 20]`). These are base values: atlas passives raise the effect of altar mods, so in-game values often exceed them (e.g. "Final Boss drops 6 additional Orbs of Alteration" for a (2-4) range, 43 % Rarity for (15-30)).
- `weights` sums the variant weights per altar.

`data/profiles/<name>.json`:

```json
{
  "name": "Default",
  "readOnly": true,
  "modsVersion": "…",
  "upsides":   { "columns": [{ "name": "B", "ids": [] }, { "name": "A", "ids": [] }],
                "top": { "name": "S", "ids": ["id1", "id2"] } },
  "downsides": { "dangerous": ["id3"], "brick": ["id4"] }
}
```

Mods missing from a profile count as Unrated/Fine. Order inside each list is the priority.

## 8. Architecture

```
Screen capture ──► Stage 1: altar detection (3–5 Hz, downscaled frame)
                        │ hit
                        ▼
                   Stage 2: locate boxes → OCR → normalize numbers to "#"
                        │            (cache by crop hash, skip while unchanged)
                        ▼
                   Fuzzy match against mods.json (+ header → target)
                        ▼
                   Verdict engine (profile) ──► HUD overlay
                        ▲
Settings board (HTML/JS) ── writes profiles/*.json ── file watch → reload
```

- **Settings board as HTML/JS:** the mockup grew into the real board (`ui/board/`). The prototype hosts it via Qt WebEngine, and the final app hosts the same files via WebView2. That way the UI isn't rewritten during the port.
- **Verdict engine:** a small pure module with unit tests. It is the one piece of logic that gets ported 1:1.

## 9. Project layout (target)

```
plans/Plan.md
data/mods.json
data/profiles/default.json
tools/gen_mods/          # mod list generator + diff report
ui/board/                # tier board (HTML/JS/CSS), grown from the mockup
prototype/               # Python: capture, detect, ocr, match, verdict, overlay, app
tests/fixtures/screens/  # altar screenshots (local only, not in git)
tests/fixtures/labels.json  # verified labels for the screenshots
tests/                   # unit + recognition accuracy tests
```

## 10. Roadmap

### Phase 0 – Mockup v2 (done when published)
- Generic cards show both Eater and Exarch tags.
- Remove the weight 0 toggle. Weight 0 mods do not exist in the UI.
- New verdict rules incl. ADAPT - BRICK. Legend and HUD preview updated.

### Phase 1 – Mod data pipeline ✅ (done 2026-11-02)
- Source: RePoE export, domain `primordial_altar` (276 mods, 231 spawnable). poedb serves as a cross-check only (no IDs; wiki content is CC BY-NC-SA).
- Generator `tools/gen_mods` → `data/mods.json`: 201 cards from 231 spawnable mods (game 3.29.3.3). Standard library only. `--offline` reuses the cached download, `--dry-run` only prints the diff.
- Diff report on every run: added / removed / changed cards.
- `data/profiles/default.json`: first-guess starter tiers (S: Divine Orbs, duplicated Scarabs, Quantity; Brick: chaos damage during flask effect only). To be reviewed by the user.
- Tests: `python -m pytest` (generator unit tests + data consistency: profile ids exist and sit on the right side).
- Result of the cross-check against poedb: every normal mod matches (side, target, text, altar, weight). Differences are presentation only: poedb shows generic wording and drops the minus sign, and it lists HardMode variants as generic (no altar icon), which matches what the screenshots show.
- The four values in the first test screenshot (`Altar.png`, since removed; 2.2 % Alteration, 61 % ignore Phys DR, 34 % Sap, 21 % Rarity) all lie inside the generated ranges.

### Phase 2 – Recognition prototype (offline, Python) ✅ (done 2026-11-02)
- **Result on 39 screenshots (44 altars, 85 readable options):** 44/44 altars found, 0 false detections, **84/85 options fully correct (98.8 %)**, 175 ms median per screenshot (max 297 ms with two altars). Run `python -m prototype.evaluate`.
- Test set: `tests/fixtures/screens/screen_*.png` (3440×1440, local only, not in git) and `tests/fixtures/labels.json` (in git). Labels were drafted from the pipeline and then checked by eye against every screenshot. A box counts as unreadable (`"mods": null`, skipped) when a line is completely hidden; partly covered lines that a human can still identify stay in.
- Pipeline (`prototype/`):
  1. `detect.py`: template match on the altar ring (shared by both types), coarse at 1/4 scale, re-scored at 1/2 scale around each candidate. Altar type from the icon color.
  2. `read.py`: OCR 650 px left/right and 320 px above/below the icon. Preprocessing keeps only bright pixels (max channel > 110) as black on white and upscales 2×; this fixes the semi-transparent box background. Row fragments split by icons or life bars are joined; a line belongs to a box if it crosses the icon's center column and is vertically contiguous. Lines before the header (gold labels, timers) are dropped.
  3. `match.py`: numbers → `#`, fuzzy match per line, then an exact-cover search over the box's lines.
- Findings that shaped the design:
  - **Windows.Media.Ocr** (~15 ms per box, near perfect after preprocessing) beat RapidOCR (~1.4 s, split lines and glued words).
  - **Line order in-game differs from the data** and lines of different mods interleave ("Hits always Shock / Gain X% … Lightning / All Damage can Shock"). Matching treats a box as a set of lines.
  - **The same mod can appear twice** on one option (two "Rarity" lines).
  - **Mods of the other altar type appear** (Embers and Unique Jewellery cards on Eater altars, Ichors on Exarch altars). Matching does not filter by altar type; generic texts are one card anyway.
  - **Box text is centered on the icon.** The icon's two halves show both options' targets (crown = boss, figure = player, group = minions).
  - **Header hidden** (screen edge, buff bar): the target is inferred as the one that explains the most lines.
  - **HUD covers boxes**: buff bar, flask bar and skill bar draw on top of altar boxes. The one remaining miss is a Rarity line almost fully behind the flask bar.

### Phase 3 – Live prototype (Python) ✅ (done 2026-11-02)
- Run: `.venv\Scripts\python -m prototype.app` (tray icon: "Settings board", "Quit"). Without the game: `--replay tests/fixtures/screens`. Board alone: `python -m prototype.board`.
- `live.py`: capture with `mss` (primary monitor by default), icon search at 4 Hz, OCR only for new altars. Altars are tracked by icon position across scans and, because running moves the icon too far between scans, also by content (≥ 60 % of the same mods on the same positions; values ignored). The better read wins (complete, more mods). Once both boxes are read completely, an altar is not read again; incomplete altars (cut off, covered by HUD) are re-read every 0.5 s.
- `verdict.py`: verdict engine with unit tests for every row of the verdict table and the recommendation rules. New verdict `UNREADABLE` when no mod of an option could be read; the recommendation then waits ("Can't read both options yet.").
- `overlay.py`: frameless, always-on-top, click-through, never takes focus; anchored at the bottom center on every size change (`hud_bottom`, `hud_offset_x` in `settings.json`). Per altar it mirrors the game: top option box, recommendation box in the middle (where the altar icon sits), bottom option box. Box borders carry the verdict color; each option shows its target as the matching half of the altar icon (original game art, top half for the top option and bottom half for the bottom one; blue = Eater, red = Exarch). Up to two altars side by side. Mod text comes from `mods.json` with the read values filled in, so stray OCR text (gold labels, NPC lines) never shows up.
- `board.py` + `ui/board/`: the tier board grown from the mockup, on the real 201 cards. **Change of plan:** hosted in Qt WebEngine (already part of PySide6) instead of pywebview, so the board opens from the same process. The page talks to Python through a small adapter (`connectBackend()` in `board.js`, seven calls); the WebView2 port only replaces that adapter. A "Settings" panel in the board (renamed from "Overlay settings"; it also holds the board card style and target tag) sets text size, tier style, values column, dimming of the other option and the HUD position; values are validated in `settings.clean()` (shared with the overlay) and written to `settings.json` right away, so the HUD changes live. Upsides use custom columns (`Unrated | own named columns | top column`, see section 5): add with "+ Column" on the top column, rename / move / delete from the column header. Drag and drop shows a drop marker, auto-scrolls near column and window edges, drops on a column header go to the top, and each column keeps its scroll position after a move. The board uses the full window width (columns 240–340 px, left aligned) and opens at 90 % of the screen. Cards got the look from section 5 (2026-11-03): the scenes come from `python -m tools.gen_board_art`, which cuts the top altar halves (no bronze rim, wide soft fade) into `ui/board/img/`; `getState` passes each card's reward icon from `labels.json`. Multi-select (Ctrl/Shift) replaced the per-card arrow buttons. Smooth scrolling (2026-11-03): Qt WebEngine paces Chromium at a fixed 60 fps, which felt choppy on the 144 Hz monitor; `board.py` sets `--disable-frame-rate-limit` in `QTWEBENGINE_CHROMIUM_FLAGS` (frames during scrolling ~6 ms, nothing drawn while idle). The card art blur, saturation and oval fade are baked into the images by `gen_board_art` instead of CSS filter/mask on 201 cards, and the sticky bar shadow has no blur.
- Files: the board writes `data/profiles/*.json` and `settings.json` (active profile, HUD position); the scanner checks both on every scan, so changes apply instantly. User profiles and `settings.json` are not in git.
- Measured on the desktop without altars: **1.96 % total CPU** (47 % of one of 24 threads) at 4 Hz. Most of it is the `mss` screen grab (~35 ms); `dxcam` (DXGI) is the next lever if needed.
- **Done when:** a real mapping session shows correct verdicts with no noticeable FPS impact (CPU idle load from stage 1 ≤ 3 %). ✅
- In-game result (2026-11-02, `--stats`, 3440×1440, 4 Hz): CPU **1.1–2.2 % total** without an altar, 1.1–1.9 % with an altar on screen; scans take ~85 ms (one 305 ms spike when a new altar is read). FPS without / with tool: hideout 126 / 126, map 166–175 / 166–172 with the HUD shown. One run without an altar in view dipped to ~160 (low 139) right after entering the map; since the heavier run with HUD matched the baseline, this is taken as scene noise. Verdicts were correct in all of the user's sessions.
- Possible lever if ever needed: lower the scan rate (e.g. 2 Hz) while no altar is on screen; nearly all idle load is the full-frame icon search.

**Board round done (2026-11-03):** card look, multi-select, undo, reward filter, card styles, key columns, the CONSIDER-beats-TAKE rule and smooth scrolling are in; the user is happy with the board for now. **Next:** the Phase 4 decision (new chat). The board loads reward icons from `prototype/assets/mods/`; the port has to move them next to `ui/board/`.

### Phase 4 – Distributable app
- **Decision (2026-11-03): Rust + Tauri 2.** The user does not write code, so learning curve and port effort no longer count; only the result for the player does. Compared with C#/.NET (the earlier preference) and Python/Nuitka: smallest download (~10–20 MB estimated, no runtime), lowest idle RAM, built-in installer and signed auto-updater via GitHub Releases, WebView2 is already part of Windows 10/11. The strict compiler keeps a growing, AI-written code base safe to refactor.
- Native APIs through Microsoft's `windows` crate: Windows.Graphics.Capture (GPU capture, replaces `mss`) and Windows.Media.Ocr. No OpenCV: the few vision steps we need (color mask, template matching) are written in Rust.
- All UI is web UI (board, settings, module pages, HUD as a transparent click-through always-on-top window). If the HUD window costs noticeable RAM or FPS, it moves to a native Direct2D window without touching the rest.
- Port detection, matching and verdict engine. Reuse `data/`; `ui/board/` only swaps its backend adapter (`connectBackend()`). The Python tests and screenshots are the reference: the port is done when the Rust version gives the same results on the same images.
- Settings: HUD position/scale, profile switching, start with Windows (optional).
- Releases via GitHub with a changelog.
- The app becomes a generic PoE helper with modules (see Phase 6). It needs a new name; architecture spec and port plan come next.
- **Plan A done (2026-11-03):** the Rust pipeline (`crates/vision`, `crates/ocr`, `modules/altar`) reproduces the prototype on the 39 local screenshots: `altars detected : 44/44 (+0 false)`, `options correct : 84/85 (98.8 %)`, same single miss (`screen_10.png [856, 1152] bottom`). Median per screen 174 ms vs 202 ms for Python on the same machine (244 ms measured earlier). `cargo run -p altar --release --bin evaluate` prints the report; `modules/altar/tests/golden.rs` guards it. Next: Plan B (live app).
- **Carry-over into Plan B** (from the Plan A reviews): (1) detection costs ~141 ms CPU per 3440×1440 frame on one thread (35 ms wall on 24 threads); before the 4 Hz live loop, add an exact integer fast path for the ×1/4 and ×1/2 downscale, run matching in a small dedicated thread pool instead of rayon's global one, and measure against the ≤ 3 % CPU budget. (2) `Reader` resolves assets and `mods.json` from the source tree; the app needs `Reader::with_paths(...)` or embedded icons. (3) The live loop logs a failed OCR read and keeps going instead of dropping the frame. (4) Saving profiles from the board keeps the raw JSON (as the prototype does) instead of round-tripping through `Profile`, so unknown keys and order survive. (5) Live frames enter through `Rgb::from_bgra(w, h, stride, bytes)`, which now checks the row stride.
- **Plan B1 done (2026-11-03):** plan `docs/superpowers/plans/2026-11-03-plan-b1-live-scanner.md`, branch `port/plan-b1`. Detection 29 ms CPU per 3440×1440 frame single-threaded (was ~130 ms); `crates/core` (paths, settings, profiles, module contract, runner), `crates/game` (log watcher, PoE window tracker), `crates/capture` (Windows.Graphics.Capture, ~1–4 ms CPU per grab), `modules/altar` `AltarModule` (4 Hz, only while PoE is in front and in a map or the area is unknown). `pov-scan` (`apps/scan`) is the headless console scanner: it finds the PoE window, follows the area via `Client.txt`, captures only while scanning and prints one line per altar (`<altar> top: <verdict> [<mod ids>] | bottom: ... -> <recommendation>`) whenever the result changes, plus a status line on context changes ("PoE not found", "in hideout – idle", "in map – scanning"). `--stats` prints every 10 s scans/s, scan avg/max ms, process CPU and altar count; `--replay <dir> --hold <s>` cycles screenshots instead of the game. Measured in replay (39 screenshots, 3440×1440, 24 logical cores): 2.8–3.0 scans/s, scan avg 83–106 ms / max 253–335 ms, CPU 1.1–1.3 % of total (26–30 % of one core). Without the game: "PoE not found", CPU 0.01 % of total. Once per user: `pov-scan --import-prototype` copies the prototype's custom profiles and `settings.json` into `%APPDATA%\miicrop-pov` (never overwrites). Merged into `main` (ff, 763e9ad). **In-game test (2026-11-03, user, borderless 3440×1440, 24 logical cores, after `--import-prototype`):** PoE window found; map/hideout/background detected, capture stops when idle and resumes by itself; verdicts correct (user-confirmed); in map 3.2–3.4 scans/s, scan avg 35–45 ms (max ~220–310 ms when a new altar is read), CPU 0.7 % of total (16–18 % of one core); FPS unchanged; no yellow capture border; Ctrl+C stops cleanly; "(partly hidden)" shown until the altar is fully read. **Next:** Plan B2 (Tauri UI).
- **Carry-over into Plan B2** (from the final B1 branch review): (1) device-lost recovery in capture: recreate the D3D device after `DXGI_ERROR_DEVICE_REMOVED` or repeated failures; (2) `RoInitialize` on the capture thread; (3) settings `save` must preserve unknown keys before the settings window saves; (4) drop the release-build fallback to the source-tree dev data before the installer; (5) frame staleness: the 2-buffer pool is drained every 250 ms, so frames can be up to one interval old; consider a `FrameArrived` handler; (6) the status logic in `pov-scan` duplicates `wants_frames`; move it to a shared helper; (7) `client_log` is only read at startup; (8) runner run-loop and multi-module tests once a second module arrives; (9) drop the "capture: no frame available yet" message after each resume (harmless: the first frame just needs a moment); (10) in-game, after walking away from an altar and back, the same altar showed twice for a moment (one complete, one partly read; "altars 2"): user considers it acceptable, keep an eye on the tracker matching in B2.
- **Plan B2 done (2026-11-04):** plan `docs/superpowers/plans/2026-11-03-plan-b2-tauri-app.md`, branch `port/plan-b2`. `pov-app` (`app/`, Tauri 2) is the desktop app "Path of Vision": a main window with sidebar (Home with module switch and status, Altars = the board served over Tauri IPC, without its former Settings panel), a tray icon (left click opens, menu Open/Quit), single instance (a second start focuses the running one), close-to-tray (closing the main window hides it, Quit in the tray ends the app), the altar HUD as a separate click-through, topmost, never-activating window at the bottom-center of the PoE client area, and a settings window built from the settings schema (HUD offsets/scale, autostart option, game-log path `Client.txt`). All driven by the B1 runner; nothing touches the network and all pages are local files. Start: `cargo run -p pov-app --release` (or `target\release\pov-app.exe`); flags `--replay <dir> --hold <s>` (cycle screenshots instead of the game), `--stats` (every 10 s scans/s, scan ms, CPU), `--dev`. **Measured (2026-11-04, 24 logical cores):** replay (`--replay tests/fixtures/screens --hold 1 --stats`, 60 s): 2.7–3.0 scans/s, scan avg 76–116 ms / max 191–269 ms, CPU 1.05–1.39 % of total (25–33 % of one core), 2 to 3 altars. RAM in replay with main window open: pov-app 599 MB working set / 569 MB private (replay holds all 39 screenshots in memory) plus 7 WebView2 processes, total 999 MB working set / 811 MB private; main window closed to tray: 1030 MB / 829 MB (pov-app 605 MB, WebView2 425 MB working set / 255 MB private), so the replay figure is dominated by the screenshots. Live mode without the game (no flags, PoE not running, main window closed to tray, HUD hidden, after 25 s): pov-app 59 MB working set / 57 MB private plus 7 WebView2 processes, total 444 MB working set / 245 MB private; CPU of the app process 0.01–0.02 % of total (0.2–0.5 % of one core; WebView2 child processes not included in `--stats`). Closing the main window only hides it, the WebView stays alive. The Python board (`prototype/board.py`) no longer works because its icons moved to `ui/modules/altar/img/` and the Tauri board replaces it; the Python scanner and HUD still run. **Next:** the user's in-game test (HUD look and verdicts vs. prototype, no focus loss while typing in chat, clicks go through, FPS unchanged, CPU ≤ 3 %, RAM with main window closed, board scrolling at 144 Hz, tray/close/second start), then Plan C; if the WebView HUD is clearly heavier than expected, the Direct2D HUD becomes a Plan C item.

### Phase 5 – Future changes (backlog, not committed)
- **Resolutions & UI scale:** support 1080p, 1440p, 4K, ultrawide and different in-game UI scales (scale-invariant detection, test fixtures per resolution).
- **Profile import/export** as a shareable text code (like PoB codes).
- **In-app mod data updates:** download the newest `mods.json` from releases. New mods show up as "new" on the board.
- **Profile hotkey switching** (e.g. per farm strategy).
- **Roll value scaling:** a high roll scores better than a low roll within the same tier. Needs the player's atlas scaling, since in-game values exceed the base ranges.
- **Target from the icon halves:** classify crown/figure/group in the icon to know a box's target even when its header is hidden.
- ~~**Compact HUD**~~ ✅ done (2026-11-02): the HUD repeated the full mod text, which is a lot to read mid-fight. Done: `data/labels.json` (hand-made, one entry per card, kept in sync by `tools/gen_mods`: new cards get an empty entry, the `text` reference copy is refreshed, aliases and icons are never touched). `alias` replaces the full text in the HUD (`#` = next read value, `#2` = second value; empty alias = full text), `icon` names a file in `prototype/assets/mods/`. HUD rows are columns: sign | icon | alias or text | values. Edits to labels.json apply while running. Aliases are filled in for all 201 cards. Currency, Eldritch and scarab icons (52, used by 104 cards) are the original item art: game tables `BaseItemTypes` → `ItemVisualIdentity.DDSFile` exported with `pathofexile-dat`, then `Art/2DItems/Currency/…` converted to PNG. Scarab categories use the base tier icon (`LesserScarab…`); Influence uses the Shaper scarab. HUD options in `settings.json`, applied while running: `hud_font_size` (icons scale with it), `hud_tiers` = `highlight` / `off` (row tint: top upside column gold, other rated columns purple, Dangerous orange, Brick red; the tier text column was dropped with the custom columns). `hud_other_opacity` (default 0.6) dims the option that is not the pick; the pick (firm choice, or the option a "Your call" names) gets a glow in its verdict color; an ADAPT - BRICK option is never dimmed. Keyword colors in `data/keywords.json` (whole words, any case; starts with the elements, chaos and physical). Composed icons (Pillow, from original art): `div-card-*` = divination card with the reward item in the corner (incubator style; uniques via `UniqueStashLayout`, e.g. Kaom's Heart, Starforge), `dup-*` = representative item with a ×2 badge. Currency, div cards, gems, maps, scarabs, uniques, jewellery, armour and weapons use the standard reward symbols players know from incubators/delirium orbs (UI sprites `Heist/Blueprints/Rewards/HeistReward*`; gem with "20" / "20%" for levelled / quality gems). Quantity/Rarity/Experience use letter tiles (Q green, R rare-yellow, XP blue). Downsides stay without icon for now (decided 2026-11-02, not relevant enough).
- ~~**Original icon files**~~ ✅ done: extracted from the game bundles (Steam install, `Bundles2`) with the npm package `pathofexile-dat`. Sprite index `art/uiimages1.txt` → `Art/2DArt/UIImages/InGame/AtlasAltar/4k/{Top,Bottom}Icon{Boss,Player,InfluencedMonsters}{Tangled,CleansingFire}` (200×100) plus `{Top,Bottom}IconFrame`, composed into `prototype/assets/target_{target}_{top|bottom}_{altar}.png`. The same sheet also has the altar backgrounds and outer frames. Still open: check GGG's terms before shipping game art in a public release.
- **Partial reads:** show "incomplete" in the HUD when a box is cut off by HUD or screen edges, instead of a verdict on half the mods.
- **Configurable thresholds** per profile (e.g. which upside columns trigger CONSIDER / ADAPT - BRICK). Partly covered by the custom columns and key columns: only the top column and key columns can make a Brick workable.
- **Combination rules** ("downside X only matters together with upside Y").
- **Accessibility:** colorblind-safe verdict colors (HUD text size is done, see Overlay settings).
- **Additional game languages** (needs per-language mod data and OCR models).

### Phase 6 – More modules (ideas, not committed)
Collected 2026-11-03. Only the altar module is ported in Phase 4; the architecture must let these fit in later without rework.
- **Atlas tree overlay:** import a planned atlas tree (poeplanner and similar sites, which ones is open) and show in-game where to go next and what is still missing.
- **Passive tree overlay:** the same for the character tree, imported from Path of Building.
- **Travel wheel:** radial menu at the mouse (like the WoW addon OPie); pick a direction to travel to the hideout or other places reachable with a `/` command. The wheel may serve other uses later. **Conflicts with ground rule "never send input to the game"**, see open questions.
- **Stash prices:** overlay on special stash tabs with poe.ninja prices.
- **Crafting help:** no concrete idea yet.
- **Faustus trade helper:** fetch prices and compute good ratios instead of using a calculator.
- **Nameless Ritual alert:** overlay when a Ritual is Nameless.
- **Betrayal helper:** recommend the right choice for the 5-5-2-2 strategy.

## 11. Open questions

- **App name:** "AltarScanner" is too narrow for a multi-module helper. Working name: **Path of Vision** (final name later).
- **Chat-command hotkeys and pasting (travel wheel, regex strings):** typing `/hideout` or pasting a stash regex sends input to the game, which section 2 rules out today. The user expects this to be tolerated (nearly every popular tool, e.g. Awakened PoE Trade, does it) or solvable another way. Future topic: the architecture leaves room for an input-output service, but nothing is built and the ground rule stays until it is checked.
- ~~Which game data export contains the altar mods with stable IDs?~~ RePoE, domain `primordial_altar` (see Phase 1).
- ~~How are multi-line mods represented?~~ As one mod with several lines; `lines`/`templates` keep them per line.
- When do HardMode variants spawn? They are not Ruthless-only (seen in normal maps); maybe tied to an atlas passive or map tier.
- How far can the boxes be from the altar icon at different camera positions and resolutions?
- ~~Can an option have several upsides/downsides?~~ Yes, up to 3 downsides and 2 upsides seen; the box simply grows (tallest ~250 px).
- How do boxes behave at other resolutions and UI scales? Only 3440×1440 is covered so far.
- **Atlas passives (to test):** the test screenshots had the Eater passive allocated (upsides ~50 % stronger, which explains the values above the base ranges). The Exarch passive (reportedly a 50 % chance for an additional upside) was **not** allocated, so boxes with an extra upside line are untested. Collect a few screenshots with it.
