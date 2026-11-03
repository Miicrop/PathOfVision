use altar::evaluate::{evaluate, fixtures_dir};

#[test]
#[ignore = "slow: run with cargo test -p altar --release --test golden -- --ignored"]
fn recognition_matches_the_prototype() {
    let screens = fixtures_dir().join("screens");
    if !screens.join("screen_02.png").exists() {
        eprintln!("skipped: no local screenshots");
        return;
    }
    let r = evaluate(&screens, &fixtures_dir().join("labels.json")).unwrap();
    assert_eq!((r.altars_found, r.false_altars), (r.altars_total, 0), "{r}");
    // Python: 84/85, the single known miss.
    assert_eq!(r.failures.len(), 1, "{r}");
    assert!(r.failures[0].starts_with("screen_10.png [856, 1152] bottom"), "{r}");
}
