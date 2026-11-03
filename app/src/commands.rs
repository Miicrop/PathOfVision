//! IPC commands of the pages. Errors reach the page as strings, which it shows.
use pov_core::schema::SettingsSection;
use pov_core::{RunnerStatus, Settings};
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_autostart::ManagerExt;

use crate::state::{AppState, lock};

pub const SETTINGS_WINDOW: &str = "settings";

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
        .inner_size(900.0, 640.0)
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
