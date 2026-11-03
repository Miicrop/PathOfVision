use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::{Context, Result};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Value};

use crate::paths::AppPaths;

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct General {
    pub autostart: bool,
    pub client_log: Option<PathBuf>,
    pub try_unsupported_size: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AltarSettings {
    pub enabled: bool,
    pub profile: String,
    pub hud_bottom: i64,
    pub hud_offset_x: i64,
    pub hud_font_size: i64,
    pub hud_tiers: String,
    pub hud_other_opacity: f64,
    pub hud_values: bool,
    pub board_cards: String,
    pub board_target_tag: bool,
}

impl Default for AltarSettings {
    fn default() -> Self {
        AltarSettings {
            enabled: true,
            profile: "default.json".into(),
            hud_bottom: 60,
            hud_offset_x: 0,
            hud_font_size: 15,
            hud_tiers: "highlight".into(),
            hud_other_opacity: 0.6,
            hud_values: true,
            board_cards: "short".into(),
            board_target_tag: true,
        }
    }
}

fn int_of(v: &Value) -> Option<i64> {
    v.as_i64().or_else(|| v.as_f64().filter(|f| f.is_finite()).map(|f| f as i64))
}

impl AltarSettings {
    /// Reads known keys from a JSON object; a missing or wrongly typed key keeps its default.
    pub fn from_map(m: &Map<String, Value>) -> AltarSettings {
        let mut s = AltarSettings::default();
        let b = |k: &str, d: bool| m.get(k).and_then(Value::as_bool).unwrap_or(d);
        let i = |k: &str, d: i64| m.get(k).and_then(int_of).unwrap_or(d);
        let t = |k: &str, d: &str| m.get(k).and_then(Value::as_str).unwrap_or(d).to_string();
        s.enabled = b("enabled", s.enabled);
        s.profile = t("profile", &s.profile);
        s.hud_bottom = i("hud_bottom", s.hud_bottom);
        s.hud_offset_x = i("hud_offset_x", s.hud_offset_x);
        s.hud_font_size = i("hud_font_size", s.hud_font_size);
        s.hud_tiers = t("hud_tiers", &s.hud_tiers);
        s.hud_other_opacity = m.get("hud_other_opacity").and_then(Value::as_f64).unwrap_or(s.hud_other_opacity);
        s.hud_values = b("hud_values", s.hud_values);
        s.board_cards = t("board_cards", &s.board_cards);
        s.board_target_tag = b("board_target_tag", s.board_target_tag);
        s
    }
}

impl<'de> Deserialize<'de> for AltarSettings {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let v = Value::deserialize(d)?;
        Ok(v.as_object().map(AltarSettings::from_map).unwrap_or_default())
    }
}

/// JSON shape: `{"general": {...}, "modules": {"altar": {...}}}`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Settings {
    pub general: General,
    pub altar: AltarSettings,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct Modules {
    altar: AltarSettings,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct Wire {
    general: General,
    modules: Modules,
}

impl Serialize for Settings {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        Wire { general: self.general.clone(), modules: Modules { altar: self.altar.clone() } }.serialize(s)
    }
}

impl<'de> Deserialize<'de> for Settings {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let w = Wire::deserialize(d)?;
        Ok(Settings { general: w.general, altar: w.modules.altar })
    }
}

/// Port of the prototype's `clean()`: coerce into the valid ranges, defaults for invalid values.
pub fn clean_altar(mut s: AltarSettings) -> AltarSettings {
    let d = AltarSettings::default();
    s.hud_font_size = s.hud_font_size.clamp(10, 24);
    s.hud_tiers = match s.hud_tiers.as_str() {
        "text" | "both" | "highlight" => "highlight".into(),
        "off" => "off".into(),
        _ => d.hud_tiers,
    };
    s.hud_other_opacity = if s.hud_other_opacity.is_finite() {
        (s.hud_other_opacity.clamp(0.1, 1.0) * 100.0).round() / 100.0
    } else {
        d.hud_other_opacity
    };
    s.hud_bottom = s.hud_bottom.clamp(0, 2000);
    s.hud_offset_x = s.hud_offset_x.clamp(-3000, 3000);
    if !matches!(s.board_cards.as_str(), "full" | "short" | "compact") {
        s.board_cards = d.board_cards;
    }
    s
}

fn load_file(path: &Path) -> Settings {
    let mut s = std::fs::read_to_string(path)
        .ok()
        .and_then(|t| serde_json::from_str::<Settings>(&t).ok())
        .unwrap_or_default();
    s.altar = clean_altar(s.altar);
    s
}

fn mtime_of(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

pub(crate) fn write_atomic(path: &Path, s: &Settings) -> Result<()> {
    let dir = path.parent().context("settings path has no parent")?;
    std::fs::create_dir_all(dir)?;
    let tmp = path.with_extension("json.tmp");
    let mut text = serde_json::to_string_pretty(s)?;
    text.push('\n');
    std::fs::write(&tmp, text).with_context(|| format!("writing {}", tmp.display()))?;
    std::fs::rename(&tmp, path).with_context(|| format!("replacing {}", path.display()))?;
    Ok(())
}

pub struct SettingsStore {
    path: PathBuf,
    settings: Settings,
    mtime: Option<SystemTime>,
}

impl SettingsStore {
    pub fn open(paths: &AppPaths) -> SettingsStore {
        let path = paths.settings_file();
        let mtime = mtime_of(&path);
        SettingsStore { settings: load_file(&path), path, mtime }
    }

    pub fn get(&self) -> &Settings {
        &self.settings
    }

    /// Reloads when the file's modification time changed; true if it did.
    pub fn reload_if_changed(&mut self) -> bool {
        let m = mtime_of(&self.path);
        if m == self.mtime {
            return false;
        }
        self.mtime = m;
        self.settings = load_file(&self.path);
        true
    }

    pub fn save(&self, s: &Settings) -> Result<()> {
        write_atomic(&self.path, s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn paths() -> (tempfile::TempDir, AppPaths) {
        let d = tempfile::tempdir().unwrap();
        let p = AppPaths::with(d.path().join("data"), d.path().join("user"));
        (d, p)
    }

    fn write(p: &AppPaths, text: &str) {
        std::fs::create_dir_all(&p.user).unwrap();
        std::fs::write(p.settings_file(), text).unwrap();
    }

    #[test]
    fn missing_settings_file_gives_defaults() {
        let (_d, p) = paths();
        assert_eq!(SettingsStore::open(&p).get(), &Settings::default());
    }

    #[test]
    fn invalid_settings_file_gives_defaults() {
        let (_d, p) = paths();
        write(&p, "{not json");
        assert_eq!(SettingsStore::open(&p).get(), &Settings::default());
    }

    #[test]
    fn partial_settings_keep_other_defaults() {
        let (_d, p) = paths();
        write(&p, r#"{"modules": {"altar": {"hud_font_size": 20}}}"#);
        let s = SettingsStore::open(&p);
        let mut want = Settings::default();
        want.altar.hud_font_size = 20;
        assert_eq!(s.get(), &want);
    }

    #[test]
    fn clean_altar_clamps_like_the_prototype() {
        let mut a = AltarSettings { hud_font_size: 99, ..Default::default() };
        assert_eq!(clean_altar(a.clone()).hud_font_size, 24);
        a.hud_tiers = "both".into();
        a.hud_other_opacity = 0.04;
        a.board_cards = "weird".into();
        let c = clean_altar(a.clone());
        assert_eq!((c.hud_tiers.as_str(), c.hud_other_opacity, c.board_cards.as_str()), ("highlight", 0.1, "short"));
        a.hud_other_opacity = 0.678;
        assert_eq!(clean_altar(a).hud_other_opacity, 0.68);
        let (_d, p) = paths();
        write(&p, r#"{"modules": {"altar": {"hud_font_size": "x"}}}"#);
        assert_eq!(SettingsStore::open(&p).get().altar.hud_font_size, 15);
    }

    #[test]
    fn save_then_reload_round_trips() {
        let (_d, p) = paths();
        let store = SettingsStore::open(&p);
        let mut s = Settings::default();
        s.altar.hud_bottom = 123;
        s.general.autostart = true;
        s.general.client_log = Some(PathBuf::from("C:/x/Client.txt"));
        store.save(&s).unwrap();
        assert_eq!(SettingsStore::open(&p).get(), &s);
        assert!(!p.user.join("settings.json.tmp").exists());
    }

    #[test]
    fn reload_if_changed_sees_external_edits() {
        let (_d, p) = paths();
        write(&p, "{}");
        let mut store = SettingsStore::open(&p);
        assert!(!store.reload_if_changed());
        write(&p, r#"{"modules": {"altar": {"hud_bottom": 99}}}"#);
        let f = std::fs::File::options().write(true).open(p.settings_file()).unwrap();
        f.set_modified(SystemTime::now() + Duration::from_secs(5)).unwrap();
        assert!(store.reload_if_changed());
        assert_eq!(store.get().altar.hud_bottom, 99);
        assert!(!store.reload_if_changed());
    }
}
