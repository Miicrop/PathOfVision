# Shot list: test screenshots for more resolutions

Path of Vision needs a few real screenshots at other screen sizes to check that it reads altars correctly there. This page tells you what to capture.

Your screenshots stay on your computer. They are git-ignored, so they are never committed or uploaded anywhere.

## Setup

1. Start Path of Exile and open Options, then Graphics.
2. Set the display mode to **Windowed**.
3. Pick the resolution for the set you are shooting (see below).
4. Take screenshots the same way as before: your usual screenshot key, capturing the whole game window.

## Sets wanted

| Resolution | Folder | Screenshots | Notes |
|---|---|---|---|
| 1920x1080 | `tests/fixtures/screens_1920x1080/` | at least 25, with at least 40 altars in total | Main set. Needed before the first release. |
| 1920x1200 | `tests/fixtures/screens_1920x1200/` | at least 8 | |
| 2304x1296 | `tests/fixtures/screens_2304x1296/` | at least 8 | If the game does not offer 2304x1296, use the closest resolution it offers with a height between 1200 and 1400, and name the folder after it. |

Name the files `screen_01.png`, `screen_02.png`, and so on, inside each folder. Create the folder if it does not exist.

## What to capture in each set

Mix it up. Try to include:

- Both altar colors (the two kinds of Eldritch Altar).
- Tall boxes: 3 downsides and 2 upsides.
- Boxes near the top buff bar.
- Boxes near the screen edges.
- An altar standing next to another altar.
- Busy fights, with lots of effects on screen.

Optional: if you allocate the Exarch atlas passive, a few shots with an extra upside line help.
