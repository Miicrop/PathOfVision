use altar::detect::Icon;
use altar::live::{ProfileCache, Tracker};
use altar::matching::Match;
use altar::read::{AltarRead, OptionRead};
use altar::verdict::{Pos, Verdict};
use altar::{AltarKind, Card, Target, load_cards, repo_data_dir};
use std::cell::{Cell, RefCell};
use vision::Scale;
use std::path::PathBuf;

const TOP_ID: &str = "TangleAltarUpsideBossDivineOrb"; // top column of the default profile
const UP_A: &str = "CleansingAltarUpsideBossChaosOrb";
const UP_B: &str = "CleansingAltarUpsideBossDivinationCardsCorrupted";
const DOWN: &str = "CleansingAltarDownsideBossArmour";

fn cards() -> Vec<Card> {
    load_cards(&repo_data_dir().join("mods.json")).unwrap()
}

fn icon(x: i32, y: i32) -> Icon {
    Icon { x, y, score: 0.9 }
}

fn mat(cards: &[Card], id: &str, partial: bool) -> Match {
    let card = cards.iter().position(|c| c.id == id).unwrap_or_else(|| panic!("no card {id}"));
    Match { card, lines: vec![0], score: 90.0, values: vec![], in_range: true, partial, variant: 0, assign: vec![Some(0)] }
}

fn option(cards: &[Card], pos: Pos, ids: &[&str], partial: bool) -> OptionRead {
    OptionRead {
        position: pos,
        target: Some(Target::Boss),
        lines: vec![],
        matches: ids.iter().map(|id| mat(cards, id, partial)).collect(),
        unexplained: vec![],
        has_header: true,
    }
}

fn altar(cards: &[Card], at: Icon, top: &[&str], bottom: &[&str], partial: bool) -> AltarRead {
    AltarRead {
        icon: at,
        altar: AltarKind::Eater,
        options: [option(cards, Pos::Top, top, partial), option(cards, Pos::Bottom, bottom, partial)],
    }
}

fn complete(cards: &[Card], at: Icon) -> AltarRead {
    altar(cards, at, &[UP_A], &[DOWN], false)
}

fn incomplete(cards: &[Card], at: Icon) -> AltarRead {
    altar(cards, at, &[UP_A], &[DOWN], true)
}

#[test]
fn new_altar_is_read_once_when_complete() {
    let cards = cards();
    let calls = Cell::new(0);
    let mut read = |i: &Icon| {
        calls.set(calls.get() + 1);
        Ok(Some(complete(&cards, *i)))
    };
    let mut t = Tracker::new();
    for k in 0..3 {
        let n = t.scan(k as f64 * 0.25, &[icon(500, 500)], &cards, Scale::REFERENCE, &mut read).len();
        assert_eq!(n, 1);
    }
    assert_eq!(calls.get(), 1);
}

#[test]
fn incomplete_altar_is_reread_every_half_second() {
    let cards = cards();
    let calls = Cell::new(0);
    let mut read = |i: &Icon| {
        calls.set(calls.get() + 1);
        Ok(Some(incomplete(&cards, *i)))
    };
    let mut t = Tracker::new();
    t.scan(0.0, &[icon(500, 500)], &cards, Scale::REFERENCE, &mut read);
    assert_eq!(calls.get(), 1);
    t.scan(0.25, &[icon(500, 500)], &cards, Scale::REFERENCE, &mut read);
    assert_eq!(calls.get(), 1);
    t.scan(0.5, &[icon(500, 500)], &cards, Scale::REFERENCE, &mut read);
    assert_eq!(calls.get(), 2);
}

#[test]
fn moving_icon_keeps_its_track() {
    let cards = cards();
    let calls = Cell::new(0);
    let mut read = |i: &Icon| {
        calls.set(calls.get() + 1);
        Ok(Some(complete(&cards, *i)))
    };
    let mut t = Tracker::new();
    t.scan(0.0, &[icon(500, 500)], &cards, Scale::REFERENCE, &mut read);
    let tracked = t.scan(0.25, &[icon(700, 500)], &cards, Scale::REFERENCE, &mut read);
    assert_eq!(tracked.len(), 1);
    assert_eq!(tracked[0].icon.x, 700);
    assert_eq!(calls.get(), 1);
}

#[test]
fn far_jump_is_matched_by_content() {
    let cards = cards();
    let mut read = |i: &Icon| Ok(Some(complete(&cards, *i)));
    let mut t = Tracker::new();
    t.scan(0.0, &[icon(500, 500)], &cards, Scale::REFERENCE, &mut read);
    let tracked = t.scan(0.25, &[icon(1100, 500)], &cards, Scale::REFERENCE, &mut read);
    assert_eq!(tracked.len(), 1);
    assert_eq!(tracked[0].icon.x, 1100);
}

#[test]
fn better_read_replaces_worse() {
    let cards = cards();
    let reads = RefCell::new(vec![
        altar(&cards, icon(500, 500), &[UP_A], &[DOWN], true),
        altar(&cards, icon(510, 500), &[UP_A, UP_B], &[DOWN], true),
        altar(&cards, icon(530, 500), &[UP_A], &[DOWN], true),
    ]);
    let mut read = |_: &Icon| Ok(Some(reads.borrow_mut().remove(0)));
    let mut t = Tracker::new();
    t.scan(0.0, &[icon(500, 500)], &cards, Scale::REFERENCE, &mut read);
    let tracked = t.scan(0.5, &[icon(510, 500)], &cards, Scale::REFERENCE, &mut read);
    assert_eq!(tracked[0].options[0].matches.len(), 2, "better read is kept");
    let tracked = t.scan(1.0, &[icon(530, 500)], &cards, Scale::REFERENCE, &mut read);
    assert_eq!(tracked[0].options[0].matches.len(), 2, "worse read is ignored");
    assert_eq!(tracked[0].icon.x, 530, "but the icon position follows");
}

#[test]
fn altar_is_dropped_after_timeout() {
    let cards = cards();
    let mut read = |i: &Icon| Ok(Some(complete(&cards, *i)));
    let mut t = Tracker::new();
    t.scan(0.0, &[icon(500, 500)], &cards, Scale::REFERENCE, &mut read);
    assert_eq!(t.scan(1.4, &[], &cards, Scale::REFERENCE, &mut read).len(), 1);
    assert_eq!(t.scan(1.6, &[], &cards, Scale::REFERENCE, &mut read).len(), 0);
}

#[test]
fn read_error_skips_only_that_altar() {
    let cards = cards();
    let fail = Cell::new(true);
    let mut read = |i: &Icon| {
        if i.x == 300 && fail.get() {
            anyhow::bail!("ocr failed");
        }
        // Different mods per altar, or the content match would merge them.
        let up = if i.x == 300 { UP_A } else { UP_B };
        Ok(Some(altar(&cards, *i, &[up], &[DOWN], false)))
    };
    let mut t = Tracker::new();
    let icons = [icon(300, 500), icon(2000, 500)];
    let tracked = t.scan(0.0, &icons, &cards, Scale::REFERENCE, &mut read);
    assert_eq!(tracked.len(), 1);
    assert_eq!(tracked[0].icon.x, 2000);
    fail.set(false);
    assert_eq!(t.scan(0.25, &icons, &cards, Scale::REFERENCE, &mut read).len(), 2);
}

#[test]
fn scale_change_clears_tracks() {
    let cards = cards();
    let calls = Cell::new(0);
    let mut read = |i: &Icon| {
        calls.set(calls.get() + 1);
        Ok(Some(complete(&cards, *i)))
    };
    let mut t = Tracker::new();
    t.scan(0.0, &[icon(500, 500)], &cards, Scale(1.0), &mut read);
    assert_eq!(calls.get(), 1);
    // A tiny drift is not a resolution change.
    t.scan(0.25, &[icon(500, 500)], &cards, Scale(1.005), &mut read);
    assert_eq!(calls.get(), 1);
    // The same icon list at another scale starts without the old track: it is read again.
    let tracked = t.scan(0.5, &[icon(500, 500)], &cards, Scale(0.75), &mut read).len();
    assert_eq!(calls.get(), 2, "old track survived the scale change");
    assert_eq!(tracked, 1);
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("altar-live-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn views_apply_the_profile() {
    let cards = cards();
    let mut read = |i: &Icon| Ok(Some(altar(&cards, *i, &[TOP_ID], &[DOWN], false)));
    let mut t = Tracker::new();
    t.scan(0.0, &[icon(500, 500)], &cards, Scale::REFERENCE, &mut read);
    let default = repo_data_dir().join("profiles/default.json");
    let mut profiles = ProfileCache::new(default.clone());
    let profile = profiles.get(&default).clone();
    let views = t.views(&cards, &profile);
    assert_eq!(views.len(), 1);
    assert_eq!(views[0].verdicts[0].verdict, Verdict::Take);
    assert_eq!(views[0].recommendation.text, "Take the top option.");
    assert!(views[0].complete);
}

fn touch(path: &std::path::Path, secs: u64) {
    let later = std::time::SystemTime::now() + std::time::Duration::from_secs(secs);
    std::fs::File::options().write(true).open(path).unwrap().set_modified(later).unwrap();
}

#[test]
fn profile_cache_reloads_on_change_and_survives_broken_files() {
    let dir = temp_dir("profiles");
    let path = dir.join("p.json");
    let fallback = repo_data_dir().join("profiles/default.json");
    std::fs::write(&path, r#"{"name":"one"}"#).unwrap();
    let mut cache = ProfileCache::new(fallback.clone());
    assert_eq!(cache.get(&path).name, "one");
    std::fs::write(&path, r#"{"name":"two","upsides":{"s":["x"]}}"#).unwrap();
    touch(&path, 5);
    assert_eq!(cache.get(&path).name, "two");
    std::fs::write(&path, "{broken").unwrap();
    touch(&path, 10);
    assert_eq!(cache.get(&path).name, "two");

    let broken = dir.join("broken.json");
    std::fs::write(&broken, "{broken").unwrap();
    let mut fresh = ProfileCache::new(fallback);
    assert_eq!(fresh.get(&broken).name, "Default");
    let _ = std::fs::remove_dir_all(&dir);
}
