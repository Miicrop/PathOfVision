use altar::evaluate::{evaluate, evaluate_scaled, fixtures_dir};

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

#[test]
#[ignore = "slow: run with cargo test -p altar --release --test golden -- --ignored"]
fn synthetic_1080_reads_well() {
    let screens = fixtures_dir().join("screens");
    if !screens.join("screen_02.png").exists() {
        eprintln!("skipped: no local screenshots");
        return;
    }
    let r = evaluate_scaled(&screens, &fixtures_dir().join("labels.json"), Some(1080)).unwrap();
    println!("synthetic 1080:\n{r}");
    assert_eq!((r.altars_found, r.false_altars), (r.altars_total, 0), "{r}");
    let share = r.options_ok as f64 / r.options_total as f64;
    assert!(share >= 0.95, "{:.1} % of options correct\n{r}", share * 100.0);
}
