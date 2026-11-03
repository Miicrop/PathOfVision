//! Ported from tests/test_match.py; detailed values are the Python prototype's output.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use altar::fill::{fill_values, matching_blocks};
use altar::matching::{Match, match_box};
use altar::{Card, Target, load_cards, repo_data_dir};

static CARDS: LazyLock<Vec<Card>> =
    LazyLock::new(|| load_cards(&repo_data_dir().join("mods.json")).unwrap());

fn s(lines: &[&str]) -> Vec<String> {
    lines.iter().map(|l| l.to_string()).collect()
}

fn ids(matches: &[Match]) -> BTreeMap<String, usize> {
    let mut out = BTreeMap::new();
    for m in matches {
        *out.entry(CARDS[m.card].id.clone()).or_insert(0) += 1;
    }
    out
}

fn counter(items: &[(&str, usize)]) -> BTreeMap<String, usize> {
    items.iter().map(|&(k, n)| (k.to_string(), n)).collect()
}

fn interleaved_lines() -> Vec<String> {
    s(&[
        "HITS ALWAYS SHOCK",
        "GAIN 118% OF PHYSICAL DAMAGE AS EXTRA LIGHTNING DAMAGE",
        "ALL DAMAGE CAN SHOCK",
        "GAIN 105% OF PHYSICAL DAMAGE AS EXTRA COLD DAMAGE",
        "ALL DAMAGE WITH HITS CAN CHILL",
        "FINAL BOSS DROPS 4 ADDITIONAL DIVINATION CARDS WHICH REWARD BASIC CURRENCY",
    ])
}

#[test]
fn interleaved_multi_line_mods() {
    // In-game order mixes the lines of different mods (screen_13).
    let lines = interleaved_lines();
    let (matches, rest) = match_box(&lines, &CARDS, Some(Target::Boss));
    assert_eq!(rest, Vec::<usize>::new());
    assert_eq!(
        ids(&matches),
        counter(&[
            ("TangledAltarDownsideBossPhysToAddAsLightning", 1),
            ("TangledAltarDownsideBossPhysToAddAsCold", 1),
            ("TangledAltarUpsideBossDivinationCardsCurrencyBasic", 1),
        ])
    );
    // Python: lines [1, 2, 0] / [3, 4] / [5], all score 100.0, sorted by first line.
    let got: Vec<(Vec<usize>, f64)> = matches.iter().map(|m| (m.lines.clone(), m.score)).collect();
    assert_eq!(
        got,
        vec![
            (vec![1, 2, 0], 100.0),
            (vec![3, 4], 100.0),
            (vec![5], 100.0)
        ]
    );
    assert_eq!(matches[0].values, vec![118.0]);
    assert!(matches.iter().all(|m| m.in_range && !m.partial));
}

#[test]
fn shared_line_goes_to_the_right_mods() {
    // Two mods each own a "Gain X% ... as Extra Fire Damage" line (screen_23).
    let lines = s(&[
        "HITS ALWAYS IGNITE",
        "GAIN 82% OF PHYSICAL DAMAGE AS EXTRA FIRE DAMAGE",
        "ALL DAMAGE CAN IGNITE",
        "GAIN 71% OF PHYSICAL DAMAGE AS FIRE DAMAGE", // OCR dropped "EXTRA"
        "COVER ENEMIES IN ASH ON HIT",
    ]);
    let (matches, rest) = match_box(&lines, &CARDS, Some(Target::Boss));
    assert_eq!(rest, Vec::<usize>::new());
    assert_eq!(
        ids(&matches),
        counter(&[
            ("CleansingAltarDownsideBossPhysToAddAsFire", 1),
            ("CleansingAltarDownsideBossCoveredInAshOnHit", 1)
        ])
    );
    // Python: [3, 2, 0] 97.7 in range; [4, 1] 100.0 out of range.
    assert_eq!(matches[0].lines, vec![3, 2, 0]);
    assert_eq!(matches[0].score, 97.7);
    assert!(matches[0].in_range);
    assert_eq!(matches[1].lines, vec![4, 1]);
    assert_eq!(matches[1].score, 100.0);
    assert!(!matches[1].in_range);
}

#[test]
fn same_mod_twice() {
    let lines = s(&["24% INCREASED RARITY OF ITEMS FOUND IN THIS AREA"; 2]);
    let (matches, _) = match_box(&lines, &CARDS, Some(Target::Player));
    assert_eq!(
        ids(&matches),
        counter(&[("TangledAltarUpsidePlayerIncreasedRarity", 2)])
    );
    assert_eq!(matches[0].lines, vec![0]);
    assert_eq!(matches[1].lines, vec![1]);
}

#[test]
fn partly_hidden_multi_line_mod() {
    let lines = s(&[
        "+10% TO MAXIMUM FIRE RESISTANCE",
        "+80% TO FIRE RESISTANCE",
        "2.3% CHANCE TO DROP AN ADDITIONAL ORB OF UNMAKING",
    ]);
    let (matches, rest) = match_box(&lines, &CARDS, Some(Target::Minions));
    assert_eq!(rest, Vec::<usize>::new());
    let by_id: BTreeMap<&str, &Match> = matches
        .iter()
        .map(|m| (CARDS[m.card].id.as_str(), m))
        .collect();
    let resist = by_id["CleansingAltarDownsideMonsterFireAndChaosResist"];
    assert!(resist.partial);
    assert!(!by_id["CleansingAltarUpsideMonsterOrbOfUnmaking"].partial);
    // Python: lines [1, 0], score 50.0, values [80, 10], out of range, assign [1, None, 0, None].
    assert_eq!(resist.lines, vec![1, 0]);
    assert_eq!(resist.score, 50.0);
    assert_eq!(resist.values, vec![80.0, 10.0]);
    assert!(!resist.in_range);
    assert_eq!(resist.assign, vec![Some(1), None, Some(0), None]);
    assert_eq!(
        resist.display(&CARDS, &lines),
        s(&[
            "+80% to Fire Resistance",
            "+80% to Chaos Resistance",
            "+10% to maximum Fire Resistance",
            "+10% to maximum Chaos Resistance"
        ])
    );
}

#[test]
fn unknown_target_picks_the_best_fit() {
    let lines = s(&[
        "+10% TO MAXIMUM COLD RESISTANCE",
        "+80% TO COLD RESISTANCE",
        "+10% TO MAXIMUM LIGHTNING RESISTANCE",
        "+80% TO LIGHTNING RESISTANCE",
        "FINAL BOSS DROPS 4 ADDITIONAL DIVINATION CARDS WHICH REWARD BASIC CURRENCY",
    ]);
    let (matches, rest) = match_box(&lines, &CARDS, None);
    assert_eq!(rest, Vec::<usize>::new());
    assert!(!matches.is_empty());
    assert!(matches.iter().all(|m| CARDS[m.card].target == Target::Boss));
    assert_eq!(matches[0].lines, vec![1, 3, 0, 2]);
}

#[test]
fn display_fills_read_values_into_clean_text() {
    let lines = s(&[
        "33% CHANCE TO BE SAPPED WHEN HIT 78 GOLD",
        "ALL DAMAGE TAKEN FROM HITS CAN SAP YOU",
    ]);
    let (matches, _) = match_box(&lines, &CARDS, Some(Target::Player));
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].score, 94.9);
    assert_eq!(matches[0].values, vec![33.0, 78.0]);
    assert_eq!(
        matches[0].display(&CARDS, &lines),
        s(&[
            "33% chance to be Sapped when Hit",
            "All Damage taken from Hits can Sap you"
        ])
    );
}

#[test]
fn fill_values_aligns_by_words() {
    for (data, ocr, shown) in [
        (
            "Create Shocked Ground on Death, lasting 3 seconds",
            "100% CHANCE TO CREATE SHOCKED GROUND ON DEATH, LASTING 3 SECONDS",
            "Create Shocked Ground on Death, lasting 3 seconds",
        ),
        (
            "(1.6-3.2)% chance to drop an additional Orb of Fusing",
            "4,3% CHANCE TO DROP AN ADDITIONAL ORB OF FUSING",
            "4.3% chance to drop an additional Orb of Fusing",
        ),
        (
            "-(60-40)% to Cold Resistance",
            "-52% TO COLD RESISTANCE",
            "-52% to Cold Resistance",
        ),
    ] {
        assert_eq!(fill_values(data, ocr), shown, "{data}");
    }
}

#[test]
fn fill_values_keeps_unmatched_slots() {
    // Python: _fill_values("(2-4)% x (10-20) foo", "BAR") == "(2-4)% x (10-20) foo"
    assert_eq!(
        fill_values("(2-4)% x (10-20) foo", "BAR"),
        "(2-4)% x (10-20) foo"
    );
}

#[test]
fn matching_blocks_like_difflib() {
    let w = |t: &str| t.chars().map(|c| c.to_string()).collect::<Vec<_>>();
    assert_eq!(
        matching_blocks(&w("ABCD"), &w("BCXD")),
        vec![(1, 0, 2), (3, 3, 1), (4, 4, 0)]
    );
    // More reference outputs from difflib.SequenceMatcher(None, a, b, autojunk=False).
    assert_eq!(
        matching_blocks(&w("ABXABCAB"), &w("ABCABXAB")),
        vec![(0, 3, 5), (8, 8, 0)]
    );
    assert_eq!(
        matching_blocks(&w("AAAA"), &w("AA")),
        vec![(0, 0, 2), (4, 2, 0)]
    );
    assert_eq!(
        matching_blocks(&w("ABAB"), &w("BABA")),
        vec![(0, 1, 3), (4, 4, 0)]
    );
    assert_eq!(matching_blocks(&w(""), &w("AB")), vec![(0, 2, 0)]);
    assert_eq!(matching_blocks(&w("XYZ"), &w("ABC")), vec![(3, 3, 0)]);
}

#[test]
#[ignore]
fn timing_interleaved() {
    let lines = interleaved_lines();
    let _ = match_box(&lines, &CARDS, Some(Target::Boss));
    let n = 50;
    let t = std::time::Instant::now();
    for _ in 0..n {
        std::hint::black_box(match_box(
            std::hint::black_box(&lines),
            &CARDS,
            Some(Target::Boss),
        ));
    }
    println!(
        "match_box interleaved (boss): {:?} per call",
        t.elapsed() / n
    );
    let t = std::time::Instant::now();
    for _ in 0..n {
        std::hint::black_box(match_box(std::hint::black_box(&lines), &CARDS, None));
    }
    println!(
        "match_box interleaved (no target): {:?} per call",
        t.elapsed() / n
    );
}
