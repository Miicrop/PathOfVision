use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::{Context, Result};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Value};

use crate::paths::AppPaths;

#[derive(Debug, Clone, PartialEq, Default, Serialize)]
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

#[derive(Serialize)]
struct Modules {
    altar: AltarSettings,
}

#[derive(Serialize)]
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
        Ok(Settings::from_value(&Value::deserialize(d)?))
    }
}

impl General {
    pub fn from_map(m: &Map<String, Value>) -> General {
        let mut g = General::default();
        g.autostart = m.get("autostart").and_then(Value::as_bool).unwrap_or(g.autostart);
        g.try_unsupported_size = m.get("try_unsupported_size").and_then(Value::as_bool).unwrap_or(g.try_unsupported_size);
        g.client_log = m.get("client_log").and_then(Value::as_str).filter(|p| !p.is_empty()).map(PathBuf::from);
        g
    }
}

impl Settings {
    /// Lenient: every level falls back to defaults when missing or of the wrong type.
    pub fn from_value(v: &Value) -> Settings {
        let obj = v.as_object();
        let general = obj.and_then(|o| o.get("general")).and_then(Value::as_object).map(General::from_map).unwrap_or_default();
        let altar = obj
            .and_then(|o| o.get("modules"))
            .and_then(Value::as_object)
            .and_then(|m| m.get("altar"))
            .and_then(Value::as_object)
            .map(AltarSettings::from_map)
            .unwrap_or_default();
        Settings { general, altar }
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

fn parse(text: &str) -> Option<(Value, Settings)> {
    let raw = serde_json::from_str::<Value>(text).ok()?;
    let mut s = Settings::from_value(&raw);
    s.altar = clean_altar(s.altar);
    Some((raw, s))
}

fn try_load(path: &Path) -> Option<(Value, Settings)> {
    parse(&std::fs::read_to_string(path).ok()?)
}

fn mtime_of(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

fn write_value_atomic(path: &Path, v: &Value) -> Result<()> {
    let dir = path.parent().context("settings path has no parent")?;
    std::fs::create_dir_all(dir)?;
    let tmp = path.with_extension("json.tmp");
    let mut text = serde_json::to_string_pretty(v)?;
    text.push('\n');
    std::fs::write(&tmp, text).with_context(|| format!("writing {}", tmp.display()))?;
    std::fs::rename(&tmp, path).with_context(|| format!("replacing {}", path.display()))?;
    Ok(())
}

pub(crate) fn write_atomic(path: &Path, s: &Settings) -> Result<()> {
    write_value_atomic(path, &serde_json::to_value(s)?)
}

fn as_object(v: &mut Value) -> &mut Map<String, Value> {
    if !v.is_object() {
        *v = Value::Object(Map::new());
    }
    v.as_object_mut().expect("just made an object")
}

/// The object stored for `section` (`general` or `modules.<id>`), created if missing or of the wrong type.
fn section_object<'a>(raw: &'a mut Value, section: &str) -> &'a mut Map<String, Value> {
    let root = as_object(raw);
    if section == "general" {
        as_object(root.entry("general").or_insert(Value::Null))
    } else {
        let modules = as_object(root.entry("modules").or_insert(Value::Null));
        as_object(modules.entry(section).or_insert(Value::Null))
    }
}

/// Default value of every key the typed settings know for `section`; `None` for an unknown section.
fn known_values(section: &str) -> Option<Map<String, Value>> {
    let v = match section {
        "general" => serde_json::to_value(General::default()),
        "altar" => serde_json::to_value(AltarSettings::default()),
        _ => return None,
    };
    v.ok()?.as_object().cloned()
}

fn section_value(s: &Settings, section: &str) -> Value {
    match section {
        "general" => serde_json::to_value(&s.general).unwrap_or_default(),
        _ => serde_json::to_value(&s.altar).unwrap_or_default(),
    }
}

pub struct SettingsStore {
    path: PathBuf,
    /// The file as read, including keys this version does not know.
    raw: Value,
    settings: Settings,
    mtime: Option<SystemTime>,
}

impl SettingsStore {
    pub fn open(paths: &AppPaths) -> SettingsStore {
        let path = paths.settings_file();
        let mtime = mtime_of(&path);
        let (raw, settings) = try_load(&path).unwrap_or_else(|| (Value::Object(Map::new()), Settings::default()));
        SettingsStore { raw, settings, path, mtime }
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
        // A file that exists but cannot be read or parsed right now (mid-write) keeps the old settings.
        match try_load(&self.path) {
            Some((raw, s)) => (self.raw, self.settings) = (raw, s),
            None if m.is_some() => return false,
            None => (self.raw, self.settings) = (Value::Object(Map::new()), Settings::default()),
        }
        true
    }

    /// Sets `section.key` (`section` = `general` or a module id), keeping every other key of the
    /// file, cleans the result, writes the file atomically and returns the new settings.
    pub fn update(&mut self, section: &str, key: &str, value: Value) -> Result<Settings> {
        self.apply(section, vec![(key.to_string(), value)])
    }

    /// Sets the given keys of `section` back to their defaults.
    pub fn reset(&mut self, section: &str, keys: &[&str]) -> Result<Settings> {
        let known = known_values(section).with_context(|| format!("unknown settings section {section:?}"))?;
        let mut changes = Vec::new();
        for k in keys {
            let d = known.get(*k).with_context(|| format!("unknown setting {section}.{k}"))?;
            changes.push((k.to_string(), d.clone()));
        }
        self.apply(section, changes)
    }

    fn apply(&mut self, section: &str, changes: Vec<(String, Value)>) -> Result<Settings> {
        let known = known_values(section).with_context(|| format!("unknown settings section {section:?}"))?;
        if let Some((k, _)) = changes.iter().find(|(k, _)| !known.contains_key(k)) {
            anyhow::bail!("unknown setting {section}.{k}");
        }
        let mut raw = self.raw.clone();
        let obj = section_object(&mut raw, section);
        for (k, v) in &changes {
            obj.insert(k.clone(), v.clone());
        }
        let mut s = Settings::from_value(&raw);
        s.altar = clean_altar(s.altar);
        // Write the cleaned values back, so the file never holds what was clamped away.
        let cleaned = section_value(&s, section);
        let obj = section_object(&mut raw, section);
        for (k, _) in &changes {
            if let Some(v) = cleaned.get(k) {
                obj.insert(k.clone(), v.clone());
            }
        }
        write_value_atomic(&self.path, &raw)?;
        self.mtime = mtime_of(&self.path);
        self.raw = raw;
        self.settings = s.clone();
        Ok(s)
    }

    /// Replaces the whole file; only `import_prototype` writes whole files, through `write_atomic`.
    #[allow(dead_code)]
    pub(crate) fn save(&self, s: &Settings) -> Result<()> {
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

    #[test]
    fn wrong_types_outside_altar_do_not_discard_the_file() {
        let (_d, p) = paths();
        write(&p, r#"{"general":{"autostart":"yes","client_log":5},"modules":{"altar":{"hud_bottom":99}}}"#);
        let s = SettingsStore::open(&p);
        assert_eq!(s.get().altar.hud_bottom, 99);
        assert_eq!(s.get().general, General::default());
        write(&p, r#"{"general":3,"modules":[1]}"#);
        assert_eq!(SettingsStore::open(&p).get(), &Settings::default());
    }

    fn read_json(p: &AppPaths) -> Value {
        serde_json::from_str(&std::fs::read_to_string(p.settings_file()).unwrap()).unwrap()
    }

    #[test]
    fn update_keeps_unknown_keys_and_clamps() {
        let (_d, p) = paths();
        write(
            &p,
            r#"{"general":{"autostart":false,"x":1},"modules":{"altar":{"hud_bottom":60,"future":"keep"},"other":{"a":2}},"top":true}"#,
        );
        let mut store = SettingsStore::open(&p);
        let s = store.update("altar", "hud_font_size", serde_json::json!(99)).unwrap();
        assert_eq!(s.altar.hud_font_size, 24);
        assert_eq!(store.get().altar.hud_font_size, 24);
        let v = read_json(&p);
        assert_eq!(v["general"]["x"], 1);
        assert_eq!(v["modules"]["altar"]["future"], "keep");
        assert_eq!(v["modules"]["other"]["a"], 2);
        assert_eq!(v["top"], true);
        assert_eq!(v["modules"]["altar"]["hud_font_size"], 24);
        assert!(!p.user.join("settings.json.tmp").exists());
    }

    #[test]
    fn update_rejects_unknown_key() {
        let (_d, p) = paths();
        let text = r#"{"modules":{"altar":{"hud_bottom":60}}}"#;
        write(&p, text);
        let mut store = SettingsStore::open(&p);
        assert!(store.update("altar", "nope", serde_json::json!(1)).is_err());
        assert!(store.update("ghost", "enabled", serde_json::json!(true)).is_err());
        assert!(store.reset("altar", &["nope"]).is_err());
        assert_eq!(std::fs::read_to_string(p.settings_file()).unwrap(), text);
    }

    #[test]
    fn update_general_client_log() {
        let (_d, p) = paths();
        let mut store = SettingsStore::open(&p);
        let s = store.update("general", "client_log", serde_json::json!("D:/PoE/logs/Client.txt")).unwrap();
        assert_eq!(s.general.client_log, Some(PathBuf::from("D:/PoE/logs/Client.txt")));
        assert_eq!(SettingsStore::open(&p).get().general.client_log, Some(PathBuf::from("D:/PoE/logs/Client.txt")));
        let s = store.update("general", "client_log", serde_json::json!("")).unwrap();
        assert_eq!(s.general.client_log, None);
        store.update("general", "client_log", serde_json::json!("C:/a")).unwrap();
        let s = store.update("general", "client_log", Value::Null).unwrap();
        assert_eq!(s.general.client_log, None);
    }

    #[test]
    fn reset_restores_defaults() {
        let (_d, p) = paths();
        let mut store = SettingsStore::open(&p);
        store.update("altar", "hud_bottom", serde_json::json!(300)).unwrap();
        assert_eq!(store.get().altar.hud_bottom, 300);
        let s = store.reset("altar", &["hud_bottom"]).unwrap();
        assert_eq!(s.altar.hud_bottom, 60);
        assert_eq!(SettingsStore::open(&p).get().altar.hud_bottom, 60);
    }

    #[test]
    fn update_survives_a_non_object_file() {
        let (_d, p) = paths();
        write(&p, r#"{"general":3,"modules":[1]}"#);
        let mut store = SettingsStore::open(&p);
        let s = store.update("altar", "hud_bottom", serde_json::json!(5)).unwrap();
        assert_eq!(s.altar.hud_bottom, 5);
        assert_eq!(read_json(&p)["modules"]["altar"]["hud_bottom"], 5);
    }

    #[test]
    fn reload_refreshes_the_raw_json_too() {
        let (_d, p) = paths();
        write(&p, r#"{"top":1}"#);
        let mut store = SettingsStore::open(&p);
        write(&p, r#"{"top":2}"#);
        let f = std::fs::File::options().write(true).open(p.settings_file()).unwrap();
        f.set_modified(SystemTime::now() + Duration::from_secs(5)).unwrap();
        assert!(store.reload_if_changed());
        store.update("altar", "hud_bottom", serde_json::json!(5)).unwrap();
        assert_eq!(read_json(&p)["top"], 2);
    }

    #[test]
    fn reload_keeps_previous_settings_on_broken_file() {
        let (_d, p) = paths();
        write(&p, r#"{"modules": {"altar": {"hud_bottom": 99}}}"#);
        let mut store = SettingsStore::open(&p);
        write(&p, "{broken");
        let f = std::fs::File::options().write(true).open(p.settings_file()).unwrap();
        f.set_modified(SystemTime::now() + Duration::from_secs(5)).unwrap();
        store.reload_if_changed();
        assert_eq!(store.get().altar.hud_bottom, 99);
    }
}
