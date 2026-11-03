//! Shared app plumbing: paths, settings and profiles.

pub mod paths;
pub mod profiles;
pub mod settings;

pub use paths::{APP_ID, AppPaths, USER_DIR_NAME};
pub use profiles::{ImportReport, active_profile_path, import_prototype};
pub use settings::{AltarSettings, General, Settings, SettingsStore, clean_altar};
