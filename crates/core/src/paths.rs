use std::path::PathBuf;

use anyhow::{Context, Result};

pub const APP_ID: &str = "io.github.miicrop.pov";
pub const USER_DIR_NAME: &str = "miicrop-pov";

#[derive(Debug, Clone)]
pub struct AppPaths {
    /// Shipped data, read-only.
    pub data: PathBuf,
    /// Per-user data (`%APPDATA%\miicrop-pov`).
    pub user: PathBuf,
}

fn user_dir() -> PathBuf {
    let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    base.join(USER_DIR_NAME)
}

impl AppPaths {
    pub fn installed() -> Result<AppPaths> {
        let exe = std::env::current_exe().context("cannot locate the executable")?;
        let dir = exe.parent().context("executable has no parent folder")?;
        Ok(AppPaths { data: dir.join("data"), user: user_dir() })
    }

    pub fn dev() -> AppPaths {
        let data = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("data");
        AppPaths { data, user: user_dir() }
    }

    pub fn with(data: impl Into<PathBuf>, user: impl Into<PathBuf>) -> AppPaths {
        AppPaths { data: data.into(), user: user.into() }
    }

    pub fn settings_file(&self) -> PathBuf {
        self.user.join("settings.json")
    }

    pub fn profiles_dir(&self, module: &str) -> PathBuf {
        self.user.join("profiles").join(module)
    }

    pub fn shipped_profiles_dir(&self, _module: &str) -> PathBuf {
        self.data.join("profiles")
    }
}
