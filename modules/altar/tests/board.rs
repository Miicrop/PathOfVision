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
fn reserved_names_get_a_suffix() {
    assert_eq!(profile_file_name("CON", |_| false), "con-profile.json");
    assert_eq!(profile_file_name("lpt1", |_| false), "lpt1-profile.json");
    assert_eq!(profile_file_name("Nul", |_| false), "nul-profile.json");
    assert_eq!(profile_file_name("com9", |_| false), "com9-profile.json");
    assert_eq!(profile_file_name("console", |_| false), "console.json");
    assert_eq!(profile_file_name("com0", |_| false), "com0.json");
    assert_eq!(profile_file_name("con", |f| f == "con-profile.json"), "con-profile-2.json");
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

// Profile codes (Export / Import).

/// A code from raw JSON, the way `export_code` packs it.
fn pack(json: &Value) -> String {
    use base64::Engine;
    use std::io::Write;
    let mut enc = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
    enc.write_all(json.to_string().as_bytes()).unwrap();
    format!("POV1:{}", base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(enc.finish().unwrap()))
}

fn user_files(paths: &AppPaths) -> Vec<String> {
    let Ok(dir) = std::fs::read_dir(paths.profiles_dir("altar")) else { return vec![] };
    let mut out: Vec<String> = dir.map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
    out.sort();
    out
}

fn mod_ids(paths: &AppPaths, side: &str) -> Vec<String> {
    let mods: Value = serde_json::from_str(&std::fs::read_to_string(paths.data.join("mods.json")).unwrap()).unwrap();
    mods["mods"].as_array().unwrap().iter().filter(|m| m["side"] == side).map(|m| m["id"].as_str().unwrap().to_string()).collect()
}

#[test]
fn export_import_round_trip() {
    let (_t, api, paths) = setup();
    let up = mod_ids(&paths, "upside");
    let down = mod_ids(&paths, "downside");
    let file = api.duplicate_profile("default.json", "Ritual Farm").unwrap()["file"].as_str().unwrap().to_string();
    let mut profile = api.load_profile(&file).unwrap();
    profile["upsides"] = json!({
        "columns": [
            {"name": "Low", "ids": [up[3], up[1]]},
            {"name": "Key", "ids": [up[0]], "key": true},
            {"name": "Mid", "ids": [up[5], up[4], up[2]]},
        ],
        "top": {"name": "Best", "ids": [up[7], up[6]]},
    });
    profile["downsides"] = json!({"dangerous": [down[2], down[0]], "brick": [down[1]]});
    api.save_profile(&file, profile.clone()).unwrap();

    let code = api.export_code(&file).unwrap();
    assert!(code.starts_with("POV1:") && !code.contains(char::is_whitespace));
    let preview = api.import_preview(&code).unwrap();
    assert_eq!(preview, json!({"name": "Ritual Farm", "columns": 4, "upsides": 8, "dangerous": 2, "brick": 1, "unknown": 0}));

    // Discord-style wrapping inside and around the code is fine.
    let wrapped = format!("  {}\n{}\r\n ", &code[..20], &code[20..]);
    let result = api.import_code(&wrapped).unwrap();
    assert_eq!(result["file"], "ritual-farm-2.json");
    assert_eq!(result["skipped"], 0);
    let imported = api.load_profile("ritual-farm-2.json").unwrap();
    assert_eq!(imported["name"], "Ritual Farm (2)");
    assert_eq!(imported["readOnly"], false);
    assert_eq!(imported["upsides"], profile["upsides"]);
    assert_eq!(imported["downsides"], profile["downsides"]);
    assert_eq!(api.get_state().unwrap()["active"], "ritual-farm-2.json");
}

#[test]
fn import_never_overwrites_and_dedupes_the_name() {
    let (_t, api, paths) = setup();
    let code = api.export_code("default.json").unwrap();
    let default_before = std::fs::read_to_string(paths.data.join("profiles/default.json")).unwrap();
    let names: Vec<String> = (0..3)
        .map(|_| {
            let f = api.import_code(&code).unwrap()["file"].as_str().unwrap().to_string();
            api.load_profile(&f).unwrap()["name"].as_str().unwrap().to_string()
        })
        .collect();
    assert_eq!(names, ["Default (2)", "Default (3)", "Default (4)"]);
    assert_eq!(user_files(&paths), ["default-2.json", "default-3.json", "default-4.json"]);
    assert_eq!(std::fs::read_to_string(paths.data.join("profiles/default.json")).unwrap(), default_before);
    // The shipped default is read-only, its copy is not and keeps the tiers.
    let original = api.load_profile("default.json").unwrap();
    let copy = api.load_profile("default-2.json").unwrap();
    assert_eq!(copy["readOnly"], false);
    assert_eq!(copy["upsides"], original["upsides"]);
    assert_eq!(copy["downsides"]["dangerous"], original["downsides"]["dangerous"]);
    assert_eq!(copy["downsides"]["brick"], original["downsides"]["brick"]);
    assert!(copy.get("modsVersion").is_none());
}

#[test]
fn unknown_ids_are_dropped_and_counted() {
    let (_t, api, paths) = setup();
    let up = mod_ids(&paths, "upside");
    let down = mod_ids(&paths, "downside");
    let code = pack(&json!({
        "v": 1, "n": "  ",
        "c": [{"n": "A", "i": [up[0], "GoneUpside", up[1]], "k": true}],
        "t": {"n": "S", "i": ["GoneTop"]},
        "d": [down[0], "GoneDown"], "b": ["GoneUpside"],
    }));
    let preview = api.import_preview(&code).unwrap();
    assert_eq!(preview, json!({"name": "Imported profile", "columns": 2, "upsides": 2, "dangerous": 1, "brick": 0, "unknown": 3}));
    let result = api.import_code(&code).unwrap();
    assert_eq!(result["skipped"], 3);
    let p = api.load_profile(result["file"].as_str().unwrap()).unwrap();
    assert_eq!(p["name"], "Imported profile");
    assert_eq!(p["upsides"]["columns"][0], json!({"name": "A", "ids": [up[0], up[1]], "key": true}));
    assert_eq!(p["upsides"]["top"]["ids"], json!([]));
    assert_eq!(p["downsides"], json!({"dangerous": [down[0]], "brick": []}));
}

#[test]
fn invalid_codes_write_nothing() {
    let (_t, api, paths) = setup();
    let good = api.export_code("default.json").unwrap();
    let invalid = "This is not a valid profile code.";
    let mut cases: Vec<(String, &str)> = vec![
        ("".into(), invalid),
        ("hello world".into(), invalid),
        (good.replacen("POV1:", "POV2:", 1), invalid),
        (good.replacen("POV1:", "", 1), invalid),
        (good[..good.len() / 2].to_string(), invalid),
        ("POV1:!!!not base64!!!".into(), invalid),
        ("POV1:aGVsbG8".into(), invalid),
        (pack(&json!([1, 2])), invalid),
        (pack(&json!({"n": "x"})), invalid),
        (pack(&json!({"v": 1, "c": "nope"})), invalid),
        (pack(&json!({"v": 1, "c": [{"i": [1, 2]}]})), invalid),
        (pack(&json!({"v": 0})), invalid),
        (pack(&json!({"v": 2, "n": "Future"})), "This code was made by a newer version of Path of Vision."),
    ];
    // Inflates far beyond the size cap.
    let huge = format!("{{\"v\":1,\"n\":\"{}\"}}", "a".repeat(3 << 20));
    cases.push((pack(&serde_json::from_str(&huge).unwrap()), invalid));
    for (code, message) in &cases {
        let err = api.import_preview(code).unwrap_err();
        assert_eq!(err.to_string(), *message, "preview of {code:.40}");
        let err = api.import_code(code).unwrap_err();
        assert_eq!(err.to_string(), *message, "import of {code:.40}");
    }
    assert!(user_files(&paths).is_empty());
    assert_eq!(api.get_state().unwrap()["active"], "default.json");
}

#[test]
fn default_profile_code_is_one_short_line() {
    let (_t, api, _p) = setup();
    let code = api.export_code("default.json").unwrap();
    println!("default profile code: {} characters", code.len());
    assert!(!code.contains(char::is_whitespace));
    assert!(code.len() < 3000, "code is {} characters", code.len());
}
