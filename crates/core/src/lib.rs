//! Shared app plumbing: paths, settings and profiles.

pub mod module;
pub mod paths;
pub mod profiles;
pub mod runner;
pub mod schema;
pub mod settings;
pub mod stats;
pub mod status;

pub use paths::{APP_ID, AppPaths, USER_DIR_NAME};
pub use profiles::{ImportReport, active_profile_path, import_prototype};
pub use settings::{AltarSettings, General, Settings, SettingsStore, clean_altar};
pub use module::{Module, ModuleOutput};
pub use runner::{Runner, Tick};
pub use schema::settings_schema;
pub use status::{PoeState, RunnerStatus};
