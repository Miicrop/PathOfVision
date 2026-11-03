//! The scanner thread: runs the B1 runner and turns its ticks into events for the pages.
//! Passive only: it reads the screen and the game log, it never sends input to the game.
use std::path::{Path, PathBuf};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use altar::module::AltarModule;
use anyhow::Context;
use pov_capture::{FrameSource, ReplaySource, WindowCapture};
use pov_core::stats::{Reporter, SharedScans, Timed};
use pov_core::{Module, Runner, RunnerStatus, SettingsContext, Tick};
use pov_game::{Area, AreaKind, ContextSource, FixedContext, GameContext, LiveContext, PoeWindow, Rect};
use serde_json::{Value, json};
use tauri::{AppHandle, Emitter};

use crate::state::{AppState, lock};

const ALTAR: &str = "altar";
const STATS_EVERY: Duration = Duration::from_secs(10);

pub enum Mode {
    /// The PoE window and the game log.
    Live,
    /// Screenshots from `dir`, each shown for `hold`, in a fake foreground PoE window in a map.
    Replay { dir: PathBuf, hold: Duration },
}

/// The `*.png` files of `dir`, sorted by name.
fn replay_images(dir: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let mut images: Vec<PathBuf> = std::fs::read_dir(dir)
        .with_context(|| format!("reading {}", dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("png")))
        .collect();
    images.sort();
    anyhow::ensure!(!images.is_empty(), "no *.png in {}", dir.display());
    Ok(images)
}

/// A foreground PoE window in a map covering the primary monitor's top-left corner at the size
/// of the first image, and the images as frames.
fn replay_sources(images: Vec<PathBuf>, hold: Duration) -> anyhow::Result<(Box<dyn ContextSource>, Box<dyn FrameSource>)> {
    let first = images.first().context("replay needs at least one image")?;
    let img = vision::Rgb::load(first).with_context(|| format!("loading {}", first.display()))?;
    let window = PoeWindow {
        hwnd: 0,
        pid: 0,
        exe: PathBuf::from("replay"),
        // The primary monitor's top-left corner is the screen origin.
        client: Rect { x: 0, y: 0, w: img.w as i32, h: img.h as i32 },
        foreground: true,
        minimized: false,
    };
    let ctx = GameContext { poe: Some(window), area: Some(Area { id: "replay".into(), level: 0, kind: AreaKind::Map }) };
    Ok((Box::new(FixedContext(ctx)), Box::new(ReplaySource::new(images, hold)?)))
}

/// The events one tick produces: `app:status` when the status changed, `altar:hud` with the
/// altar module's view whenever it ran, and an empty view once when it stops being active.
pub fn tick_events(tick: &Tick, last: &mut Option<RunnerStatus>, altar_was_active: &mut bool) -> Vec<(&'static str, Value)> {
    let mut events = Vec::new();
    if last.as_ref() != Some(&tick.status) {
        events.push(("app:status", serde_json::to_value(&tick.status).unwrap_or_default()));
        *last = Some(tick.status.clone());
    }
    let active = tick.status.active.iter().any(|m| m == ALTAR);
    if let Some(out) = tick.outputs.iter().find(|o| o.module == ALTAR) {
        events.push(("altar:hud", out.view.clone()));
    } else if *altar_was_active && !active {
        events.push(("altar:hud", json!({"altars": []})));
    }
    *altar_was_active = active;
    events
}

/// Window each event goes to.
fn target(event: &str) -> &'static str {
    if event == "altar:hud" { "hud" } else { "main" }
}

/// Starts the scanner thread. It runs until `state.stop` is set.
pub fn spawn(app: AppHandle, state: &AppState, mode: Mode, stats: bool) -> JoinHandle<()> {
    let paths = state.paths.clone();
    let settings = state.settings.clone();
    let status = state.status.clone();
    let poe = state.poe.clone();
    let stop = state.stop.clone();
    std::thread::Builder::new()
        .name("scanner".into())
        .spawn(move || {
            let scans = SharedScans::default();
            let built = (|| -> anyhow::Result<Runner> {
                // Capture objects are created here, on the thread that uses them.
                let (context, frames): (Box<dyn ContextSource>, Box<dyn FrameSource>) = match mode {
                    Mode::Live => (
                        Box::new(SettingsContext::new(LiveContext::new(None), settings.clone())),
                        Box::new(WindowCapture::new()?),
                    ),
                    Mode::Replay { dir, hold } => replay_sources(replay_images(&dir)?, hold)?,
                };
                let mut module: Box<dyn Module> = Box::new(AltarModule::new(&paths, settings)?);
                if stats {
                    module = Box::new(Timed { inner: module, scans: scans.clone() });
                }
                Ok(Runner::new(vec![module], context, frames))
            })();
            let mut runner = match built {
                Ok(r) => r,
                Err(e) => {
                    // Shown on Home instead of the runner status.
                    eprintln!("scanner: {e:#}");
                    let mut s = lock(&status).clone();
                    s.text = format!("scanner failed to start: {e:#}");
                    *lock(&status) = s.clone();
                    let _ = app.emit_to("main", "app:status", s);
                    return;
                }
            };

            let mut reporter = stats.then(|| (Reporter::new(scans), Instant::now() + STATS_EVERY));
            let (mut last, mut altar_was_active) = (None, false);
            runner.run(&stop, &mut |tick| {
                *lock(&status) = tick.status.clone();
                *lock(&poe) = tick.poe.clone();
                for (event, payload) in tick_events(tick, &mut last, &mut altar_was_active) {
                    // A window that does not exist (yet) simply gets nothing.
                    let _ = app.emit_to(target(event), event, payload);
                }
                if let Some((rep, next)) = reporter.as_mut()
                    && Instant::now() >= *next
                {
                    eprintln!("{}", rep.line());
                    *next += STATS_EVERY;
                }
            });
        })
        .expect("spawning the scanner thread")
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use pov_core::{AppPaths, PoeState, SettingsStore};

    use super::*;

    fn screens() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures/screens")
    }

    #[test]
    fn replay_mode_emits_status_and_hud_view() {
        let screen = screens().join("screen_02.png");
        if !screen.exists() {
            eprintln!("skipped: {} missing", screen.display());
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::with(altar::repo_data_dir(), dir.path().join("user"));
        let settings = Arc::new(Mutex::new(SettingsStore::open(&paths)));
        let altar: Box<dyn Module> = Box::new(AltarModule::new(&paths, settings).unwrap());
        let (context, frames) = replay_sources(vec![screen], Duration::from_secs(5)).unwrap();
        let mut runner = Runner::new(vec![altar], context, frames);

        let (mut last, mut was_active) = (None, false);
        let tick = runner.tick();
        let events = tick_events(&tick, &mut last, &mut was_active);
        let names: Vec<_> = events.iter().map(|(n, _)| *n).collect();
        assert_eq!(names, ["app:status", "altar:hud"]);
        assert_eq!(events[0].1["poe"], "foreground");
        assert_eq!(events[0].1["text"], "in map \u{2013} scanning");
        assert_eq!(events[1].1["altars"].as_array().map(Vec::len), Some(2));
        assert!(was_active);

        // Same status again: no `app:status`, the view is sent again.
        let names: Vec<_> = tick_events(&tick, &mut last, &mut was_active).into_iter().map(|(n, _)| n).collect();
        assert_eq!(names, ["altar:hud"]);

        // The altar module stops (e.g. PoE in background): the HUD is cleared once.
        let mut idle = tick.clone();
        idle.status.poe = PoeState::Background;
        idle.status.active.clear();
        idle.status.text = idle.status.describe();
        idle.outputs.clear();
        let events = tick_events(&idle, &mut last, &mut was_active);
        assert_eq!(events.len(), 2, "{events:?}");
        assert_eq!(events[0].0, "app:status");
        assert_eq!(events[0].1["poe"], "background");
        assert_eq!(events[1], ("altar:hud", json!({"altars": []})));
        assert!(tick_events(&idle, &mut last, &mut was_active).is_empty());
    }
}
