use std::path::PathBuf;

use anyhow::{Context, Result};

pub const APP_ID: &str = "io.github.miicrop.pov";
pub const USER_DIR_NAME: &str = "miicrop-pov";

#[derive(Debug, Clone)]
pub struct AppPaths {
    /// Shipped data, read-only.
    pub data: PathBuf,
    /// Shipped module folders (`modules/<id>/assets`), read-only.
    pub modules: PathBuf,
    /// Per-user data (`%APPDATA%\miicrop-pov`).
    pub user: PathBuf,
}

fn user_dir() -> PathBuf {
    let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    base.join(USER_DIR_NAME)
}

impl AppPaths {
    fn repo_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
    }

    pub fn installed() -> Result<AppPaths> {
        let exe = std::env::current_exe().context("cannot locate the executable")?;
        let dir = exe.parent().context("executable has no parent folder")?;
        Ok(AppPaths::next_to(dir))
    }

    /// The installed layout: `data` and `modules` folders next to `dir` (the exe folder).
    pub fn next_to(dir: &std::path::Path) -> AppPaths {
        AppPaths { data: dir.join("data"), modules: dir.join("modules"), user: user_dir() }
    }

    pub fn dev() -> AppPaths {
        let root = Self::repo_root();
        AppPaths { data: root.join("data"), modules: root.join("modules"), user: user_dir() }
    }

    /// Tests: the repo's `modules/` folder.
    pub fn with(data: impl Into<PathBuf>, user: impl Into<PathBuf>) -> AppPaths {
        AppPaths::with_modules(data, Self::repo_root().join("modules"), user)
    }

    pub fn with_modules(data: impl Into<PathBuf>, modules: impl Into<PathBuf>, user: impl Into<PathBuf>) -> AppPaths {
        AppPaths { data: data.into(), modules: modules.into(), user: user.into() }
    }

    pub fn module_assets(&self, module: &str) -> PathBuf {
        self.modules.join(module).join("assets")
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installed_layout_is_next_to_the_exe() {
        let x = PathBuf::from("X");
        let p = AppPaths::next_to(&x);
        assert_eq!(p.data, x.join("data"));
        assert_eq!(p.modules, x.join("modules"));
        assert_eq!(p.module_assets("altar"), x.join("modules").join("altar").join("assets"));
    }
}
