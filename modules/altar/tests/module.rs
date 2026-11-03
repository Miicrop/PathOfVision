use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use altar::module::AltarModule;
use altar::repo_data_dir;
use pov_core::{AppPaths, Module, SettingsStore};
use pov_game::{Area, AreaKind, GameContext, PoeWindow, Rect};
use vision::Rgb;

fn screen(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/screens").join(name)
}

fn window(foreground: bool) -> PoeWindow {
    PoeWindow {
        hwnd: 0,
        pid: 0,
        exe: PathBuf::from("PathOfExile.exe"),
        client: Rect { x: 0, y: 0, w: 3440, h: 1440 },
        foreground,
        minimized: false,
    }
}

fn ctx(foreground: bool, kind: Option<AreaKind>) -> GameContext {
    GameContext {
        poe: Some(window(foreground)),
        area: kind.map(|kind| Area { id: "x".into(), level: 83, kind }),
    }
}

fn module() -> (tempfile::TempDir, AltarModule) {
    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths::with(repo_data_dir(), dir.path().join("user"));
    let store = Arc::new(Mutex::new(SettingsStore::open(&paths)));
    let m = AltarModule::new(&paths, store).unwrap();
    (dir, m)
}

#[test]
fn module_reads_altars_on_a_screenshot() {
    let path = screen("screen_02.png");
    if !path.exists() {
        eprintln!("skipped: {} missing", path.display());
        return;
    }
    let (_dir, mut m) = module();
    assert_eq!((m.id(), m.name()), ("altar", "Altars"));
    let c = ctx(true, Some(AreaKind::Map));
    assert!(m.wants_frames(&c));
    let out = m.on_frame(&Rgb::load(&path).unwrap(), &c).unwrap();
    assert_eq!(out.module, "altar");
    assert_eq!(out.lines.len(), 2, "{:?}", out.lines);
    for line in &out.lines {
        assert!(line.contains("top:") && line.contains("->"), "{line}");
    }
    assert_eq!(out.view["altars"].as_array().map(Vec::len), Some(2));
}

#[test]
fn module_is_idle_outside_maps() {
    let (_dir, m) = module();
    assert!(!m.wants_frames(&ctx(true, Some(AreaKind::Hideout))));
    assert!(m.wants_frames(&ctx(true, None)));
    assert!(!m.wants_frames(&ctx(false, Some(AreaKind::Map))));
    assert!(!m.wants_frames(&GameContext::default()));
}

#[test]
fn settings_changes_apply_while_idle() {
    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths::with(repo_data_dir(), dir.path().join("user"));
    let write = |enabled: bool, age: u64| {
        std::fs::create_dir_all(&paths.user).unwrap();
        let file = paths.settings_file();
        std::fs::write(&file, format!(r#"{{"modules": {{"altar": {{"enabled": {enabled}}}}}}}"#)).unwrap();
        let t = std::time::SystemTime::now() - std::time::Duration::from_secs(age);
        std::fs::File::options().write(true).open(&file).unwrap().set_modified(t).unwrap();
    };
    write(false, 60);
    let store = Arc::new(Mutex::new(SettingsStore::open(&paths)));
    let m = AltarModule::new(&paths, store).unwrap();
    let c = ctx(true, Some(AreaKind::Map));
    assert!(!m.wants_frames(&c));
    // Re-enabled while the module is idle (no on_frame runs): it must notice.
    write(true, 0);
    assert!(m.wants_frames(&c));
}
