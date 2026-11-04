use std::sync::{Arc, Mutex};

use altar::board::{BoardApi, profile_file_name, upgrade_upsides};
use pov_core::{AppPaths, SettingsStore};
use serde_json::{Value, json};

/// Shipped data copied into a temp dir (so the user's own prototype profiles never show up).
fn setup() -> (tempfile::TempDir, BoardApi, AppPaths) {
    let tmp = tempfile::tempdir().unwrap();
    let data = tmp.path().join("data");
    std::fs::create_dir_all(data.join("profiles")).unwrap();
    for f in ["mods.json", "labels.json", "keywords.json", "profiles/default.json", "profiles/empty.json"] {
        std::fs::copy(altar::repo_data_dir().join(f), data.join(f)).unwrap();
    }
    let paths = AppPaths::with(data, tmp.path().join("user"));
    let store = Arc::new(Mutex::new(SettingsStore::open(&paths)));
    (tmp, BoardApi::new(paths.clone(), store), paths)
}

fn files(profiles: &Value) -> Vec<String> {
    profiles.as_array().unwrap().iter().map(|p| p["file"].as_str().unwrap().to_string()).collect()
}

#[test]
fn state_lists_mods_profiles_and_active() {
    let (_t, api, paths) = setup();
    let state = api.get_state().unwrap();
    let mods: Value = serde_json::from_str(&std::fs::read_to_string(paths.data.join("mods.json")).unwrap()).unwrap();
    let got = state["mods"].as_array().unwrap();
    assert_eq!(got.len(), mods["mods"].as_array().unwrap().len());
    for m in got {
        let mut keys: Vec<_> = m.as_object().unwrap().keys().cloned().collect();
        keys.sort();
        assert_eq!(keys, ["alias", "altars", "icon", "id", "lines", "side", "target", "weights"]);
    }
    assert!(state["keywords"].is_object());
    assert_eq!(state["gameVersion"], mods["gameVersion"]);
    assert_eq!(state["profiles"][0], json!({"file": "default.json", "name": "Default", "readOnly": true}));
    assert_eq!(files(&state["profiles"]), ["default.json", "empty.json"]);
    assert_eq!(state["active"], "default.json");
}

#[test]
fn duplicate_picks_a_free_file_name() {
    let (_t, api, paths) = setup();
    let first = api.duplicate_profile("default.json", "My Farm").unwrap();
    assert_eq!(first["file"], "my-farm.json");
    let second = api.duplicate_profile("default.json", "My Farm").unwrap();
    assert_eq!(second["file"], "my-farm-2.json");
    for f in ["my-farm.json", "my-farm-2.json"] {
        let v: Value = serde_json::from_str(&std::fs::read_to_string(paths.profiles_dir("altar").join(f)).unwrap()).unwrap();
        assert_eq!(v["name"], "My Farm");
        assert_eq!(v["readOnly"], false);
    }
    assert_eq!(api.get_state().unwrap()["active"], "my-farm-2.json");
    assert_eq!(files(&second["profiles"]), ["default.json", "empty.json", "my-farm-2.json", "my-farm.json"]);
    assert_eq!(profile_file_name("!!!", |_| false), "profile.json");
    assert_eq!(profile_file_name("My Scarab  Farm!", |_| false), "my-scarab-farm.json");
    assert_eq!(profile_file_name("a", |f| f == "a.json"), "a-2.json");
}

#[test]
fn save_keeps_unknown_keys() {
    let (_t, api, paths) = setup();
    let file = api.duplicate_profile("default.json", "Mine").unwrap()["file"].as_str().unwrap().to_string();
    let mut profile = api.load_profile(&file).unwrap();
    profile["extra"] = json!({"a": 1});
    profile["readOnly"] = json!(true);
    assert_eq!(api.save_profile(&file, profile).unwrap(), json!({"ok": true}));
    let text = std::fs::read_to_string(paths.profiles_dir("altar").join(&file)).unwrap();
    assert!(text.contains("\"extra\""));
    assert!(text.ends_with("}\n"));
    assert!(text.starts_with("{\n  \"name\""), "key order survives");
    let v: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(v["readOnly"], false);
    assert_eq!(v["extra"], json!({"a": 1}));
}

#[test]
fn read_only_profiles_cannot_be_saved_or_deleted() {
    let (_t, api, paths) = setup();
    let path = paths.data.join("profiles/default.json");
    let before = std::fs::read(&path).unwrap();
    let err = json!({"ok": false, "error": "read-only profile"});
    assert_eq!(api.save_profile("default.json", json!({"name": "x"})).unwrap(), err);
    assert_eq!(api.delete_profile("default.json").unwrap(), err);
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert!(!paths.profiles_dir("altar").join("default.json").exists());
    // a user file marked read-only is protected too
    std::fs::create_dir_all(paths.profiles_dir("altar")).unwrap();
    std::fs::write(paths.profiles_dir("altar").join("ro.json"), r#"{"name":"Ro","readOnly":true}"#).unwrap();
    assert_eq!(api.save_profile("ro.json", json!({"name": "x"})).unwrap(), err);
    assert_eq!(api.delete_profile("ro.json").unwrap(), err);
}

#[test]
fn delete_active_profile_falls_back_to_default() {
    let (_t, api, paths) = setup();
    let file = api.duplicate_profile("default.json", "Gone").unwrap()["file"].as_str().unwrap().to_string();
    assert_eq!(api.get_state().unwrap()["active"], file.as_str());
    let out = api.delete_profile(&file).unwrap();
    assert_eq!(out["ok"], true);
    assert_eq!(out["active"], "default.json");
    assert_eq!(files(&out["profiles"]), ["default.json", "empty.json"]);
    assert!(!paths.profiles_dir("altar").join(&file).exists());
    let settings: Value = serde_json::from_str(&std::fs::read_to_string(paths.settings_file()).unwrap()).unwrap();
    assert_eq!(settings["modules"]["altar"]["profile"], "default.json");
}

#[test]
fn parallel_saves_of_one_profile_do_not_collide() {
    let (_t, api, paths) = setup();
    let file = api.duplicate_profile("default.json", "Busy").unwrap()["file"].as_str().unwrap().to_string();
    let base = api.load_profile(&file).unwrap();
    let mut a = base.clone();
    a["name"] = json!("A");
    let mut b = base;
    b["name"] = json!("B");
    let path = paths.profiles_dir("altar").join(&file);
    let (api, file, path, a, b) = (&api, &file, &path, &a, &b);
    std::thread::scope(|s| {
        for p in [a, b] {
            s.spawn(move || {
                for _ in 0..50 {
                    assert_eq!(api.save_profile(file, p.clone()).unwrap(), json!({"ok": true}));
                    let v: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
                    assert!(&v == a || &v == b, "file holds neither save: {v}");
                }
            });
        }
    });
}

#[test]
fn file_names_cannot_escape_the_profile_folder() {
    let (_t, api, paths) = setup();
    std::fs::write(paths.user.join("settings.json"), "{}").unwrap_or_else(|_| {
        std::fs::create_dir_all(&paths.user).unwrap();
        std::fs::write(paths.user.join("settings.json"), "{}").unwrap();
    });
    assert!(api.load_profile("../settings.json").is_err());
    assert!(api.save_profile("..\\x.json", json!({})).is_err());
    assert!(api.delete_profile("a/b.json").is_err());
    assert!(api.load_profile("settings.txt").is_err());
    assert!(api.load_profile("missing.json").is_err());
    assert!(api.set_active("../x.json").is_err());
    assert!(!paths.user.join("x.json").exists());
}

#[test]
fn old_upsides_are_upgraded() {
    let up = upgrade_upsides(&json!({"s": ["a"], "a": ["b"], "b": ["c"]}));
    assert_eq!(
        up,
        json!({"columns": [{"name": "B", "ids": ["c"]}, {"name": "A", "ids": ["b"]}], "top": {"name": "S", "ids": ["a"]}})
    );
    let new = upgrade_upsides(&json!({"columns": [{"name": "K", "ids": ["x"], "key": true}, {"name": "L", "ids": []}], "top": {"ids": ["t"]}}));
    assert_eq!(new["columns"][0]["key"], true);
    assert!(new["columns"][1].get("key").is_none());
    assert_eq!(new["top"], json!({"name": "S", "ids": ["t"]}));
    let (_t, api, _p) = setup();
    assert!(api.load_profile("empty.json").unwrap()["downsides"].is_object());
}
