# Backlog

Open topics and ideas. Nothing here is committed; the project's current state is in [project.md](project.md).

## Next small things

- Nicer "Update now" button on Home.
- Show the app version on Home.
- Check why the update offer was not shown once on the first start with PoE running (it worked on the next try).

## Altar module

- **More screenshots:** the 1080 set has 28 altars (40 wanted); 1920x1200 (9) and 2560x1353 (4) are small. Misreads reported by players (screenshot + what the HUD showed) become test cases.
- **Known misreads:** a life bar hiding a word can pick the wrong card (1080 `screen_23` top, "Unique" hidden); a Rarity line behind the flask bar (1440 `screen_10`); OCR drops words on the bright pillarboxed hard test.
- **Resolutions:** real 4K shots; in-game UI scale options other than default; very wide pillarboxed windows (hard test 7/8); real windowed shots on a 1080 monitor (~1000 px client, only checked on downscaled shots).
- **Partial reads:** show "incomplete" in the HUD when a box is cut off by HUD or screen edges, instead of a verdict on half the mods.
- **Target from the icon halves:** classify crown/figure/group in the icon to know a box's target when its header is hidden.
- **Exarch atlas passive** (reportedly a 50 % chance for an additional upside) is untested: collect screenshots with it.
- **HardMode variants:** when do they spawn? Not Ruthless-only (seen in normal maps).
- **Profile import/export** as a shareable text code (like PoB codes).
- **In-app mod data updates:** download the newest `mods.json`; new cards show up as "new" on the board.
- **Profile hotkey switching** (e.g. per farm strategy).
- **Roll value scaling:** a high roll scores better than a low one within the same tier (needs the player's atlas scaling).
- **Configurable thresholds** per profile, **combination rules** ("downside X only matters together with upside Y").
- **Accessibility:** colorblind-safe verdict colors.
- **Additional game languages** (per-language mod data and OCR).

## App

- **Direct2D HUD:** only if RAM becomes a problem (WebView HUD ~380 MB working set with the main window closed).
- Lower the scan rate (e.g. 2 Hz) while no altar is on screen, if CPU ever matters (idle today ~1 %).
- Final app name ("Path of Vision" is the working name); renaming the local folder `PoE_AltarScanner` later.

## More modules (ideas)

- **Atlas tree overlay:** import a planned atlas tree and show in-game where to go next.
- **Passive tree overlay:** the same for the character tree, from Path of Building.
- **Travel wheel:** radial menu at the mouse to travel via `/` commands. Conflicts with "never send input to the game" (see below).
- **Stash prices:** overlay on stash tabs with poe.ninja prices.
- **Faustus trade helper:** prices and good ratios.
- **Nameless Ritual alert.**
- **Betrayal helper:** recommendations for the 5-5-2-2 strategy.
- **Crafting help:** no concrete idea yet.

## Open questions

- **Chat commands and pasting** (travel wheel, regex strings) send input to the game, which the ground rules rule out today. Many popular tools do it (e.g. Awakened PoE Trade); check GGG's stance before building anything.
