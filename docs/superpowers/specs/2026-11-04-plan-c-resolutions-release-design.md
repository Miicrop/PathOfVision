# Plan C – Resolutions and first release

Date: 2026-11-04. Status: builder-checked, waiting for user approval.
Parent design: [2026-11-03-path-of-vision-port-design.md](2026-11-03-path-of-vision-port-design.md) (sections 8 and 10 are the starting point; this document records what changed and what is decided for Plan C). State before Plan C: [plans/Plan.md](../../../plans/Plan.md) Phase 4 (Plan A, B1, B2 merged; `pov-app` works in-game at 3440×1440).

## 1. Intent

Plan B2 produced a desktop app that works on the user's own setup (3440×1440, English client). Plan C makes it usable by **the user's friends**: install it from a public GitHub repository, run it on their own monitors, get correct altar verdicts, and receive updates without asking the user for a new file.

The friends are mostly IT people with 24" or 27" monitors at standard resolutions, from 1920×1080 up to 3840×2160 (user's estimate). They understand GitHub.

A second, long-term intent (user): future modules (Plan.md Phase 6) should **not** each need large screenshot collections per resolution. The resolution work is therefore done once in the shared core, and modules inherit it.

## 2. Goals, non-goals, anti-goals

**Goals**
1. The altar module gives correct verdicts at 1920×1080, 2560×1440 and 3840×2160 (16:9), and keeps its current quality at 3440×1440.
2. Resolution handling lives in the core: a module is written for one reference size and works at other client sizes without its own per-resolution code or fixtures.
3. A public GitHub repository, named **PathOfVision**, with CI that builds, tests and publishes a per-user NSIS installer on each release.
4. The app checks GitHub Releases for updates at start, and installs them on the next start or on "update now", never mid-game (parent design section 5/10).
5. A README a friend can follow alone (contents as in parent design section 10).
6. The Plan B2 carry-over list (section 6) is done.

**Non-goals**
- A Direct2D HUD. The WebView HUD stays (user decision 2026-11-04: ~380 MB working set with the main window closed is acceptable; no FPS impact seen in-game). It goes into the Plan.md backlog as a long-term optimization, to be picked up only when RAM becomes a problem.
- Renaming the local project folder (`PoE_AltarScanner`). Done much later, when everything is finished (user decision). Only the GitHub repo is named PathOfVision now.
- Real 4K screenshots. Nobody with a 4K monitor is available yet; 4K is supported by design and marked as untested (section 4.4).
- 32:9, other game languages, in-game UI scale options other than default, a code-signing certificate, anything else from the Phase 5 backlog.
- A second module.

**Anti-goals**
- A confident wrong verdict at a resolution that was not verified (parent anti-goal 1). Unknown or failed reads show nothing or "incomplete".
- A design where each new module needs its own screenshot sets for every resolution.
- An updater that installs or restarts while the player is in a map.

## 3. Constraints

- All constraints of the parent design section 3 stay (ToS ground rules, Windows 10/11, English client, privacy, MIT, tests never touch the user's `settings.json` or profiles).
- **Game art stays in the repo and the installer** (user decision 2026-11-04: GGG tolerates good community tools and the game is free; the parent design's "gate before a public release" is closed by this decision). The repository is public from the first push, including the existing git history.
- The user does not read code. The user takes screenshots when asked, following a written shot list.
- The internal identity stays `io.github.miicrop.pov` / `%APPDATA%\miicrop-pov` (parent design section 10), so a later rename only touches display name, titles and README.

## 4. Resolution support

### 4.1 Approach (decided)

PoE 1 lays out its UI relative to the client height. The core defines a **reference height of 1440 px** (the height all current templates, offsets and thresholds were made at) and a **scale factor `s = client_height / 1440`**. Modules work in reference coordinates; the core converts between reference and client coordinates and provides captured frames (or regions) at reference scale.

- The scale hypothesis is verified first, before code is built on it (parent design section 8): measure the altar icon and box sizes in the windowed 1080p and 1440p shots against 3440×1440. If the UI does not scale linearly with height, fall back to a small multi-scale search, and tell the user.
- Width does not change the scale for 16:9 and wider; wider clients only have more room at the sides (3440×1440 and 2560×1440 have the same UI size).
- **Narrower than 16:9 (16:10, e.g. 1920×1200)** *(builder's call)*: common on 24" office monitors, so in scope. PoE may limit the UI by width there; the scale check covers it with a real 1920×1200 windowed shot. If the UI is width-limited, the factor becomes `min(h/1440, w/2560)`. Until that is verified, clients narrower than 16:9 count as "untested" (section 4.4), never as normal.
- The HUD size and position follow the same factor (on top of the user's HUD scale setting).

### 4.2 What the user provides (decided)

- **Windowed screenshots on the user's 3440×1440 monitor** at 1920×1080 (main set, hardest case), 1920×1200 (16:10, small set) and a mid height such as 2304×1296 (small set, a real point between 1080 and 1440). *(builder's call: a 2560×1440 window does not fit on a 1440 px tall screen with its title bar; 1440 px height is already covered by the 3440×1440 fixtures.)* The builder writes a short shot list (how to set the window size, how many altars, which variety: both altar colors, tall boxes, boxes near the screen edges). Screenshots stay local and git-ignored; labels go into git as today.
- No 4K screenshots for now.

### 4.3 Test layers (decided)

1. Existing 3440×1440 fixtures: must stay at the current result (prototype parity, 98.8 % of boxes).
2. **Synthetic downscales** of the 3440×1440 fixtures to 1080 px height, generated by a test tool (not stored). Cheap, catches most scaling bugs; not identical to real rendering, since the game re-renders text at each size.
3. **Real windowed shots** at 1080p and 1440p (section 4.2). 1080p is the hardest case (smallest text, OCR most likely to fail).

Acceptance per real resolution as in parent design section 8: all altars found, no false detections, ≥ 95 % of readable options fully correct.

**1080p is a hard gate for the first release** (user decision 2026-11-04: likely the most common setup among the friends). `v0.1.0` is not published until the real 1080p set passes. If 1080p OCR needs extra work, the plan includes an open-ended "improve 1080p reads" step; release preparation (README, CI, updater) can proceed in parallel, but publishing waits.

### 4.4 Unverified sizes (decided, from parent design section 8)

- Client heights inside the verified range (lowest to highest height with passing real fixtures) run normally.
- Heights above the verified range up to 2160 (4K) run, with a small "untested resolution" note in the HUD until someone confirms it. *(builder's call: downscaling to the reference is the easy direction, so 4K is not blocked; the note keeps it honest.)*
- Heights outside that: the game context reports "unsupported size"; the altar module shows no verdicts; Home explains why. A general setting "try anyway" opts in, with the "untested resolution" note.

### 4.5 For future modules

The shot list and the synthetic-downscale test tool are written so a future module can reuse them: per module, a few real shots at the smallest supported height plus its reference-size fixtures should be enough. This is documented in the core's module guide (or README for contributors).

## 5. Release

### 5.1 Repository (decided)

- The user creates a **public** GitHub repo named **PathOfVision** and pushes the existing history. The history contains `Altar.png` (an early 3440×1440 test screenshot, removed in `b3dfce4`); checked 2026-11-04: it shows no character or account name, only the league, realm and map, so it may stay.
- **The Python prototype (`prototype/`, its tests, `requirements.txt`) is removed before the first push** *(builder's call, following the parent design: "removed once parity is shown", which Plan A showed; its board no longer works)*. It stays available in git history. The Python dev tools in `tools/` stay. CI covers Rust only.
- **Local cleanup too** (user request 2026-11-04): everything the prototype leaves behind that is no longer needed goes, in the repo and in the local working folder: `prototype/` (incl. its assets, which now live under `ui/modules/altar/img/` and `modules/altar/assets/`), the Python tests that only test the prototype (keep the ones for `tools/gen_mods`, labels and data files), prototype-only dependencies in `requirements.txt` (keep only what `tools/` needs), stale local caches (`.pytest_cache`, `__pycache__`), and the `.venv` rebuilt with the slim requirements. Docs and Plan.md stop describing the prototype as runnable. Before deleting anything untracked, the builder lists it for the user. The user's `settings.json`, profiles and screenshots are never deleted.
- The builder prepares everything that has to be in place before the first push (README, LICENSE, `.gitignore` check that no screenshots, settings or personal profiles are tracked) and tells the user the exact steps.

### 5.2 Build and CI (decided)

- GitHub Actions on a Windows runner: build and unit tests on every push; on a version tag, build the NSIS installer and the updater artifacts and attach them to a GitHub Release with a changelog.
- Fixture-based tests run locally only (screenshots are not in git).
- Per-user install, no admin rights, no extras in the installer.
- **Who releases** *(builder's call)*: the builder bumps the version and pushes the tag when the user says "release"; CI creates a **draft** release; the user reads it and clicks "Publish". Only published releases reach friends' updaters.
- **Plan C ends with a real published release** (`v0.1.0`) installed the way a friend would, and a tested update to a second release (`v0.1.1`) that shows "update available", installs on the next start and keeps settings and profiles.

### 5.3 Updates (decided)

- Tauri updater with a signing key, feed from GitHub Releases (`latest.json`).
- Check in the background at start; Home shows "update available"; install on next start or on "update now". Never during a map.
- The updater signing key's private half is kept out of the repo (GitHub secret plus a backup the user stores safely). The builder explains to the user what to store where.
- The update check is the only network traffic (parent constraint); the README says so.

### 5.4 README (decided)

As in parent design section 10: what the app does, what it never does (no memory reading, no input to the game, no data leaves the PC), supported setups and resolutions (with 4K marked untested), install, the SmartScreen warning on first start and why, privacy, license, how to report a bad read.

## 6. Carry-over from Plan B2 (decided: all in scope)

From Plan.md Phase 4 "Carry-over into Plan C":
- Back-off after a failed capture device reset (no reset storm).
- A module registry instead of match arms in `settings.rs`.
- Windows reserved profile names (`con.json` and the like) get a suffix in `profile_file_name`.
- Declare serde_json `preserve_order` in pov-core itself.
- Settings window: save Number/Path fields on Enter/input, flush pending changes on `pagehide`, guard against undefined values.
- URL-encode the section in `open_settings`.
- Autostart leaves a `StartupApproved\Run` entry behind: clean it up in the uninstaller.
- Ship only `default.json` / `empty.json` as shipped profiles.
- Tests for the `hud_altar` recommendation/dimming and for `hud_fit`.
- Cache the `color_keywords` regex.
- ~~Decide WebView HUD vs. Direct2D HUD~~ decided: WebView stays (section 2).

## 7. Decided by the user (this conversation)

- Target players: friends, mostly IT people, standard 24"/27" monitors, 1080p to 4K.
- 4K without real screenshots for now.
- 1080p must pass before the first release is published.
- Resolution handling once in the core so future modules avoid per-resolution screenshot work.
- Public GitHub repo now, named PathOfVision; game art is fine to publish.
- Local folder rename postponed until everything is finished.
- WebView HUD stays; Direct2D noted as a long-term optimization.
- The summary of Plan C (resolutions in the core, three test layers, unsupported-size handling, CI + installer + updater + README, carry-over list) was confirmed.

## 8. Left to the builder

- Where exactly the reference-scale conversion sits (capture, game context or a shared helper) and whether frames are rescaled whole or per region. Prefer not to resample text twice: OCR crops should be taken from the native frame and scaled once to the OCR's working size.
- Resampling filter choice, how the synthetic-downscale tool is invoked, the exact shot list wording.
- Versioning scheme and changelog format (start from the current `0.1.0`).
- CI workflow details, caching, the updater endpoint format.
- Order of the Plan C steps, except that the scale hypothesis check comes before building on it and the user's screenshots are requested early so they are ready when needed.
