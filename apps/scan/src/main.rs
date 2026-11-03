//! `pov-scan`: headless live scanner. Prints the altar verdicts to the console.
//!
//! Live mode reads the PoE window and game log; `--replay <dir>` cycles through screenshots.
//! Passive only: it never sends input to the game.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, bail};
use altar::module::AltarModule;
use pov_capture::{FrameSource, ReplaySource, WindowCapture};
use pov_core::stats::{Reporter, SharedScans, Timed};
use pov_core::{AppPaths, Module, ModuleOutput, Runner, SettingsStore, import_prototype};
use pov_game::{
    Area, AreaKind, ContextSource, FixedContext, GameContext, LiveContext, PoeWindow, Rect, enable_dpi_awareness,
};
use windows::Win32::Foundation::{FALSE, TRUE};
use windows::Win32::System::Console::SetConsoleCtrlHandler;
use windows::core::BOOL;

const STATS_EVERY: Duration = Duration::from_secs(10);
const USAGE: &str = "usage: pov-scan [--replay <dir>] [--hold <secs>] [--stats] [--dev] [--import-prototype]";

static STOP: AtomicBool = AtomicBool::new(false);

unsafe extern "system" fn on_ctrl(_kind: u32) -> BOOL {
    // Ctrl+C, Ctrl+Break, console close: ask the loop to stop.
    if STOP.swap(true, Ordering::SeqCst) {
        FALSE // second press: let Windows terminate the process
    } else {
        TRUE
    }
}

#[derive(Debug, Default)]
struct Args {
    replay: Option<PathBuf>,
    hold: f64,
    stats: bool,
    import: bool,
    dev: bool,
}

fn parse_args() -> anyhow::Result<Args> {
    let mut a = Args { hold: 3.0, dev: cfg!(debug_assertions), ..Default::default() };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--replay" => a.replay = Some(it.next().context("--replay needs a folder")?.into()),
            "--hold" => {
                a.hold = it.next().context("--hold needs seconds")?.parse().context("--hold needs a number")?;
                if !(a.hold.is_finite() && a.hold > 0.0) {
                    bail!("--hold must be positive");
                }
            }
            "--stats" => a.stats = true,
            "--import-prototype" => a.import = true,
            "--dev" => a.dev = true,
            "-h" | "--help" => {
                println!("{USAGE}");
                std::process::exit(0);
            }
            other => bail!("unknown argument {other}\n{USAGE}"),
        }
    }
    Ok(a)
}

fn app_paths(dev: bool) -> anyhow::Result<AppPaths> {
    if dev {
        return Ok(AppPaths::dev());
    }
    let p = AppPaths::installed()?;
    if p.data.join("mods.json").is_file() {
        return Ok(p);
    }
    // A release build run from `target/` has no data folder next to it.
    let dev = AppPaths::dev();
    eprintln!("no data folder at {}, using {}", p.data.display(), dev.data.display());
    Ok(dev)
}

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

/// A foreground PoE window the size of the first image, in a map.
fn replay_context(first: &Path) -> anyhow::Result<GameContext> {
    let img = vision::Rgb::load(first).with_context(|| format!("loading {}", first.display()))?;
    let window = PoeWindow {
        hwnd: 0,
        pid: 0,
        exe: PathBuf::from("replay"),
        client: Rect { x: 0, y: 0, w: img.w as i32, h: img.h as i32 },
        foreground: true,
        minimized: false,
    };
    Ok(GameContext { poe: Some(window), area: Some(Area { id: "replay".into(), level: 0, kind: AreaKind::Map }) })
}

/// Seconds since start, for log lines.
fn clock() -> String {
    static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    format!("{:7.1}s", START.get_or_init(Instant::now).elapsed().as_secs_f64())
}

fn print_changes(last: &mut HashMap<&'static str, Vec<String>>, outputs: &[ModuleOutput]) {
    for out in outputs {
        let prev = last.entry(out.module).or_default();
        if *prev == out.lines {
            continue;
        }
        if out.lines.is_empty() {
            println!("[{}] {}: no altars in view", clock(), out.module);
        }
        for line in &out.lines {
            println!("[{}] {line}", clock());
        }
        *prev = out.lines.clone();
    }
}

fn main() -> anyhow::Result<()> {
    clock();
    let args = parse_args()?;
    let paths = app_paths(args.dev)?;

    if args.import {
        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..");
        let report = import_prototype(&paths, &repo)?;
        println!("imported profiles: {}", if report.profiles.is_empty() { "none".into() } else { report.profiles.join(", ") });
        println!("imported settings: {}", if report.settings { "yes" } else { "no (already present or no prototype settings)" });
        println!("user folder: {}", paths.user.display());
        return Ok(());
    }

    // Read only: a missing settings file means defaults in memory; nothing is written.
    let settings = Arc::new(Mutex::new(SettingsStore::open(&paths)));
    let altar = AltarModule::new(&paths, settings.clone())?;
    let scans = SharedScans::default();
    let module: Box<dyn Module> = Box::new(Timed { inner: Box::new(altar), scans: scans.clone() });

    let (context, frames): (Box<dyn ContextSource>, Box<dyn FrameSource>) = match &args.replay {
        Some(dir) => {
            let images = replay_images(dir)?;
            println!("replay: {} images from {}, {} s each", images.len(), dir.display(), args.hold);
            let ctx = replay_context(&images[0])?;
            (Box::new(FixedContext(ctx)), Box::new(ReplaySource::new(images, Duration::from_secs_f64(args.hold))?))
        }
        None => {
            // Before any window geometry is read: the capture crop needs physical pixels.
            enable_dpi_awareness();
            let log = settings.lock().unwrap_or_else(|e| e.into_inner()).get().general.client_log.clone();
            (Box::new(LiveContext::new(log)), Box::new(WindowCapture::new()?))
        }
    };

    // SAFETY: the handler only touches an atomic.
    unsafe { SetConsoleCtrlHandler(Some(on_ctrl), true) }.context("installing the Ctrl+C handler")?;

    let reporter = args.stats.then(|| {
        let mut rep = Reporter::new(scans.clone());
        std::thread::spawn(move || {
            let mut next = Instant::now() + STATS_EVERY;
            while !STOP.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(100));
                if Instant::now() >= next {
                    println!("[{}] {}", clock(), rep.line());
                    next += STATS_EVERY;
                }
            }
        })
    });

    println!("pov-scan running (data {}), Ctrl+C to stop", paths.data.display());
    let mut runner = Runner::new(vec![module], context, frames);
    let mut last = HashMap::new();
    let mut last_status = String::new();
    runner.run(&STOP, &mut |tick| {
        if tick.status.text != last_status {
            println!("[{}] {}", clock(), tick.status.text);
            last_status = tick.status.text.clone();
        }
        print_changes(&mut last, &tick.outputs);
    });
    if let Some(h) = reporter {
        let _ = h.join();
    }
    println!("[{}] stopped", clock());
    Ok(())
}
