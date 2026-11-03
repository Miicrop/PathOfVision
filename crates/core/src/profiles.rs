use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde_json::Value;

use crate::paths::AppPaths;
use crate::settings::{AltarSettings, Settings, clean_altar, write_atomic};

const MODULE: &str = "altar";
const SHIPPED: [&str; 2] = ["default.json", "empty.json"];

pub fn active_profile_path(paths: &AppPaths, s: &Settings) -> PathBuf {
    let name = &s.altar.profile;
    let shipped = paths.shipped_profiles_dir(MODULE);
    // A name with separators must not escape the profile folders.
    if Path::new(name).file_name().is_some_and(|f| f == name.as_str()) {
        let user = paths.profiles_dir(MODULE).join(name);
        if user.exists() {
            return user;
        }
        let ship = shipped.join(name);
        if ship.exists() {
            return ship;
        }
    }
    shipped.join("default.json")
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct ImportReport {
    pub profiles: Vec<String>,
    pub settings: bool,
}

pub fn import_prototype(paths: &AppPaths, repo: &Path) -> Result<ImportReport> {
    let mut report = ImportReport::default();
    let src = repo.join("data").join("profiles");
    let dst = paths.profiles_dir(MODULE);
    if src.is_dir() {
        let mut names: Vec<String> = std::fs::read_dir(&src)?
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".json") && !SHIPPED.contains(&n.as_str()))
            .collect();
        names.sort();
        for n in names {
            let target = dst.join(&n);
            if target.exists() {
                continue;
            }
            std::fs::create_dir_all(&dst)?;
            std::fs::copy(src.join(&n), &target).with_context(|| format!("copying {n}"))?;
            report.profiles.push(n);
        }
    }
    let target = paths.settings_file();
    let proto = repo.join("settings.json");
    if !target.exists() && proto.is_file() {
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&proto)?)
            .with_context(|| format!("parsing {}", proto.display()))?;
        let altar = v.as_object().map(AltarSettings::from_map).unwrap_or_default();
        let s = Settings { altar: clean_altar(altar), ..Default::default() };
        write_atomic(&target, &s)?;
        report.settings = true;
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (tempfile::TempDir, AppPaths) {
        let d = tempfile::tempdir().unwrap();
        let p = AppPaths::with(d.path().join("data"), d.path().join("user"));
        std::fs::create_dir_all(p.shipped_profiles_dir("altar")).unwrap();
        std::fs::write(p.shipped_profiles_dir("altar").join("default.json"), "{}").unwrap();
        std::fs::write(p.shipped_profiles_dir("altar").join("empty.json"), "{}").unwrap();
        (d, p)
    }

    fn with_profile(name: &str) -> Settings {
        let mut s = Settings::default();
        s.altar.profile = name.into();
        s
    }

    #[test]
    fn active_profile_prefers_user_dir() {
        let (_d, p) = setup();
        std::fs::create_dir_all(p.profiles_dir("altar")).unwrap();
        std::fs::write(p.profiles_dir("altar").join("empty.json"), "{}").unwrap();
        assert_eq!(active_profile_path(&p, &with_profile("empty.json")), p.profiles_dir("altar").join("empty.json"));
    }

    #[test]
    fn active_profile_falls_back_to_shipped() {
        let (_d, p) = setup();
        assert_eq!(active_profile_path(&p, &with_profile("empty.json")), p.shipped_profiles_dir("altar").join("empty.json"));
    }

    #[test]
    fn missing_profile_name_falls_back_to_default() {
        let (_d, p) = setup();
        let want = p.shipped_profiles_dir("altar").join("default.json");
        assert_eq!(active_profile_path(&p, &with_profile("nope.json")), want);
        assert_eq!(active_profile_path(&p, &with_profile("../x.json")), want);
    }

    #[test]
    fn import_prototype_copies_user_profiles_and_settings() {
        let (d, p) = setup();
        let repo = d.path().join("repo");
        let prof = repo.join("data").join("profiles");
        std::fs::create_dir_all(&prof).unwrap();
        for n in ["default", "empty", "maps-eater"] {
            std::fs::write(prof.join(format!("{n}.json")), "{}").unwrap();
        }
        std::fs::write(repo.join("settings.json"), r#"{"profile": "maps-eater.json", "hud_font_size": 15}"#).unwrap();

        let r = import_prototype(&p, &repo).unwrap();
        assert_eq!(r, ImportReport { profiles: vec!["maps-eater.json".into()], settings: true });
        assert!(p.profiles_dir("altar").join("maps-eater.json").exists());
        assert!(!p.profiles_dir("altar").join("default.json").exists());
        let text = std::fs::read_to_string(p.settings_file()).unwrap();
        let v: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(v["modules"]["altar"]["profile"], "maps-eater.json");

        let r2 = import_prototype(&p, &repo).unwrap();
        assert_eq!(r2, ImportReport::default());
        assert_eq!(std::fs::read_to_string(p.settings_file()).unwrap(), text);
    }
}
