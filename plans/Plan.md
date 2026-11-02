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
| Tech path | Python prototype first, then port to a distributable app |
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
Search: _______
```

- Filters combine (AND). Choosing an altar also shows generic mods.
- **Upside board:** `Unrated | B | A | S`. Order inside a column = priority (top = best).
- **Downside board:** `Fine | Dangerous | Brick`. Unrated downsides count as Fine.
- New mods after a data update land in Unrated/Fine and are flagged as new.
- Cards show: altar tag(s) (generic = Eater tag + Exarch tag), target, mod text with value range, weight, "rare" marker for low weights.
- Drag & drop between and within columns. Arrow buttons as a fallback.
- Profiles: read-only **Default** profile, **Duplicate** to create an editable copy, switch via dropdown. Every change is saved and applied to the overlay immediately.

## 6. Verdict model

Per option, using the best upside tier and the worst downside tier:

| Downside (worst) | Upside (best) | Verdict |
|---|---|---|
| Brick | S | **ADAPT - BRICK** (a brick by default, workable with build adjustments) |
| Brick | below S | **AVOID** |
| Dangerous | B / A / S | **CONSIDER** |
| Dangerous | Unrated | **AVOID** |
| Fine | B / A / S | **TAKE** |
| Fine | Unrated | **NEUTRAL** |

Assumption: users sort strictly, so anything rated B or better is worth considering. Thresholds can be changed later (see Future).

Recommendation line under the two options:
1. One or two TAKE → pick the better one (higher tier, then higher position in the column).
2. Otherwise CONSIDER or ADAPT - BRICK present → "Your call" with the reason shown.
3. Both AVOID → "Leave this altar alone."
4. Otherwise (NEUTRAL) → "Take the Minions option" if one option targets minions (safe default advice for new players).

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
  "upsides":   { "s": ["id1", "id2"], "a": [], "b": [] },
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
- The four values in `Altar.png` (2.2 % Alteration, 61 % ignore Phys DR, 34 % Sap, 21 % Rarity) all lie inside the generated ranges.

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

### Phase 3 – Live prototype (Python) 🔶 (built 2026-11-02, waiting for a real mapping session)
- Run: `.venv\Scripts\python -m prototype.app` (tray icon: "Settings board", "Quit"). Without the game: `--replay tests/fixtures/screens`. Board alone: `python -m prototype.board`.
- `live.py`: capture with `mss` (primary monitor by default), icon search at 4 Hz, OCR only for new altars. Altars are tracked by icon position across scans and, because running moves the icon too far between scans, also by content (≥ 60 % of the same mods on the same positions; values ignored). The better read wins (complete, more mods). Once both boxes are read completely, an altar is not read again; incomplete altars (cut off, covered by HUD) are re-read every 0.5 s.
- `verdict.py`: verdict engine with unit tests for every row of the verdict table and the recommendation rules. New verdict `UNREADABLE` when no mod of an option could be read; the recommendation then waits ("Can't read both options yet.").
- `overlay.py`: frameless, always-on-top, click-through, never takes focus; anchored at the bottom center on every size change (`hud_bottom`, `hud_offset_x` in `settings.json`). Per altar it mirrors the game: top option box, recommendation box in the middle (where the altar icon sits), bottom option box. Box borders carry the verdict color; each option shows its target as the matching half of the altar icon (original game art, top half for the top option and bottom half for the bottom one; blue = Eater, red = Exarch). Up to two altars side by side. Mod text comes from `mods.json` with the read values filled in, so stray OCR text (gold labels, NPC lines) never shows up.
- `board.py` + `ui/board/`: the tier board grown from the mockup, on the real 201 cards. **Change of plan:** hosted in Qt WebEngine (already part of PySide6) instead of pywebview, so the board opens from the same process. The page talks to Python through a five-call adapter (`connectBackend()` in `board.js`); the WebView2 port only replaces that adapter.
- Files: the board writes `data/profiles/*.json` and `settings.json` (active profile, HUD position); the scanner checks both on every scan, so changes apply instantly. User profiles and `settings.json` are not in git.
- Measured on the desktop without altars: **1.96 % total CPU** (47 % of one of 24 threads) at 4 Hz. Most of it is the `mss` screen grab (~35 ms); `dxcam` (DXGI) is the next lever if needed.
- **Done when:** a real mapping session shows correct verdicts with no noticeable FPS impact (CPU idle load from stage 1 ≤ 3 %). Open: the in-game test.

### Phase 4 – Distributable app
- Decision gate after Phase 3: port to **C#/.NET** (WPF overlay, WebView2 hosting `ui/board`, Windows.Graphics.Capture, Windows.Media.Ocr, OpenCvSharp; single-file `.exe`) or ship Python via Nuitka if it is good enough. Current preference: C#.
- Port detection, matching and verdict engine. Reuse `data/` and `ui/board/` unchanged.
- Settings: HUD position/scale, profile switching, start with Windows (optional).
- Releases via GitHub with a changelog.

### Phase 5 – Future changes (backlog, not committed)
- **Resolutions & UI scale:** support 1080p, 1440p, 4K, ultrawide and different in-game UI scales (scale-invariant detection, test fixtures per resolution).
- **Profile import/export** as a shareable text code (like PoB codes).
- **In-app mod data updates:** download the newest `mods.json` from releases. New mods show up as "new" on the board.
- **Profile hotkey switching** (e.g. per farm strategy).
- **Roll value scaling:** a high roll scores better than a low roll within the same tier. Needs the player's atlas scaling, since in-game values exceed the base ranges.
- **Target from the icon halves:** classify crown/figure/group in the icon to know a box's target even when its header is hidden.
- **Compact HUD (in progress):** the HUD repeated the full mod text, which is a lot to read mid-fight. Done: `data/labels.json` (hand-made, one entry per card, kept in sync by `tools/gen_mods`: new cards get an empty entry, the `text` reference copy is refreshed, aliases and icons are never touched). `alias` replaces the full text in the HUD (`#` = next read value, `#2` = second value; empty alias = full text), `icon` names a file in `prototype/assets/mods/`. HUD rows are columns: sign | icon | alias or text | tier. Edits to labels.json apply while running. Aliases are filled in for all 201 cards. Currency, Eldritch and scarab icons (52, used by 104 cards) are the original item art: game tables `BaseItemTypes` → `ItemVisualIdentity.DDSFile` exported with `pathofexile-dat`, then `Art/2DItems/Currency/…` converted to PNG. Scarab categories use the base tier icon (`LesserScarab…`); Influence uses the Shaper scarab. HUD options in `settings.json`, applied while running: `hud_font_size` (icons scale with it), `hud_tiers` = `off` / `text` / `highlight` (row tinted in the tier color) / `both`. `hud_other_opacity` (default 0.6) dims the option that is not the pick; the pick (firm choice, or the option a "Your call" names) gets a glow in its verdict color; an ADAPT - BRICK option is never dimmed. Keyword colors in `data/keywords.json` (whole words, any case; starts with the elements, chaos and physical). Composed icons (Pillow, from original art): `div-card-*` = divination card with the reward item in the corner (incubator style; uniques via `UniqueStashLayout`, e.g. Kaom's Heart, Starforge), `dup-*` = representative item with a ×2 badge. Currency, div cards, gems, maps, scarabs, uniques, jewellery, armour and weapons use the standard reward symbols players know from incubators/delirium orbs (UI sprites `Heist/Blueprints/Rewards/HeistReward*`; gem with "20" / "20%" for levelled / quality gems). Quantity/Rarity/Experience use letter tiles (Q green, R rare-yellow, XP blue). Downsides stay without icon for now (decided 2026-11-02, not relevant enough).
- ~~**Original icon files**~~ ✅ done: extracted from the game bundles (Steam install, `Bundles2`) with the npm package `pathofexile-dat`. Sprite index `art/uiimages1.txt` → `Art/2DArt/UIImages/InGame/AtlasAltar/4k/{Top,Bottom}Icon{Boss,Player,InfluencedMonsters}{Tangled,CleansingFire}` (200×100) plus `{Top,Bottom}IconFrame`, composed into `prototype/assets/target_{target}_{top|bottom}_{altar}.png`. The same sheet also has the altar backgrounds and outer frames. Still open: check GGG's terms before shipping game art in a public release.
- **Partial reads:** show "incomplete" in the HUD when a box is cut off by HUD or screen edges, instead of a verdict on half the mods.
- **Configurable thresholds** per profile (e.g. which upside tiers trigger CONSIDER / ADAPT - BRICK).
- **Combination rules** ("downside X only matters together with upside Y").
- **Accessibility:** colorblind-safe verdict colors, HUD text size.
- **Additional game languages** (needs per-language mod data and OCR models).

## 11. Open questions

- ~~Which game data export contains the altar mods with stable IDs?~~ RePoE, domain `primordial_altar` (see Phase 1).
- ~~How are multi-line mods represented?~~ As one mod with several lines; `lines`/`templates` keep them per line.
- When do HardMode variants spawn? They are not Ruthless-only (seen in normal maps); maybe tied to an atlas passive or map tier.
- How far can the boxes be from the altar icon at different camera positions and resolutions?
- ~~Can an option have several upsides/downsides?~~ Yes, up to 3 downsides and 2 upsides seen; the box simply grows (tallest ~250 px).
- How do boxes behave at other resolutions and UI scales? Only 3440×1440 is covered so far.
- **Atlas passives (to test):** the test screenshots had the Eater passive allocated (upsides ~50 % stronger, which explains the values above the base ranges). The Exarch passive (reportedly a 50 % chance for an additional upside) was **not** allocated, so boxes with an extra upside line are untested. Collect a few screenshots with it.
