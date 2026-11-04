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

/// Longest a download may take before the install is given up (the app stays on its version).
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(120);

/// A download that finished is installed only if PoE is still closed.
pub fn install_after_download(poe_running: bool) -> bool {
    !poe_running
}

fn poe_running() -> bool {
    pov_game::find_poe_window().is_some()
}

/// Downloads within `DOWNLOAD_TIMEOUT`; returns the installer bytes.
async fn download(update: &tauri_plugin_updater::Update) -> Result<Vec<u8>, String> {
    match tokio::time::timeout(DOWNLOAD_TIMEOUT, update.download(|_, _| {}, || {})).await {
        Ok(r) => r.map_err(|e| e.to_string()),
        Err(_) => Err("download timed out".into()),
    }
}

fn offer(app: &AppHandle, update: &tauri_plugin_updater::Update) {
    let offer = UpdateOffer { version: update.version.clone(), notes: update.body.clone().unwrap_or_default() };
    if let Some(pending) = tauri::Manager::try_state::<PendingUpdate>(app) {
        *crate::state::lock(&pending.0) = Some(offer.clone());
    }
    if let Err(e) = app.emit("app:update", offer) {
        eprintln!("update offer: {e}");
    }
}

/// Checks once at start; errors only go to stderr. `poe_running` is the state at app start:
/// if PoE is closed, the update is downloaded and, if PoE is still closed afterwards, installed
/// with a restart (never returns on success); otherwise Home gets an offer. With PoE closed the
/// caller starts the scanner only after this returns (bounded by check + download timeouts).
pub async fn run_at_start(app: &AppHandle, poe_running_at_start: bool) {
    let update = match check(app).await {
        Ok(Some(u)) => u,
        Ok(None) => return,
        Err(e) => {
            eprintln!("update check: {e}");
            return;
        }
    };
    match start_action(true, poe_running_at_start) {
        StartAction::Install => match download(&update).await {
            Ok(bytes) if install_after_download(poe_running()) => match update.install(bytes) {
                Ok(()) => app.restart(),
                Err(e) => eprintln!("update install: {e}"),
            },
            Ok(_) => offer(app, &update),
            Err(e) => eprintln!("update download: {e}"),
        },
        StartAction::Offer => offer(app, &update),
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
    let bytes = download(&update).await?;
    update.install(bytes).map_err(|e| e.to_string())?;
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

    #[test]
    fn install_is_dropped_when_poe_started_during_the_download() {
        assert!(install_after_download(false));
        assert!(!install_after_download(true));
    }
}
