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

/// Acceptance bar for a real resolution: every altar found, no false detections, and at least
/// 95 % of the readable options fully correct. The screens are client-area crops made with
/// `tools/crop_client.py` (git-ignored, like all screenshots); skipped when absent.
fn real_set_passes(set: &str, labels: &str) {
    let screens = fixtures_dir().join(set).join("client");
    if !screens.join("screen_01.png").exists() {
        eprintln!("skipped: no local screenshots in {}", screens.display());
        return;
    }
    let r = evaluate(&screens, &fixtures_dir().join(labels)).unwrap();
    println!("{set}:\n{r}");
    assert_eq!((r.altars_found, r.false_altars), (r.altars_total, 0), "{r}");
    let share = r.options_ok as f64 / r.options_total as f64;
    assert!(share >= 0.95, "{:.1} % of options correct\n{r}", share * 100.0);
}

#[test]
#[ignore = "slow: run with cargo test -p altar --release --test golden -- --ignored"]
fn real_1920x1080() {
    real_set_passes("screens_1920x1080", "labels_1920x1080.json");
}

#[test]
#[ignore = "slow: run with cargo test -p altar --release --test golden -- --ignored"]
fn real_1920x1200() {
    real_set_passes("screens_1920x1200", "labels_1920x1200.json");
}

/// A windowed 2560x1353 client (between 1200 and 1440 px).
#[test]
#[ignore = "slow: run with cargo test -p altar --release --test golden -- --ignored"]
fn real_mid() {
    real_set_passes("screens_2560x1353", "labels_2560x1353.json");
}

/// A 3435x1132 window stretched over an ultrawide monitor; PoE pillarboxes the game to
/// 2717x1132. Scale still follows the client height.
#[test]
#[ignore = "slow: run with cargo test -p altar --release --test golden -- --ignored"]
fn real_custom() {
    real_set_passes("screens_custom", "labels_custom.json");
}
