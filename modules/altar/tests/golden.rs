use altar::evaluate::{evaluate, fixtures_dir};

#[test]
fn recognition_matches_the_prototype() {
    let screens = fixtures_dir().join("screens");
    if !screens.join("screen_02.png").exists() {
        eprintln!("skipped: no local screenshots");
        return;
    }
    let r = evaluate(&screens, &fixtures_dir().join("labels.json")).unwrap();
    assert_eq!((r.altars_found, r.false_altars), (r.altars_total, 0), "{r}");
    assert!(r.options_ok + 1 >= r.options_total, "{r}"); // Python: 84/85
}
