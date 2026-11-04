//! Backend of the tier board: profile files and the board state. Port of `prototype/board.py`
//! `BoardApi`; the app exposes these functions as IPC commands, results keep the board's JSON shapes.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result, bail};
use pov_core::{AppPaths, SettingsStore};
use serde_json::{Map, Value, json};

use crate::labels::{load_keywords, load_labels};

const MODULE: &str = "altar";
const DEFAULT_PROFILE: &str = "default.json";

pub struct BoardApi {
    paths: AppPaths,
    settings: Arc<Mutex<SettingsStore>>,
    /// Held across every write to the profile folders or the active profile: the app runs its
    /// IPC commands in parallel, and two saves of one profile would share its `.json.tmp` file.
    writes: Mutex<()>,
}

fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}

/// Upsides in the column format; old profiles (`{"s", "a", "b"}`) become top = S, columns = [B, A].
pub fn upgrade_upsides(upsides: &Value) -> Value {
    let ids = |v: Option<&Value>| -> Vec<Value> { v.and_then(Value::as_array).cloned().unwrap_or_default() };
    let name = |v: Option<&Value>, default: &str| v.and_then(Value::as_str).unwrap_or(default).to_string();
    if upsides.get("top").is_none() && upsides.get("columns").is_none() {
        return json!({
            "columns": [
                {"name": "B", "ids": ids(upsides.get("b"))},
                {"name": "A", "ids": ids(upsides.get("a"))},
            ],
            "top": {"name": "S", "ids": ids(upsides.get("s"))},
        });
    }
    let columns: Vec<Value> = upsides
        .get("columns")
        .and_then(Value::as_array)
        .map(|cols| {
            cols.iter()
                .map(|c| {
                    let mut col = Map::new();
                    col.insert("name".into(), json!(name(c.get("name"), "")));
                    col.insert("ids".into(), Value::Array(ids(c.get("ids"))));
                    if c.get("key").is_some_and(truthy) {
                        col.insert("key".into(), json!(true));
                    }
                    Value::Object(col)
                })
                .collect()
        })
        .unwrap_or_default();
    let top = upsides.get("top");
    json!({
        "columns": columns,
        "top": {"name": name(top.and_then(|t| t.get("name")), "S"), "ids": ids(top.and_then(|t| t.get("ids")))},
    })
}

/// A free file name derived from a profile name: "My Scarab Farm" -> `my-scarab-farm.json`.
pub fn profile_file_name(name: &str, taken: impl Fn(&str) -> bool) -> String {
    let mut stem = String::new();
    let mut gap = false;
    for c in name.to_lowercase().chars() {
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            if gap && !stem.is_empty() {
                stem.push('-');
            }
            gap = false;
            stem.push(c);
        } else {
            gap = true;
        }
    }
    if stem.is_empty() {
        stem = "profile".into();
    }
    let mut candidate = format!("{stem}.json");
    let mut n = 2;
    while taken(&candidate) {
        candidate = format!("{stem}-{n}.json");
        n += 1;
    }
    candidate
}

fn read_only_error() -> Value {
    json!({"ok": false, "error": "read-only profile"})
}

fn read_json(path: &Path) -> Result<Value> {
    let text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))
}

fn write_profile(path: &Path, profile: &Value) -> Result<()> {
    let dir = path.parent().context("profile path has no parent")?;
    std::fs::create_dir_all(dir)?;
    let mut text = serde_json::to_string_pretty(profile)?;
    text.push('\n');
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, text).with_context(|| format!("writing {}", tmp.display()))?;
    std::fs::rename(&tmp, path).with_context(|| format!("replacing {}", path.display()))?;
    Ok(())
}

/// Only a bare `*.json` file name is accepted, so nothing outside the profile folders is reachable.
fn check_file_name(file: &str) -> Result<()> {
    let bare = Path::new(file).file_name().is_some_and(|f| f == file);
    if !bare || !file.ends_with(".json") || file.len() <= ".json".len() || file.contains([':', '/', '\\']) {
        bail!("not a profile file name: {file:?}");
    }
    Ok(())
}

impl BoardApi {
    pub fn new(paths: AppPaths, settings: Arc<Mutex<SettingsStore>>) -> BoardApi {
        BoardApi { paths, settings, writes: Mutex::new(()) }
    }

    fn user_path(&self, file: &str) -> PathBuf {
        self.paths.profiles_dir(MODULE).join(file)
    }

    fn shipped_path(&self, file: &str) -> PathBuf {
        self.paths.shipped_profiles_dir(MODULE).join(file)
    }

    /// The file a name stands for (user folder first) and whether it is a shipped one.
    fn resolve(&self, file: &str) -> Result<(PathBuf, bool)> {
        check_file_name(file)?;
        let user = self.user_path(file);
        if user.is_file() {
            return Ok((user, false));
        }
        let shipped = self.shipped_path(file);
        if shipped.is_file() {
            return Ok((shipped, true));
        }
        bail!("no such profile: {file}")
    }

    fn is_read_only(&self, file: &str) -> Result<bool> {
        let (path, shipped) = self.resolve(file)?;
        Ok(shipped || read_json(&path)?.get("readOnly").is_some_and(truthy))
    }

    /// Every profile of both folders (a user file hides a shipped one of the same name):
    /// read-only first, then by lowercase name.
    fn profiles(&self) -> Vec<Value> {
        let mut seen = HashSet::new();
        let mut out: Vec<(bool, String, Value)> = Vec::new();
        let dirs = [(self.paths.profiles_dir(MODULE), false), (self.paths.shipped_profiles_dir(MODULE), true)];
        for (dir, shipped) in dirs {
            let Ok(entries) = std::fs::read_dir(&dir) else { continue };
            let mut paths: Vec<PathBuf> = entries.filter_map(|e| e.ok()).map(|e| e.path()).collect();
            paths.sort();
            for path in paths {
                let Some(file) = path.file_name().and_then(|f| f.to_str()).map(str::to_string) else { continue };
                if !file.ends_with(".json") || !path.is_file() || !seen.insert(file.clone()) {
                    continue;
                }
                let Ok(data) = read_json(&path) else { continue };
                let name = data.get("name").and_then(Value::as_str).unwrap_or(&file[..file.len() - 5]).to_string();
                let read_only = shipped || data.get("readOnly").is_some_and(truthy);
                out.push((read_only, name.to_lowercase(), json!({"file": file, "name": name, "readOnly": read_only})));
            }
        }
        out.sort_by(|a, b| (!a.0, &a.1).cmp(&(!b.0, &b.1)));
        out.into_iter().map(|(_, _, v)| v).collect()
    }

    fn active(&self) -> String {
        let settings = self.settings.lock().unwrap_or_else(|e| e.into_inner());
        pov_core::active_profile_path(&self.paths, settings.get())
            .file_name()
            .and_then(|f| f.to_str())
            .unwrap_or(DEFAULT_PROFILE)
            .to_string()
    }

    pub fn get_state(&self) -> Result<Value> {
        let mods = read_json(&self.paths.data.join("mods.json"))?;
        let labels = load_labels(&self.paths.data.join("labels.json"));
        let keywords: Map<String, Value> = load_keywords(&self.paths.data.join("keywords.json"))
            .into_iter()
            .map(|(word, color)| (word, Value::String(color)))
            .collect();
        let list = mods.get("mods").and_then(Value::as_array).context("mods.json has no mods")?;
        let mods_out: Vec<Value> = list
            .iter()
            .map(|m| {
                let mut o = Map::new();
                for k in ["id", "side", "target", "altars", "lines", "weights"] {
                    o.insert(k.into(), m.get(k).cloned().unwrap_or(Value::Null));
                }
                let label = m.get("id").and_then(Value::as_str).and_then(|id| labels.get(id));
                o.insert("icon".into(), json!(label.map_or("", |l| l.icon.as_str())));
                o.insert("alias".into(), json!(label.map_or("", |l| l.alias.as_str())));
                Value::Object(o)
            })
            .collect();
        Ok(json!({
            "gameVersion": mods.get("gameVersion").cloned().unwrap_or(Value::Null),
            "mods": mods_out,
            "keywords": keywords,
            "profiles": self.profiles(),
            "active": self.active(),
        }))
    }

    pub fn load_profile(&self, file: &str) -> Result<Value> {
        let (path, _) = self.resolve(file)?;
        let mut profile = read_json(&path)?;
        let obj = profile.as_object_mut().with_context(|| format!("{file} is not a profile"))?;
        let upsides = upgrade_upsides(obj.get("upsides").unwrap_or(&Value::Null));
        obj.insert("upsides".into(), upsides);
        obj.entry("downsides").or_insert_with(|| json!({}));
        Ok(profile)
    }

    fn write_guard(&self) -> std::sync::MutexGuard<'_, ()> {
        self.writes.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn save_profile(&self, file: &str, mut profile: Value) -> Result<Value> {
        let _guard = self.write_guard();
        if self.is_read_only(file)? {
            return Ok(read_only_error());
        }
        profile.as_object_mut().context("a profile must be a JSON object")?.insert("readOnly".into(), json!(false));
        write_profile(&self.user_path(file), &profile)?;
        Ok(json!({"ok": true}))
    }

    pub fn duplicate_profile(&self, file: &str, name: &str) -> Result<Value> {
        let _guard = self.write_guard();
        let mut profile = self.load_profile(file)?;
        let obj = profile.as_object_mut().expect("load_profile returns an object");
        obj.insert("name".into(), json!(name));
        obj.insert("readOnly".into(), json!(false));
        let new_file = profile_file_name(name, |f| self.user_path(f).exists() || self.shipped_path(f).exists());
        write_profile(&self.user_path(&new_file), &profile)?;
        self.write_active(&new_file)?;
        Ok(json!({"file": new_file, "profiles": self.profiles()}))
    }

    pub fn delete_profile(&self, file: &str) -> Result<Value> {
        let _guard = self.write_guard();
        if self.is_read_only(file)? {
            return Ok(read_only_error());
        }
        // Read before deleting: once the file is gone the active profile resolves to the default.
        let was_active = self.active() == file;
        std::fs::remove_file(self.user_path(file)).with_context(|| format!("deleting {file}"))?;
        if was_active {
            self.write_active(DEFAULT_PROFILE)?;
        }
        Ok(json!({"ok": true, "profiles": self.profiles(), "active": self.active()}))
    }

    pub fn set_active(&self, file: &str) -> Result<()> {
        let _guard = self.write_guard();
        self.write_active(file)
    }

    /// `set_active` for callers that hold the write lock already.
    fn write_active(&self, file: &str) -> Result<()> {
        check_file_name(file)?;
        let mut settings = self.settings.lock().unwrap_or_else(|e| e.into_inner());
        settings.update(MODULE, "profile", json!(file))?;
        Ok(())
    }
}
