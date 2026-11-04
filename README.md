# Path of Vision

A free, open-source, passive overlay for **Path of Exile 1**. It reads the options on an Eldritch Altar, recommends one according to your profile, and shows the result in a small HUD next to the altar.

## What it does

- Watches the game window and recognises Eldritch Altars on screen.
- Reads the mods of each option and rates them against a profile (what you want and what you want to avoid).
- Shows a HUD that highlights the recommended option and dims the others.

## What it never does

- It never reads the game's memory.
- It never sends input to the game (no clicks, no key presses).
- No data leaves your PC. The only network traffic is the update check against GitHub Releases (see Updates).

It only looks at the picture on your screen, like a screenshot would.

## Supported setups

- Windows 10 or 11, English game client, windowed or borderless.
- Resolution support (by client height, aspect ratio 16:10 up to about 21:9):

| Resolution | Status |
| --- | --- |
| 1080 to 1440 px height (verified on 1920x1080, 1920x1200, 2560x1353 and 3440x1440) | Verified (windowed clients down to 90 % of 1080 px count too; that case was checked on downscaled screenshots only) |
| 4K and heights up to 2160 px | Runs, with an "untested resolution" note in the HUD |
| Narrower than 16:10 (e.g. 4:3) | Runs, with an "untested resolution" note in the HUD |
| Wider than about 2.6:1 (e.g. a window stretched across an ultrawide at reduced height) | Runs, with an "untested resolution" note in the HUD |
| Smaller than the verified range | Unsupported, unless "Try unsupported resolutions" is enabled in Settings |

## Install

1. Download the latest installer (`.exe`) from the [Releases](https://github.com/Miicrop/PathOfVision/releases) page.
2. Run it. It installs for the current user only and needs no administrator rights.

**SmartScreen warning:** Windows may say "Windows protected your PC". Click "More info", then "Run anyway". The installer is unsigned because a code-signing certificate costs money for a free hobby project. The source code is in this repository, and releases are built by the workflow in `.github/workflows/release.yml`.

## Updates

The app checks GitHub Releases for a new version when it starts. If one exists, it installs it only while Path of Exile is closed; otherwise Home shows an "Update now" button. This check is the only network traffic the app produces.

## Privacy

Nothing is collected, uploaded or tracked. Settings and profiles are stored locally in `%APPDATA%\miicrop-pov`. Screenshots are processed in memory and not saved.

## Reporting a bad read

If the HUD shows a wrong value or misses an altar, open a [GitHub issue](https://github.com/Miicrop/PathOfVision/issues) with:

- a screenshot of the altar (full game window, please),
- what the HUD showed,
- your resolution and whether the game runs windowed or borderless.

## Development

Requires the Rust toolchain (stable) on Windows.

```
cargo build --workspace
cargo test --workspace
```

Tests that need real game screenshots read them from `tests/fixtures/screens` (3440x1440) and `tests/fixtures/screens_<size>/client` (other resolutions, cut out of desktop screenshots with `tools/crop_client.py`), which are git-ignored and exist only on the author's machine. Those tests skip themselves when the screenshots are missing; the slow accuracy tests are `#[ignore]` and run locally with:

```
cargo test -p altar --release --test golden -- --ignored
```

The app lives in `app/` (Tauri 2). To build the installer: `cargo install tauri-cli --version "^2"`, then `cargo tauri build` inside `app/`.

## License

MIT, see [LICENSE](LICENSE). Path of Vision is not affiliated with or endorsed by Grinding Gear Games.
