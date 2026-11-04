//! IPC commands of the pages. Errors reach the page as strings, which it shows.
use pov_core::schema::SettingsSection;
use pov_core::{RunnerStatus, Settings};
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_autostart::ManagerExt;

use crate::state::{AppState, lock};

pub const SETTINGS_WINDOW: &str = "settings";
pub const HUD_WINDOW: &str = "hud";

/// The HUD window's handle, if it exists.
pub fn hud_hwnd(app: &AppHandle) -> Option<isize> {
    let w = app.get_webview_window(HUD_WINDOW)?;
    w.hwnd().ok().map(|h| h.0 as isize)
}

/// Hides the HUD. Never use Tauri's `show()`/`hide()` for it: `show()` activates the window.
pub fn hide_hud(app: &AppHandle) {
    if let Some(hwnd) = hud_hwnd(app) {
        let _ = pov_overlay::hide(hwnd);
    }
}

/// The HUD page measured itself (physical pixels): show it bottom-center of the PoE client
/// area with the user's offsets, without activating it. Size 0 or no PoE window hides it.
/// Synchronous on purpose: it runs on the main thread, which owns the window.
#[tauri::command]
pub fn hud_fit(app: AppHandle, state: State<'_, AppState>, width: u32, height: u32) -> Result<(), String> {
    let Some(hwnd) = hud_hwnd(&app) else { return Ok(()) };
    let client = lock(&state.poe).as_ref().map(|p| p.client);
    let (w, h) = (width.min(i32::MAX as u32) as i32, height.min(i32::MAX as u32) as i32);
    match client {
        Some(client) if w > 0 && h > 0 => {
            let (offset_x, bottom) = {
                let mut store = lock(&state.settings);
                store.reload_if_changed();
                let altar = &store.get().altar;
                (altar.hud_offset_x, altar.hud_bottom)
            };
            pov_overlay::show_at(hwnd, pov_overlay::hud_rect(client, (w, h), offset_x, bottom))
        }
        _ => pov_overlay::hide(hwnd),
    }
    .map_err(|e| format!("{e:#}"))
}

#[tauri::command]
pub fn app_status(state: State<'_, AppState>) -> RunnerStatus {
    lock(&state.status).clone()
}

/// Opens the settings window (or focuses it) on `section` (`general` or a module id).
/// Async: creating a window from a synchronous command deadlocks on Windows.
#[tauri::command]
pub async fn open_settings(app: AppHandle, section: Option<String>) -> Result<(), String> {
    open_settings_window(&app, section.as_deref()).map_err(|e| format!("{e:#}"))
}

pub fn open_settings_window(app: &AppHandle, section: Option<&str>) -> tauri::Result<()> {
    if let Some(w) = app.get_webview_window(SETTINGS_WINDOW) {
        if let Some(s) = section {
            // The page switches to that section (`settings.js`).
            app.emit_to(SETTINGS_WINDOW, "settings:section", s)?;
        }
        w.unminimize()?;
        w.show()?;
        return w.set_focus();
    }
    let mut url = "shell/settings.html".to_string();
    if let Some(s) = section {
        url = format!("{url}?section={s}");
    }
    WebviewWindowBuilder::new(app, SETTINGS_WINDOW, WebviewUrl::App(url.into()))
        .title("Path of Vision \u{2013} Settings")
        .inner_size(900.0, 700.0)
        .center()
        .build()?;
    Ok(())
}

#[tauri::command]
pub fn settings_schema() -> Vec<SettingsSection> {
    pov_core::settings_schema()
}

#[tauri::command]
pub fn settings_get(state: State<'_, AppState>) -> Value {
    let mut store = lock(&state.settings);
    store.reload_if_changed();
    serde_json::to_value(store.get()).unwrap_or_default()
}

#[tauri::command]
pub fn settings_set(app: AppHandle, state: State<'_, AppState>, section: String, key: String, value: Value) -> Result<Value, String> {
    change_settings(&app, &state, |store| store.update(&section, &key, value))
}

#[tauri::command]
pub fn settings_reset(app: AppHandle, state: State<'_, AppState>, section: String, keys: Vec<String>) -> Result<Value, String> {
    let keys: Vec<&str> = keys.iter().map(String::as_str).collect();
    change_settings(&app, &state, |store| store.reset(&section, &keys))
}

/// Applies a settings change, follows `general.autostart` and tells every window.
fn change_settings(
    app: &AppHandle,
    state: &AppState,
    change: impl FnOnce(&mut pov_core::SettingsStore) -> anyhow::Result<Settings>,
) -> Result<Value, String> {
    let mut store = lock(&state.settings);
    store.reload_if_changed();
    let before = store.get().general.autostart;
    let mut settings = change(&mut store).map_err(|e| format!("{e:#}"))?;
    let mut error = None;
    if settings.general.autostart != before {
        if let Err(e) = set_autostart(app, settings.general.autostart) {
            // Keep the file in line with what Windows actually does.
            error = Some(format!("Start with Windows could not be changed: {e}"));
            settings = store.update("general", "autostart", Value::Bool(before)).map_err(|e| format!("{e:#}"))?;
        }
    }
    drop(store);
    let json = serde_json::to_value(&settings).map_err(|e| e.to_string())?;
    let _ = app.emit("settings:changed", json.clone());
    match error {
        Some(e) => Err(e),
        None => Ok(json),
    }
}

fn set_autostart(app: &AppHandle, on: bool) -> Result<(), String> {
    let launch = app.autolaunch();
    if on { launch.enable() } else { launch.disable() }.map_err(|e| e.to_string())
}

// Tier board: 1:1 wrappers of `altar::board::BoardApi`. Async so the file I/O runs off the
// main thread.

fn board_result(r: anyhow::Result<Value>) -> Result<Value, String> {
    r.map_err(|e| format!("{e:#}"))
}

#[tauri::command]
pub async fn altar_get_state(state: State<'_, AppState>) -> Result<Value, String> {
    board_result(state.board.get_state())
}

#[tauri::command]
pub async fn altar_load_profile(state: State<'_, AppState>, file: String) -> Result<Value, String> {
    board_result(state.board.load_profile(&file))
}

#[tauri::command]
pub async fn altar_save_profile(state: State<'_, AppState>, file: String, profile: Value) -> Result<Value, String> {
    board_result(state.board.save_profile(&file, profile))
}

/// Duplicating makes the copy the active profile, so the other windows hear about it.
#[tauri::command]
pub async fn altar_duplicate_profile(app: AppHandle, state: State<'_, AppState>, file: String, name: String) -> Result<Value, String> {
    let result = board_result(state.board.duplicate_profile(&file, &name))?;
    emit_settings(&app, &state);
    Ok(result)
}

/// Deleting the active profile falls back to the default one.
#[tauri::command]
pub async fn altar_delete_profile(app: AppHandle, state: State<'_, AppState>, file: String) -> Result<Value, String> {
    let result = board_result(state.board.delete_profile(&file))?;
    emit_settings(&app, &state);
    Ok(result)
}

#[tauri::command]
pub async fn altar_set_active(app: AppHandle, state: State<'_, AppState>, file: String) -> Result<(), String> {
    state.board.set_active(&file).map_err(|e| format!("{e:#}"))?;
    emit_settings(&app, &state);
    Ok(())
}

/// Tells every window the current settings (the board changed `modules.altar.profile`).
fn emit_settings(app: &AppHandle, state: &AppState) {
    let json = serde_json::to_value(lock(&state.settings).get()).unwrap_or_default();
    let _ = app.emit("settings:changed", json);
}
