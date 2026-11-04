//! Update check against GitHub Releases. Installs by itself only while Path of Exile is closed;
//! otherwise Home offers the update and installs on the user's click.

use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_updater::UpdaterExt;

/// What to do with the result of the check at app start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartAction {
    Install,
    Offer,
    Nothing,
}

pub fn start_action(update_available: bool, poe_running: bool) -> StartAction {
    match (update_available, poe_running) {
        (true, false) => StartAction::Install,
        (true, true) => StartAction::Offer,
        (false, _) => StartAction::Nothing,
    }
}

/// The update offered to Home. Kept so a Home window opened later still shows it.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct UpdateOffer {
    pub version: String,
    pub notes: String,
}

#[derive(Default)]
pub struct PendingUpdate(pub Mutex<Option<UpdateOffer>>);

const CHECK_TIMEOUT: Duration = Duration::from_secs(15);

async fn check(app: &AppHandle) -> tauri_plugin_updater::Result<Option<tauri_plugin_updater::Update>> {
    app.updater_builder().timeout(CHECK_TIMEOUT).build()?.check().await
}

async fn install(update: &tauri_plugin_updater::Update) -> tauri_plugin_updater::Result<()> {
    update.download_and_install(|_, _| {}, || {}).await
}

/// Checks once at start. Install: download, install and restart (never returns on success).
/// Errors only go to stderr. Runs before the scanner starts; the caller starts it afterwards.
pub async fn run_at_start(app: &AppHandle) {
    let update = match check(app).await {
        Ok(u) => u,
        Err(e) => {
            eprintln!("update check: {e}");
            return;
        }
    };
    let poe_running = pov_game::find_poe_window().is_some();
    let Some(update) = update else { return };
    match start_action(true, poe_running) {
        StartAction::Install => {
            match install(&update).await {
                Ok(()) => app.restart(),
                Err(e) => eprintln!("update install: {e}"),
            }
        }
        StartAction::Offer => {
            let offer = UpdateOffer { version: update.version.clone(), notes: update.body.clone().unwrap_or_default() };
            if let Some(pending) = tauri::Manager::try_state::<PendingUpdate>(app) {
                *crate::state::lock(&pending.0) = Some(offer.clone());
            }
            let _ = app.emit("app:update", offer);
        }
        StartAction::Nothing => {}
    }
}

/// The offer waiting for the user, if any.
#[tauri::command]
pub fn update_offer(pending: State<'_, PendingUpdate>) -> Option<UpdateOffer> {
    crate::state::lock(&pending.0).clone()
}

/// Downloads and installs the update, then restarts the app.
#[tauri::command]
pub async fn update_now(app: AppHandle) -> Result<(), String> {
    let update = check(&app).await.map_err(|e| e.to_string())?.ok_or("no update available")?;
    install(&update).await.map_err(|e| e.to_string())?;
    app.restart()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_install_only_when_poe_is_closed() {
        assert_eq!(start_action(true, false), StartAction::Install);
        assert_eq!(start_action(true, true), StartAction::Offer);
        assert_eq!(start_action(false, false), StartAction::Nothing);
        assert_eq!(start_action(false, true), StartAction::Nothing);
    }
}
