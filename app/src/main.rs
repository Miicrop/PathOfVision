//! `pov-app`: the Path of Vision desktop app. Main window (Home, board), tray icon and the
//! scanner thread. Passive only: it reads the screen and the game log, never sends input.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod scanner;
mod state;

use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::time::Duration;

use anyhow::{Context, bail};
use pov_core::AppPaths;
use tauri::image::Image;
use tauri::menu::{Menu, MenuEvent, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, RunEvent, WebviewUrl, WebviewWindowBuilder, WindowEvent};

use crate::scanner::Mode;
use crate::state::{AppState, ScannerThread, lock};

const TITLE: &str = "Path of Vision";
const MAIN_WINDOW: &str = "main";
const USAGE: &str = "usage: pov-app [--replay <dir>] [--hold <secs>] [--stats] [--dev]";
/// Share of the primary monitor's work area the main window covers.
const MAIN_SIZE: f64 = 0.9;

#[derive(Debug, Default)]
struct Args {
    replay: Option<PathBuf>,
    hold: f64,
    stats: bool,
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
    // A release build run from `target/` has no data folder next to it (until Plan C ships one).
    let dev = AppPaths::dev();
    eprintln!("no data folder at {}, using {}", p.data.display(), dev.data.display());
    Ok(dev)
}

/// Shows, restores and focuses the main window.
fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(MAIN_WINDOW) {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// The main window: Home, 90 % of the primary monitor's work area, centered on it.
fn create_main_window(app: &AppHandle) -> tauri::Result<()> {
    let mut builder = WebviewWindowBuilder::new(app, MAIN_WINDOW, WebviewUrl::App("shell/home.html".into()))
        .title(TITLE)
        .min_inner_size(640.0, 480.0);
    builder = match app.primary_monitor()? {
        Some(m) => {
            let (scale, area) = (m.scale_factor(), m.work_area());
            let (w, h) = (area.size.width as f64 * MAIN_SIZE, area.size.height as f64 * MAIN_SIZE);
            let x = area.position.x as f64 + (area.size.width as f64 - w) / 2.0;
            let y = area.position.y as f64 + (area.size.height as f64 - h) / 2.0;
            builder
                .inner_size(w / scale, h / scale)
                .position(x / scale, y / scale)
        }
        None => builder.inner_size(1280.0, 800.0).center(),
    };
    let window = builder.build()?;
    let w = window.clone();
    window.on_window_event(move |event| {
        // Closing the main window only hides it; the app lives on in the tray.
        if let WindowEvent::CloseRequested { api, .. } = event {
            api.prevent_close();
            let _ = w.hide();
        }
    });
    Ok(())
}

/// Stops the scanner thread, waits for it, then ends the app.
fn quit(app: &AppHandle) {
    app.state::<AppState>().stop.store(true, Ordering::SeqCst);
    if let Some(handle) = lock(&app.state::<ScannerThread>().0).take() {
        let _ = handle.join();
    }
    app.exit(0);
}

fn on_menu(app: &AppHandle, event: MenuEvent) {
    match event.id().as_ref() {
        "open" => show_main(app),
        "settings" => {
            if let Err(e) = commands::open_settings_window(app, None) {
                eprintln!("settings window: {e}");
            }
        }
        "quit" => quit(app),
        _ => {}
    }
}

fn create_tray(app: &AppHandle) -> tauri::Result<()> {
    let menu = Menu::with_items(
        app,
        &[
            &MenuItem::with_id(app, "open", "Open Path of Vision", true, None::<&str>)?,
            &MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?,
            &MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?,
        ],
    )?;
    TrayIconBuilder::with_id("tray")
        .icon(Image::from_bytes(include_bytes!("../icons/icon.png"))?)
        .tooltip(TITLE)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(on_menu)
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                show_main(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

fn main() {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e:#}");
            std::process::exit(2);
        }
    };
    let paths = match app_paths(args.dev) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{e:#}");
            std::process::exit(1);
        }
    };
    let mode = match args.replay {
        Some(dir) => Mode::Replay { dir, hold: Duration::from_secs_f64(args.hold) },
        None => Mode::Live,
    };
    let stats = args.stats;

    let app = tauri::Builder::default()
        // First, so a second start hands over before anything else is set up.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| show_main(app)))
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, None))
        .manage(AppState::new(paths))
        .manage(ScannerThread::default())
        .invoke_handler(tauri::generate_handler![
            commands::app_status,
            commands::open_settings,
            commands::settings_schema,
            commands::settings_get,
            commands::settings_set,
            commands::settings_reset,
        ])
        .setup(move |app| {
            let handle = app.handle().clone();
            create_main_window(&handle)?;
            create_tray(&handle)?;
            let thread = scanner::spawn(handle.clone(), &app.state::<AppState>(), mode, stats);
            *lock(&app.state::<ScannerThread>().0) = Some(thread);
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("building the app");

    app.run(|_app, event| {
        // Only "Quit" ends the app (with an exit code); closing the last window does not.
        if let RunEvent::ExitRequested { code: None, api, .. } = event {
            api.prevent_exit();
        }
    });
}
