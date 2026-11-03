//! A game context that follows the game-log path from the settings file.
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use pov_game::{ContextSource, GameContext, LogOverride};

use crate::settings::SettingsStore;

/// Wraps a context source and forwards `general.client_log` changes (including hand edits of
/// the settings file) to it.
pub struct SettingsContext<C: ContextSource + LogOverride> {
    inner: C,
    settings: Arc<Mutex<SettingsStore>>,
    /// Last value passed on; `None` outer = nothing passed yet.
    last: Option<Option<PathBuf>>,
}

impl<C: ContextSource + LogOverride> SettingsContext<C> {
    pub fn new(inner: C, settings: Arc<Mutex<SettingsStore>>) -> Self {
        Self { inner, settings, last: None }
    }
}

impl<C: ContextSource + LogOverride> ContextSource for SettingsContext<C> {
    fn current(&mut self) -> GameContext {
        let log = {
            let mut s = self.settings.lock().unwrap_or_else(|e| e.into_inner());
            s.reload_if_changed();
            s.get().general.client_log.clone()
        };
        if self.last.as_ref() != Some(&log) {
            self.inner.set_log_override(log.clone());
            self.last = Some(log);
        }
        self.inner.current()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::AppPaths;

    struct Fake(Arc<Mutex<Vec<Option<PathBuf>>>>);

    impl ContextSource for Fake {
        fn current(&mut self) -> GameContext {
            GameContext::default()
        }
    }
    impl LogOverride for Fake {
        fn set_log_override(&mut self, path: Option<PathBuf>) {
            self.0.lock().unwrap().push(path);
        }
    }

    #[test]
    fn settings_context_forwards_log_path_changes() {
        let d = tempfile::tempdir().unwrap();
        let paths = AppPaths::with(d.path().join("data"), d.path().join("user"));
        std::fs::create_dir_all(&paths.user).unwrap();
        let store = Arc::new(Mutex::new(SettingsStore::open(&paths)));
        let calls = Arc::new(Mutex::new(Vec::new()));
        let mut ctx = SettingsContext::new(Fake(calls.clone()), store.clone());
        ctx.current();
        assert_eq!(*calls.lock().unwrap(), vec![None]);
        store.lock().unwrap().update("general", "client_log", serde_json::json!("X")).unwrap();
        ctx.current();
        assert_eq!(*calls.lock().unwrap(), vec![None, Some(PathBuf::from("X"))]);
        ctx.current();
        assert_eq!(calls.lock().unwrap().len(), 2);
    }
}
