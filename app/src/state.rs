//! State shared by the commands, the tray and the scanner thread.
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use altar::board::BoardApi;
use pov_core::{AppPaths, RunnerStatus, SettingsStore};
use pov_game::{GameContext, PoeWindow};

pub struct AppState {
    pub paths: AppPaths,
    pub settings: Arc<Mutex<SettingsStore>>,
    /// Last status the scanner saw.
    pub status: Arc<Mutex<RunnerStatus>>,
    /// The PoE window of the last tick (for the HUD position).
    pub poe: Arc<Mutex<Option<PoeWindow>>>,
    /// Backend of the tier board page.
    pub board: BoardApi,
    /// Set on quit; the scanner thread ends on its next check.
    pub stop: Arc<AtomicBool>,
}

impl AppState {
    pub fn new(paths: AppPaths) -> AppState {
        let settings = Arc::new(Mutex::new(SettingsStore::open(&paths)));
        let board = BoardApi::new(paths.clone(), settings.clone());
        AppState {
            paths,
            settings,
            status: Arc::new(Mutex::new(RunnerStatus::from_context(&GameContext::default(), Vec::new(), false))),
            poe: Arc::new(Mutex::new(None)),
            board,
            stop: Arc::new(AtomicBool::new(false)),
        }
    }
}

/// The scanner thread, joined on quit.
#[derive(Default)]
pub struct ScannerThread(pub Mutex<Option<JoinHandle<()>>>);

/// Locks a mutex even if a panicking thread poisoned it: the data stays usable.
pub fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}
