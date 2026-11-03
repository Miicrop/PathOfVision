//! Accuracy and timing report against `tests/fixtures/labels.json` (port of `prototype/evaluate.py`).
//!
//! An option counts as correct when the target and the multiset of card ids match exactly;
//! `"mods": null` marks an unreadable box and is skipped.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::Context;
use serde_json::Value;
use vision::Rgb;

use crate::data::Target;
use crate::read::{AltarRead, Reader};
use crate::verdict::Pos;

/// px between labeled and detected icon centers.
const ICON_TOLERANCE: i32 = 40;

pub fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures")
}

#[derive(Debug, Clone)]
pub struct Report {
    pub altars_found: usize,
    pub altars_total: usize,
    pub false_altars: usize,
    pub options_ok: usize,
    pub options_total: usize,
    pub median_ms: f64,
    pub max_ms: f64,
    pub failures: Vec<String>,
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "altars detected : {}/{}  (+{} false)", self.altars_found, self.altars_total, self.false_altars)?;
        let pct = 100.0 * self.options_ok as f64 / self.options_total.max(1) as f64;
        writeln!(f, "options correct : {}/{}  ({pct:.1} %)", self.options_ok, self.options_total)?;
        write!(f, "time per screen : median {:.0} ms, max {:.0} ms", self.median_ms, self.max_ms)?;
        for failure in &self.failures {
            write!(f, "\n  FAIL {failure}")?;
        }
        Ok(())
    }
}

fn target_word(t: Target) -> &'static str {
    match t {
        Target::Player => "player",
        Target::Minions => "minions",
        Target::Boss => "boss",
    }
}

type Counts = BTreeMap<String, usize>;

fn counts<'a>(ids: impl Iterator<Item = &'a str>) -> Counts {
    let mut c = Counts::new();
    for id in ids {
        *c.entry(id.to_string()).or_default() += 1;
    }
    c
}

/// Multiset difference `a - b`, expanded and sorted.
fn minus(a: &Counts, b: &Counts) -> Vec<String> {
    let mut out = Vec::new();
    for (id, &n) in a {
        let left = n.saturating_sub(b.get(id).copied().unwrap_or(0));
        out.extend(std::iter::repeat_n(id.clone(), left));
    }
    out
}

fn py_list(items: &[String]) -> String {
    let inner: Vec<String> = items.iter().map(|s| format!("'{s}'")).collect();
    format!("[{}]", inner.join(", "))
}

pub fn evaluate(screens: &Path, labels: &Path) -> anyhow::Result<Report> {
    let text = std::fs::read_to_string(labels).with_context(|| format!("reading {}", labels.display()))?;
    let labels: Value = serde_json::from_str(&text).context("parsing labels")?;
    let reader = Reader::new()?;

    let mut paths: Vec<PathBuf> = std::fs::read_dir(screens)
        .with_context(|| format!("listing {}", screens.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            let n = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            n.starts_with("screen_") && n.ends_with(".png")
        })
        .collect();
    paths.sort();

    let (mut altars_total, mut altars_found, mut options_total, mut options_ok, mut false_altars) = (0, 0, 0, 0, 0);
    let mut timings = Vec::new();
    let mut failures = Vec::new();

    for path in &paths {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
        let img = Rgb::load(path)?;
        let t = Instant::now();
        let altars = reader.read_altars(&img)?;
        timings.push(t.elapsed().as_secs_f64() * 1000.0);

        let mut unmatched: Vec<&AltarRead> = altars.iter().collect();
        let expected = labels.get(&name).and_then(Value::as_array).cloned().unwrap_or_default();
        for exp in &expected {
            altars_total += 1;
            let (ex, ey) = (exp["icon"][0].as_i64().unwrap_or(0) as i32, exp["icon"][1].as_i64().unwrap_or(0) as i32);
            let hit_idx = unmatched.iter().position(|a| (a.icon.x - ex).abs() <= ICON_TOLERANCE && (a.icon.y - ey).abs() <= ICON_TOLERANCE);
            let hit = hit_idx.map(|i| unmatched.remove(i));
            if hit.is_some() {
                altars_found += 1;
            }
            for (position, pos) in [("top", Pos::Top), ("bottom", Pos::Bottom)] {
                let want = &exp[position];
                let Some(want_mods) = want["mods"].as_array() else { continue };
                options_total += 1;
                let want_target = want["target"].as_str().unwrap_or("");
                let want_ids = counts(want_mods.iter().filter_map(Value::as_str));
                let got = hit.and_then(|a| a.options.iter().find(|o| o.position == pos));
                let got_ids = match got {
                    Some(o) => counts(o.matches.iter().map(|m| reader.cards[m.card].id.as_str())),
                    None => Counts::new(),
                };
                let got_target = got.and_then(|o| o.target).map(target_word);
                if got.is_some() && got_target == Some(want_target) && got_ids == want_ids {
                    options_ok += 1;
                } else {
                    failures.push(format!(
                        "{name} [{ex}, {ey}] {position}: target {} (want {want_target}), missing {}, extra {}",
                        got_target.unwrap_or(if got.is_some() { "None" } else { "-" }),
                        py_list(&minus(&want_ids, &got_ids)),
                        py_list(&minus(&got_ids, &want_ids)),
                    ));
                }
            }
        }
        false_altars += unmatched.len();
    }

    timings.sort_by(|a, b| a.total_cmp(b));
    Ok(Report {
        altars_found,
        altars_total,
        false_altars,
        options_ok,
        options_total,
        median_ms: timings.get(timings.len() / 2).copied().unwrap_or(0.0),
        max_ms: timings.last().copied().unwrap_or(0.0),
        failures,
    })
}
