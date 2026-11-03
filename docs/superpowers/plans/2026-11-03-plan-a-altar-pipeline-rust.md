# Plan A – Altar pipeline in Rust (offline) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [x]`) syntax for tracking.

**Status:** ✅ Done and merged into `main` (2026-11-03). Result: 44/44 altars, 84/85 options, median 179 ms per screen (Python 202 ms). Carry-over notes in `plans/Plan.md`, Phase 4.

**Goal:** A Rust workspace that finds, reads, matches and rates Eldritch Altars on the existing screenshots at least as well as the Python prototype, with no UI.

**Architecture:** Shared crates `vision` (image types, resizing, masked template matching) and `ocr` (Windows.Media.Ocr) plus the module crate `modules/altar` (mod data, text matching, verdict engine, detection, reading, an `evaluate` binary). Pure logic has no Windows dependency except the `ocr` crate. Behavior is ported from `prototype/*.py`, which stays as the reference; numbers and rules below are copied from it.

**Tech Stack:** Rust stable (edition 2024), `image` (PNG loading only), `serde`/`serde_json`, `fancy-regex` (the prototype's regexes use look-around), `rapidfuzz` (Indel ratio), `rayon`, `windows` (WinRT OCR), `anyhow`.

**Spec:** `docs/superpowers/specs/2026-11-03-path-of-vision-port-design.md` (sections 4, 6.1, 7, 11). Plans B (live app) and C (resolutions, release) follow after this one.

## Global Constraints

- Windows 10/11 only; MSVC toolchain (Visual Studio 2022 Community with C++ tools is installed; Rust is not yet).
- No OpenCV. Vision code is our own.
- Tests never touch the user's `settings.json` or `data/profiles/*` except the read-only shipped `data/profiles/default.json`.
- Screenshots `tests/fixtures/screens/*.png` are local only and git-ignored; tests needing them skip when absent. `tests/fixtures/labels.json` is in git.
- Reference result (Python, measured 2026-11-03): `altars detected : 44/44 (+0 false)`, `options correct : 84/85 (98.8 %)`, median 244 ms per screen. The one known miss is `screen_10.png [856, 1152] bottom`.
- Commit messages: plain, no `Co-Authored-By` trailer (user rule).
- Where the prototype picks the **first** maximum among ties (`max(...)` in Python), Rust must too (`Iterator::max_by` returns the last; use a fold or `rev()` accordingly).
- Image dimension rounding for scaled sizes: `(n as f64 * scale).round_ties_even() as usize` (matches OpenCV's `cvRound`).

## Review Focus

1. **PNG variants:** RGBA, RGB, palette or 16-bit PNGs load into the same `Rgb` image (alpha dropped) → Task 1 test `loads_rgba_and_rgb_png`.
2. **Icon at the screen edge:** an icon partly outside the frame is skipped, never a panic or out-of-bounds slice → Task 7 test `icon_cut_off_at_edge_is_skipped`.
3. **Empty OCR region:** an icon touching the top or bottom edge gives an empty crop; reading returns no lines for that box instead of an error → Task 8 test `empty_region_reads_no_lines`.
4. **Missing OCR language:** `Ocr::new("xx-XX")` returns an `Err` with a readable message, not a panic → Task 6 test `unknown_language_is_an_error`.
5. **Profiles in older or partial shape:** old `{"s","a","b"}` upsides, missing `downsides`, unknown mod ids all load and judge without error → Task 5 tests `old_profiles_are_upgraded_to_columns`, `profile_with_missing_parts_loads`.

---

### Task 1: Toolchain, workspace and the `vision` image types

**Files:**
- Create: `Cargo.toml` (workspace, members `crates/vision`, `crates/ocr`, `modules/altar`; `resolver = "3"`)
- Create: `crates/vision/Cargo.toml`, `crates/vision/src/lib.rs`, `crates/vision/src/image.rs`
- Create: `crates/vision/tests/fixtures/` (small generated PNGs, see Step 1)
- Modify: `.gitignore` (add `/target/`)

**Interfaces:**
- Produces (`vision`):
  - `pub struct Gray { pub w: usize, pub h: usize, pub px: Vec<u8> }` with `get(x, y) -> u8`, `crop(x0, y0, x1, y1) -> Gray` (half-open, clamped to the image).
  - `pub struct Rgb { pub w: usize, pub h: usize, pub px: Vec<[u8; 3]> }` with `load(path: &Path) -> anyhow::Result<Rgb>`, `from_bgra(w, h, bytes: &[u8]) -> Rgb` (for captures in Plan B), `crop(...) -> Rgb`, `to_gray() -> Gray`.
  - `Gray::resize_area(&self, scale: f64) -> Gray`, `Gray::resize_nearest(&self, w: usize, h: usize) -> Gray`.
  - `pub fn hsv(p: [u8; 3]) -> (u8, u8, u8)` in OpenCV's 8-bit convention (H 0–179, S and V 0–255), input in RGB order.

- [x] **Step 1: Install the toolchain and create the workspace**

Run: `winget install --id Rustlang.Rustup -e` then, in a new shell, `rustup default stable` and `cargo --version`.
Expected: a cargo version ≥ 1.85 (edition 2024). Create the workspace and an empty `vision` crate; `cargo build` succeeds. Generate the two test PNGs once with Python/Pillow (2×2 pixels, the same colors, one saved as RGBA and one as RGB) into `crates/vision/tests/fixtures/`.

- [x] **Step 2: Write the failing tests** in `crates/vision/src/image.rs` (`#[cfg(test)]`):

```rust
#[test] fn loads_rgba_and_rgb_png() { /* both fixtures load; same px; w == h == 2 */ }
#[test] fn gray_uses_opencv_weights() {
    // OpenCV: Y = (R*4899 + G*9617 + B*1868 + 8192) >> 14
    assert_eq!(Rgb { w: 1, h: 1, px: vec![[255, 0, 0]] }.to_gray().px, vec![76]);
    assert_eq!(Rgb { w: 1, h: 1, px: vec![[0, 255, 0]] }.to_gray().px, vec![150]);
    assert_eq!(Rgb { w: 1, h: 1, px: vec![[0, 0, 255]] }.to_gray().px, vec![29]);
}
#[test] fn resize_area_averages_blocks() { /* 4x4 with values 0..16 at scale 0.5 -> 2x2 of block means (rounded) */ }
#[test] fn resize_area_rounds_dims_like_opencv() { /* 141x141 at 0.25 -> 35x35, at 0.5 -> 70x70 */ }
#[test] fn hsv_matches_opencv() {
    assert_eq!(hsv([255, 0, 0]), (0, 255, 255));
    assert_eq!(hsv([0, 0, 255]), (120, 255, 255));
    assert_eq!(hsv([0, 128, 128]), (90, 255, 128));
}
```

- [x] **Step 3: Run `cargo test -p vision` and see the tests fail** (functions missing).

- [x] **Step 4: Implement** the types in `image.rs`. `resize_area` is a fractional box filter (each destination pixel is the area-weighted mean of the source pixels it covers, result rounded); `from_bgra` drops alpha.

- [x] **Step 5: Run `cargo test -p vision`**, expected PASS.

- [x] **Step 6: Commit** `git add Cargo.toml .gitignore crates/vision && git commit -m "Port: Rust workspace, vision image types"`

### Task 2: Masked template matching

**Files:**
- Create: `crates/vision/src/template.rs`

**Interfaces:**
- Consumes: `Gray` (Task 1).
- Produces:
  - `pub struct Template { pub px: Gray, pub mask: Gray }` (mask: 0 = ignore, >0 = use).
  - `pub struct ScoreMap { pub w: usize, pub h: usize, pub v: Vec<f32> }` with `max_loc(&self) -> (usize, usize, f32)` (first maximum in row-major order), `max_in_place(&mut self, other: &ScoreMap)`, `clear_disk(&mut self, cx: usize, cy: usize, r: usize)` (sets a filled disk to 0, like `cv2.circle(..., 0, -1)`).
  - `pub fn match_masked(img: &Gray, templates: &[Template]) -> ScoreMap`: size `(W - w + 1) × (H - h + 1)`; per position the best score over `templates` (all templates share size and mask).

The score equals OpenCV `TM_CCOEFF_NORMED` with a binary mask: the Pearson correlation between template and window over the mask pixels. Zero variance or NaN gives 0. Implementation: precompute each template's zero-mean values on the mask (so the numerator is `Σ t'_k · I_k`), compute `Σ I` and `Σ I²` over the mask once per position (shared by all templates), iterate a list of mask offsets (sparse), rows in parallel with `rayon`.

- [x] **Step 1: Write the failing tests:**

```rust
#[test] fn exact_copy_scores_one_at_its_position() { /* random 60x40 image, template = crop at (17, 9) size 11x11, full mask -> max_loc == (17, 9, ~1.0) */ }
#[test] fn mask_ignores_pixels_outside() { /* change pixels where mask == 0 in the image copy; score stays ~1.0 */ }
#[test] fn inverted_patch_scores_minus_one() { /* 255 - template pasted -> score ~ -1.0 at that spot */ }
#[test] fn flat_window_scores_zero() { /* constant image -> all scores 0, no NaN */ }
#[test] fn best_of_two_templates_wins() { /* two different templates, image contains the second -> ~1.0 there */ }
#[test] fn clear_disk_zeroes_a_circle() { /* r = 2 around (5,5) zeroes (5,3) and (7,5) but not (8,5) */ }
```

- [x] **Step 2: Run `cargo test -p vision template`**, expected FAIL.
- [x] **Step 3: Implement** `template.rs` as described.
- [x] **Step 4: Run `cargo test -p vision`**, expected PASS.
- [x] **Step 5: Commit** `git commit -am "Port: masked template matching"` (add the new file first).

### Task 3: Mod data and text normalization

**Files:**
- Create: `modules/altar/Cargo.toml`, `modules/altar/src/lib.rs`, `modules/altar/src/data.rs`, `modules/altar/src/text.rs`

**Interfaces:**
- Produces (`altar`):
  - `pub fn repo_data_dir() -> PathBuf` = `CARGO_MANIFEST_DIR/../../data` (dev and tests; Plan B adds app resources).
  - `#[serde(rename_all = "lowercase")] pub enum Target { Player, Minions, Boss }`, `pub enum AltarKind { Eater, Exarch }`, `pub enum Side { Upside, Downside }`.
  - `pub struct Card { pub id: String, pub side: Side, pub target: Target, pub altars: Vec<AltarKind>, pub templates: Vec<Vec<String>>, pub ranges: Vec<Vec<[f64; 2]>>, pub lines: Vec<Vec<String>> }` (per variant; `templates` already normalized).
  - `pub fn load_cards(path: &Path) -> anyhow::Result<Vec<Card>>` from `data/mods.json` (`mods[].variants[].{templates, ranges, lines}`).
  - `text::to_template(&str) -> String`, `text::normalize(&str) -> String`, `text::ratio(a, b) -> f64` (0–100), `text::partial_ratio(short, long) -> f64` (0–100), `text::parse_header(&str) -> Option<Target>`.

Exact rules (from `tools/gen_mods/gen_mods.py` and `prototype/match.py`):
- `to_template`: first replace `\((-?\d+(?:\.\d+)?)-(-?\d+(?:\.\d+)?)\)` with `#`, then `(?<![\w.#])\d+(?:\.\d+)?` with `#`.
- `normalize`: uppercase; `—` and `–` → `-`; `(?<=\d)[O](?=[\d%])|(?<=[\d.])O\b` → `0`; `(?<=\d) +(?=%)` → ``; `(?<=\d),(?=\d)` → `.`; then `to_template`; collapse whitespace to one space; trim.
- `ratio` = rapidfuzz `fuzz.ratio` (normalized Indel similarity × 100).
- `partial_ratio` = rapidfuzz `fuzz.partial_ratio` for a needle ≤ 64 chars: best `ratio` of the needle against every window of the needle's length in the haystack **and** against the shorter windows at both ends (prefixes and suffixes of the haystack shorter than the needle).
- `parse_header`: `up = text.to_uppercase().trim()`; `None` if `up` is longer than 28 chars or `partial_ratio("GAIN", up) < 75`; best key of `PLAYER`, `MINION`, `BOSS` by `partial_ratio` (first max wins); `Some(target)` if that score ≥ 80.

- [x] **Step 1: Write the failing tests** (`modules/altar/src/text.rs`, `data.rs`):

```rust
#[test] fn normalize_fixes_ocr_slips() {
    assert_eq!(normalize("2,5% chance to drop an additional Jeweller's Orb"), normalize("2.5% CHANCE TO DROP AN ADDITIONAL JEWELLER'S ORB"));
    assert_eq!(normalize("1O% increased Quantity"), "#% INCREASED QUANTITY");
    assert_eq!(normalize("33 % chance"), "#% CHANCE");
    assert_eq!(normalize("Final Boss drops (2-4) additional Chaos Orbs"), "FINAL BOSS DROPS # ADDITIONAL CHAOS ORBS");
    assert_eq!(normalize("-(60-40)% to Cold Resistance"), "-#% TO COLD RESISTANCE");
    assert_eq!(normalize("3.2% x1.5 ab#2"), "#% X1.5 AB#2");
}
#[test] fn ratio_matches_rapidfuzz() {
    assert!((ratio("THIS IS A TEST", "THIS IS A TEST!") - 96.5517).abs() < 1e-3);
    assert!((ratio("#% INCREASED QUANTITY", "#% INCREASED QUANTlTY") - 95.2381).abs() < 1e-3);
}
#[test] fn partial_ratio_matches_rapidfuzz() {
    for (a, b, want) in [("GAIN", "MINIONS GAN:", 75.0), ("PLAYER", "PLAVER GAINS:", 83.3333), ("BOSS", "MAP B0SS GAINS:", 75.0),
                         ("GAIN", "NEARBY ENEMIES GAIN 100%", 100.0), ("GAIN", "AIN:", 85.7143), ("BOSS", "SS GAINS", 66.6667)] {
        assert!((partial_ratio(a, b) - want).abs() < 1e-3, "{a} / {b}");
    }
}
#[test] fn parse_header_cases() {
    assert_eq!(parse_header("PLAYER GAINS:"), Some(Target::Player));
    assert_eq!(parse_header("ELDRITCH MINIONS GAIN:"), Some(Target::Minions));
    assert_eq!(parse_header("MINIONS GAIN:"), Some(Target::Minions));
    assert_eq!(parse_header("MAP Boss GAINS:"), Some(Target::Boss));
    assert_eq!(parse_header("NEARBY ENEMIES GAIN 100% OF THEIR PHYSICAL DAMAGE AS EXTRA COLD DAMAGE"), None);
    assert_eq!(parse_header("GAIN 82% OF PHYSICAL DAMAGE AS EXTRA FIRE DAMAGE"), None);
}
#[test] fn loads_all_cards() { /* load_cards(repo_data_dir()/"mods.json") has 201 cards; first id "CleansingAltarUpsideBossChaosOrb", ranges[0] == [[2.0, 4.0]] */ }
```

- [x] **Step 2: Run `cargo test -p altar`**, expected FAIL.
- [x] **Step 3: Implement** `data.rs` and `text.rs` (regexes compiled once with `std::sync::LazyLock`).
- [x] **Step 4: Run `cargo test -p altar`**, expected PASS.
- [x] **Step 5: Commit** `git add modules/altar Cargo.toml && git commit -m "Port: altar mod data and text normalization"`

### Task 4: Box matching and value filling

**Files:**
- Create: `modules/altar/src/matching.rs`, `modules/altar/src/fill.rs`

**Interfaces:**
- Consumes: `Card`, `Target`, `normalize`, `ratio` (Task 3).
- Produces:
  - `pub struct Match { pub card: usize /* index into the cards slice */, pub lines: Vec<usize>, pub score: f64 /* rounded to 0.1 */, pub values: Vec<f64>, pub in_range: bool, pub partial: bool, pub variant: usize, pub assign: Vec<Option<usize>> }`
  - `Match::display(&self, cards: &[Card], ocr_lines: &[String]) -> Vec<String>`
  - `pub fn match_box(lines: &[String], cards: &[Card], target: Option<Target>) -> (Vec<Match>, Vec<usize>)` (matches sorted by first line; unexplained line indices ascending).
  - `fill::fill_values(data_line: &str, ocr_line: &str) -> String`, `fill::matching_blocks(a: &[String], b: &[String]) -> Vec<(usize, usize, usize)>`.

Port `prototype/match.py` `_candidates`, `match_box` and `_fill_values` behavior-for-behavior, with these pinned values and orders:
- `MIN_LINE_SCORE = 80`, `LINE_MARGIN = 6`; number regex for values `\d+(?:\.\d+)?`; value-slot regex `\(-?[\d.]+--?[\d.]+\)|\d+(?:\.\d+)?`.
- Candidate order: cards in file order → variants → Cartesian product of per-template options in order; then a **stable** sort by descending score.
- Exact-cover search key `(lines covered, -partial count, score sum)`, compared lexicographically, strictly greater replaces.
- Without a target: try `Player`, `Minions`, `Boss` in that order; pick by `(lines covered, score sum)`, first maximum wins.
- `in_range` compares magnitudes: `bounds = sorted(|lo|, |hi|)`, all values inside and same count.
- `matching_blocks` is a port of Python `difflib.SequenceMatcher(None, a, b, autojunk=False).get_matching_blocks()` (recursive longest-match, no junk heuristics, trailing `(len_a, len_b, 0)` sentinel, adjacent blocks merged).
- `fill_values`: convert `4,3` → `4.3` in the OCR line first; tokenize both lines to words of `[A-Z#]+` after replacing value slots with ` # ` and uppercasing; map slots via `matching_blocks`; unmatched slots keep the data text.

- [x] **Step 1: Write the failing tests** (`modules/altar/tests/matching.rs`), ported from `tests/test_match.py` with the same lines and expected card ids:
  - `interleaved_multi_line_mods` (boss; 6 lines; no rest; ids `TangledAltarDownsideBossPhysToAddAsLightning`, `TangledAltarDownsideBossPhysToAddAsCold`, `TangledAltarUpsideBossDivinationCardsCurrencyBasic`)
  - `shared_line_goes_to_the_right_mods` (boss; ids `CleansingAltarDownsideBossPhysToAddAsFire`, `CleansingAltarDownsideBossCoveredInAshOnHit`)
  - `same_mod_twice` (player; `TangledAltarUpsidePlayerIncreasedRarity` × 2)
  - `partly_hidden_multi_line_mod` (minions; `CleansingAltarDownsideMonsterFireAndChaosResist` partial, `CleansingAltarUpsideMonsterOrbOfUnmaking` not)
  - `unknown_target_picks_the_best_fit` (target `None`; all matches boss)
  - `display_fills_read_values_into_clean_text` (`["33% chance to be Sapped when Hit", "All Damage taken from Hits can Sap you"]`)
  - `fill_values_aligns_by_words`, the three cases from the Python test, e.g. `fill_values("-(60-40)% to Cold Resistance", "-52% TO COLD RESISTANCE") == "-52% to Cold Resistance"`
  - `matching_blocks_like_difflib`: `a = ["A","B","C","D"]`, `b = ["B","C","X","D"]` → `[(1,0,2), (3,3,1), (4,4,0)]`
- [x] **Step 2: Run `cargo test -p altar --test matching`**, expected FAIL.
- [x] **Step 3: Implement** `fill.rs` and `matching.rs`.
- [x] **Step 4: Run `cargo test -p altar`**, expected PASS.
- [x] **Step 5: Commit** `git commit -m "Port: exact-cover box matching and value filling"` (after `git add`).

### Task 5: Profiles and verdict engine

**Files:**
- Create: `modules/altar/src/verdict.rs`

**Interfaces:**
- Consumes: `Target` (Task 3).
- Produces:
  - `pub struct Column { pub name: String, pub ids: Vec<String>, #[serde(default, skip_serializing_if = "is_false")] pub key: bool }`, `pub struct Upsides { pub columns: Vec<Column>, pub top: Column }`
  - `pub struct Profile { pub name: String, pub upsides: Upsides, pub downsides: HashMap<String, Vec<String>>, pub read_only: bool }` with `from_json(&str) -> anyhow::Result<Profile>` (JSON keys `name`, `upsides`, `downsides`, `readOnly`; old `{"s","a","b"}` upsides become `columns = [B, A]`, `top = S`; a missing top name is `"S"`), `load(&Path)`, `up_tier(&str) -> (Option<UpKind>, usize, usize)`, `down_tier(&str) -> Option<DownTier>`.
  - `pub enum UpKind { Top, Rated }`, `pub enum DownTier { Brick, Dangerous }`, `pub enum Pos { Top, Bottom }`.
  - `pub enum Verdict { Take, Consider, Adapt, Avoid, Neutral, Unreadable }` with `label()` → `"TAKE"`, `"CONSIDER"`, `"ADAPT - BRICK"`, `"AVOID"`, `"NEUTRAL"`, `"UNREADABLE"`.
  - `pub struct OptionInput { pub target: Option<Target>, pub upsides: Vec<String>, pub downsides: Vec<String> }`
  - `pub struct OptionVerdict { pub verdict: Verdict, pub up_tier: Option<UpKind>, pub up_pos: usize, pub down_tier: Option<DownTier>, pub tiers: HashMap<String, Tier>, pub up_rank: usize }` where `pub enum Tier { Top, Rated, Dangerous, Brick, None }`
  - `pub fn judge(option: &OptionInput, profile: &Profile) -> OptionVerdict`
  - `pub struct Recommendation { pub choice: Option<Pos>, pub text: String, pub about: Option<Pos> }`
  - `pub fn recommend(options: &[OptionInput; 2], verdicts: &[OptionVerdict; 2]) -> Recommendation` (index 0 = top, 1 = bottom)

Port `prototype/verdict.py` exactly (rules, texts, "unrated" position `10^6`, key columns get `UpKind::Top` but keep their rank, CONSIDER-beats-TAKE when its rank is strictly higher, ties prefer the top option, minions as neutral default).

- [x] **Step 1: Write the failing tests** (`modules/altar/tests/verdict.rs`): port every test of `tests/test_verdict.py` 1:1 (same profiles, ids and expected verdicts/texts), including `test_verdict_table` as a table over the 8 rows, `test_default_profile_loads` (reads `data/profiles/default.json`, read-only, top ids not empty) and `test_old_profiles_are_upgraded_to_columns`. Add:

```rust
#[test] fn profile_with_missing_parts_loads() {
    let p = Profile::from_json(r#"{"name": "x"}"#).unwrap();
    let v = judge(&OptionInput { target: Some(Target::Boss), upsides: vec!["NotAMod".into()], downsides: vec!["AlsoNot".into()] }, &p);
    assert_eq!(v.verdict, Verdict::Neutral);
}
```

- [x] **Step 2: Run `cargo test -p altar --test verdict`**, expected FAIL.
- [x] **Step 3: Implement** `verdict.rs`.
- [x] **Step 4: Run `cargo test -p altar`**, expected PASS.
- [x] **Step 5: Commit** `git commit -m "Port: profiles and verdict engine"` (after `git add`).

### Task 6: OCR crate (Windows.Media.Ocr)

**Files:**
- Create: `crates/ocr/Cargo.toml`, `crates/ocr/src/lib.rs`, `crates/ocr/tests/ocr.rs`, `crates/ocr/tests/fixtures/header.png`

**Interfaces:**
- Consumes: `vision::Gray`.
- Produces: `pub struct Ocr`, `Ocr::new(language: &str) -> anyhow::Result<Ocr>`, `Ocr::read(&self, img: &Gray) -> anyhow::Result<Vec<OcrLine>>`, `pub struct OcrLine { pub text: String, pub x: i32, pub y: i32, pub w: i32, pub h: i32 }` (box = union of the line's word rects, as in `prototype/ocr.py`). `Ocr` is `Send + Sync`.

Use the `windows` crate (features for `Media_Ocr`, `Globalization`, `Graphics_Imaging`, `Storage_Streams`, `Foundation`, `Foundation_Collections`): `OcrEngine::TryCreateFromLanguage`, `SoftwareBitmap::CreateCopyFromBuffer` with `BitmapPixelFormat::Bgra8` (gray → B=G=R, A=255, buffer via `DataWriter`), `RecognizeAsync(...)` awaited by blocking `.get()`. A failed `TryCreateFromLanguage` becomes `Err("Windows OCR language <lang> is not installed")`.

- [x] **Step 1: Create the fixture**: render `PLAYER GAINS:` in black on white, Arial 36 px, ~400×60, with Pillow; save as `crates/ocr/tests/fixtures/header.png`.
- [x] **Step 2: Write the failing tests:**

```rust
#[test] fn reads_a_rendered_line() { /* Ocr::new("en-US"), read fixture (to gray) -> exactly one line, text == "PLAYER GAINS:", box inside the image */ }
#[test] fn unknown_language_is_an_error() { assert!(Ocr::new("xx-XX").is_err()); }
#[test] fn blank_image_reads_nothing() { /* 200x50 white -> empty vec */ }
```

- [x] **Step 3: Run `cargo test -p ocr`**, expected FAIL.
- [x] **Step 4: Implement** `lib.rs`.
- [x] **Step 5: Run `cargo test -p ocr`**, expected PASS.
- [x] **Step 6: Commit** `git add crates/ocr Cargo.toml && git commit -m "Port: Windows OCR crate"`

### Task 7: Stage 1 detection

**Files:**
- Create: `modules/altar/src/detect.rs`, `modules/altar/assets/icon_eater.png`, `modules/altar/assets/icon_exarch.png` (copies of `prototype/assets/icon_*.png`, 141×141)

**Interfaces:**
- Consumes: `Gray`, `Rgb`, `hsv`, `Template`, `match_masked`, `ScoreMap` (Tasks 1–2), `AltarKind` (Task 3).
- Produces: `pub struct Icon { pub x: i32, pub y: i32, pub score: f32 }` (ring center, full resolution), `pub struct Detector`, `Detector::new(assets: &Path) -> anyhow::Result<Detector>`, `Detector::find_icons(&self, gray: &Gray) -> Vec<Icon>`, `pub fn altar_type(img: &Rgb, icon: &Icon) -> AltarKind`, `pub fn assets_dir() -> PathBuf` (`CARGO_MANIFEST_DIR/assets`), `pub const RING: (i32, i32) = (52, 63)`.

Port `prototype/detect.py` with its constants: `COARSE_SCALE = 0.25`, `FINE_SCALE = 0.5`, `COARSE_THRESHOLD = 0.35`, `THRESHOLD = 0.55`, `RING = (52, 63)`, `FINE_WINDOW = 12`, `max_icons = 6`, candidate loop up to `2 × max_icons`, suppression radius `int(63 × 0.25 × 1.5) = 23` at the coarse scale, skip a window smaller than the fine template, accept a new icon only if it is more than `RING.1` away in x or y from every accepted one. Ring mask: radius = template size / 2 (70); pixels with distance in `[52, 63]` plus the divider bar `|dy| ≤ 4 && r < 52`; mask resized with `resize_nearest` to each scaled template size. `altar_type`: patch of half-size `RING.0 - 4` around the center; vivid = `S > 90 && V > 70`; red = `H < 20 || H > 165`, blue = `80 < H < 125`; `Exarch` if red > blue, else `Eater`.

- [x] **Step 1: Write the failing tests** (`modules/altar/tests/detect.rs`). Build synthetic frames: a 1200×700 frame of dark noise (seeded, values 10–60) with an icon PNG pasted at a known top-left.

```rust
#[test] fn finds_a_pasted_icon() { /* eater icon at top-left (400, 250) -> one Icon with |x - 470| <= 2, |y - 320| <= 2, score >= 0.66; altar_type == Eater */ }
#[test] fn tells_exarch_from_eater() { /* exarch icon pasted -> altar_type == Exarch */ }
#[test] fn finds_two_icons() { /* eater at (100, 100), exarch at (800, 400) -> two icons */ }
#[test] fn noise_alone_finds_nothing() { /* no icon pasted -> empty */ }
#[test] fn icon_cut_off_at_edge_is_skipped() { /* icon pasted at top-left (-60, 300), i.e. partly outside -> no panic, empty result */ }
```

- [x] **Step 2: Run `cargo test -p altar --test detect`**, expected FAIL.
- [x] **Step 3: Implement** `detect.rs`; copy the assets.
- [x] **Step 4: Run `cargo test -p altar`**, expected PASS.
- [x] **Step 5: Commit** `git add modules/altar && git commit -m "Port: altar icon detection"`

### Task 8: Stage 2 reading

**Files:**
- Create: `modules/altar/src/read.rs`

**Interfaces:**
- Consumes: `Detector`, `Icon`, `altar_type`, `RING` (Task 7), `Ocr`, `OcrLine` (Task 6), `match_box`, `Match` (Task 4), `parse_header` (Task 3), `Pos` (Task 5).
- Produces:
  - `pub struct Line { pub text: String, pub x0: f32, pub x1: f32, pub y0: f32, pub y1: f32 }`
  - `pub fn preprocess(crop: &Rgb) -> Gray` (max channel > 110 → 0 else 255, then 2× bicubic upscale)
  - `pub fn merge_rows(lines: Vec<Line>) -> Vec<Line>`, `pub fn box_lines(lines: Vec<Line>, cx: f32, near_y: f32, upward: bool) -> Vec<Line>`
  - `pub struct OptionRead { pub position: Pos, pub target: Option<Target>, pub lines: Vec<String>, pub matches: Vec<Match>, pub unexplained: Vec<String>, pub has_header: bool }` with `complete(&self) -> bool`
  - `pub struct AltarRead { pub icon: Icon, pub altar: AltarKind, pub options: [OptionRead; 2] }`
  - `pub struct Reader { pub detector: Detector, pub ocr: Ocr, pub cards: Vec<Card> }`, `Reader::new() -> anyhow::Result<Reader>` (repo data + assets, OCR `en-US`), `Reader::read_altars(&self, img: &Rgb) -> anyhow::Result<Vec<AltarRead>>`

Port `prototype/read.py` with `SEARCH_HALF_WIDTH = 650`, `SEARCH_HEIGHT = 320`, `UPSCALE = 2`, `TEXT_BRIGHTNESS = 110`, `CENTER_TOLERANCE = 20`, `MAX_FRAGMENT_GAP = 220`, `LINE_GAP = 22` (first gap may be `3 × LINE_GAP`), OCR coordinates mapped back by `/ UPSCALE` plus the region offset, header cut (lines before the first header line dropped), target fallback to the first match's card target, altar dropped when no option has a match. An empty crop returns no lines (not an error).

- [x] **Step 1: Write the failing tests** (pure functions, no OCR; `modules/altar/tests/read.rs`):

```rust
#[test] fn merge_rows_joins_fragments_of_one_row() { /* "HITS ALWAYS" (x 100–300, y 10–30) + "SHOCK" (x 420–500, y 12–31) -> one line "HITS ALWAYS SHOCK", x1 500 */ }
#[test] fn merge_rows_keeps_far_fragments_apart() { /* gap 221 px -> two lines */ }
#[test] fn box_lines_takes_the_contiguous_block_next_to_the_icon() { /* upward, near_y 500: lines ending at 480, 455, 430 crossing cx, a fourth at 300 -> first three, sorted top to bottom */ }
#[test] fn box_lines_ignores_lines_off_the_center_column() { /* line x 600–900 with cx 400 is dropped */ }
#[test] fn empty_region_reads_no_lines() { /* Reader on a 1200x700 frame with an icon pasted at y = 0 (top region empty) -> no panic */ }
```

The last test needs OCR and runs on Windows only (it is the only one using `Reader`).

- [x] **Step 2: Run `cargo test -p altar --test read`**, expected FAIL.
- [x] **Step 3: Implement** `read.rs`.
- [x] **Step 4: Run `cargo test -p altar`**, expected PASS.
- [x] **Step 5: Commit** `git commit -m "Port: altar box reading"` (after `git add`).

### Task 9: Evaluate binary and golden test (parity gate)

**Files:**
- Create: `modules/altar/src/evaluate.rs`, `modules/altar/src/bin/evaluate.rs`, `modules/altar/tests/golden.rs`
- Modify: `plans/Plan.md` (Phase 4: Plan A result)

**Interfaces:**
- Consumes: `Reader` (Task 8), `Rgb::load` (Task 1).
- Produces: `pub struct Report { pub altars_found: usize, pub altars_total: usize, pub false_altars: usize, pub options_ok: usize, pub options_total: usize, pub median_ms: f64, pub max_ms: f64, pub failures: Vec<String> }` with `Display` printing the same three lines and `  FAIL ...` lines as `prototype/evaluate.py`; `pub fn evaluate(screens: &Path, labels: &Path) -> anyhow::Result<Report>`; `pub fn fixtures_dir() -> PathBuf` (`CARGO_MANIFEST_DIR/../../tests/fixtures`).

Same rules as `prototype/evaluate.py`: screens `screen_*.png` sorted by name, `ICON_TOLERANCE = 40`, `"mods": null` skipped, option correct when target and multiset of card ids match, timing per screen covers the whole `read_altars` call. The binary runs `evaluate` on the default fixtures and prints the report.

- [x] **Step 1: Write the failing golden test:**

```rust
#[test] fn recognition_matches_the_prototype() {
    let screens = fixtures_dir().join("screens");
    if !screens.join("screen_02.png").exists() { eprintln!("skipped: no local screenshots"); return; }
    let r = evaluate(&screens, &fixtures_dir().join("labels.json")).unwrap();
    assert_eq!((r.altars_found, r.false_altars), (r.altars_total, 0), "{r}");
    assert!(r.options_ok + 1 >= r.options_total, "{r}");   // Python: 84/85
}
```

- [x] **Step 2: Run `cargo test -p altar --release --test golden -- --nocapture`**, expected FAIL (missing `evaluate`).
- [x] **Step 3: Implement** `evaluate.rs` and the binary.
- [x] **Step 4: Run `cargo test -p altar --release --test golden -- --nocapture`**, expected PASS. Then `cargo run -p altar --release --bin evaluate` and compare with `.venv\Scripts\python -m prototype.evaluate`: same detections, ≥ 84/85 options, median time per screen ≤ 244 ms. If slower, profile `match_masked` first (Task 2) before touching anything else.
- [x] **Step 5: Run the whole suite** `cargo test --workspace --release` and `.venv\Scripts\python -m pytest -q`; both green.
- [x] **Step 6: Update `plans/Plan.md`** Phase 4 with a short "Plan A done" entry (Rust result line, timing, crates) and commit: `git add modules/altar plans/Plan.md && git commit -m "Port: evaluate binary, Rust pipeline matches the prototype"`

---

## After Plan A

Plan B (live app: Tauri shell, tray, single instance, game context with `Client.txt`, Windows.Graphics.Capture, altar tracking from `prototype/live.py`, HUD window, board and settings window over IPC, settings/profile store) and Plan C (resolution scaling with friends' fixtures, installer, updater, README, CI) are written when Plan A is done, using what it taught.
