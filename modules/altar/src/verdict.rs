//! Verdict engine: rates altar options against a profile. Port of `prototype/verdict.py`.

use crate::data::Target;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

const UNRATED_POS: usize = 1_000_000;

fn is_false(b: &bool) -> bool {
    !*b
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Column {
    pub name: String,
    pub ids: Vec<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub key: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Upsides {
    pub columns: Vec<Column>,
    pub top: Column,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpKind {
    Top,
    Rated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DownTier {
    Brick,
    Dangerous,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pos {
    Top,
    Bottom,
}

impl Pos {
    fn other(self) -> Pos {
        match self {
            Pos::Top => Pos::Bottom,
            Pos::Bottom => Pos::Top,
        }
    }
    fn word(self) -> &'static str {
        match self {
            Pos::Top => "top",
            Pos::Bottom => "bottom",
        }
    }
    fn title(self) -> &'static str {
        match self {
            Pos::Top => "Top",
            Pos::Bottom => "Bottom",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Take,
    Consider,
    Adapt,
    Avoid,
    Neutral,
    Unreadable,
}

impl Verdict {
    pub fn label(self) -> &'static str {
        match self {
            Verdict::Take => "TAKE",
            Verdict::Consider => "CONSIDER",
            Verdict::Adapt => "ADAPT - BRICK",
            Verdict::Avoid => "AVOID",
            Verdict::Neutral => "NEUTRAL",
            Verdict::Unreadable => "UNREADABLE",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    Top,
    Rated,
    Dangerous,
    Brick,
    None,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profile {
    pub name: String,
    pub upsides: Upsides,
    pub downsides: HashMap<String, Vec<String>>,
    pub read_only: bool,
}

#[derive(Deserialize, Default)]
struct RawColumn {
    #[serde(default)]
    name: String,
    #[serde(default)]
    ids: Vec<String>,
    #[serde(default)]
    key: bool,
}

#[derive(Deserialize)]
struct RawTop {
    #[serde(default = "default_top_name")]
    name: String,
    #[serde(default)]
    ids: Vec<String>,
}

fn default_top_name() -> String {
    "S".to_string()
}

#[derive(Deserialize, Default)]
struct RawUpsides {
    columns: Option<Vec<RawColumn>>,
    top: Option<RawTop>,
    #[serde(default)]
    s: Vec<String>,
    #[serde(default)]
    a: Vec<String>,
    #[serde(default)]
    b: Vec<String>,
}

#[derive(Deserialize)]
struct RawProfile {
    name: String,
    #[serde(default)]
    upsides: RawUpsides,
    #[serde(default)]
    downsides: HashMap<String, Vec<String>>,
    #[serde(default, rename = "readOnly")]
    read_only: bool,
}

fn upgrade(raw: RawUpsides) -> Upsides {
    if raw.top.is_some() || raw.columns.is_some() {
        let columns = raw
            .columns
            .unwrap_or_default()
            .into_iter()
            .map(|c| Column { name: c.name, ids: c.ids, key: c.key })
            .collect();
        let top = raw.top.map_or_else(
            || Column { name: default_top_name(), ids: vec![], key: false },
            |t| Column { name: t.name, ids: t.ids, key: false },
        );
        Upsides { columns, top }
    } else {
        Upsides {
            columns: vec![
                Column { name: "B".into(), ids: raw.b, key: false },
                Column { name: "A".into(), ids: raw.a, key: false },
            ],
            top: Column { name: "S".into(), ids: raw.s, key: false },
        }
    }
}

impl Profile {
    pub fn from_json(json: &str) -> anyhow::Result<Profile> {
        let raw: RawProfile = serde_json::from_str(json)?;
        Ok(Profile {
            name: raw.name,
            upsides: upgrade(raw.upsides),
            downsides: raw.downsides,
            read_only: raw.read_only,
        })
    }

    pub fn load(path: &Path) -> anyhow::Result<Profile> {
        Self::from_json(&std::fs::read_to_string(path)?)
    }

    /// (kind, rank, position in its column). Rank grows to the right: unrated 0, top highest.
    /// A key column has the kind Top but keeps its rank.
    pub fn up_tier(&self, mod_id: &str) -> (Option<UpKind>, usize, usize) {
        let columns = &self.upsides.columns;
        if let Some(p) = self.upsides.top.ids.iter().position(|i| i == mod_id) {
            return (Some(UpKind::Top), columns.len() + 1, p);
        }
        for (i, col) in columns.iter().enumerate().rev() {
            if let Some(p) = col.ids.iter().position(|x| x == mod_id) {
                let kind = if col.key { UpKind::Top } else { UpKind::Rated };
                return (Some(kind), i + 1, p);
            }
        }
        (None, 0, UNRATED_POS)
    }

    pub fn down_tier(&self, mod_id: &str) -> Option<DownTier> {
        for (key, tier) in [("brick", DownTier::Brick), ("dangerous", DownTier::Dangerous)] {
            if self.downsides.get(key).is_some_and(|l| l.iter().any(|x| x == mod_id)) {
                return Some(tier);
            }
        }
        None
    }
}

#[derive(Debug, Clone)]
pub struct OptionInput {
    pub target: Option<Target>,
    pub upsides: Vec<String>,
    pub downsides: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct OptionVerdict {
    pub verdict: Verdict,
    pub up_tier: Option<UpKind>,
    pub up_pos: usize,
    pub down_tier: Option<DownTier>,
    pub tiers: HashMap<String, Tier>,
    pub up_rank: usize,
}

pub fn judge(option: &OptionInput, profile: &Profile) -> OptionVerdict {
    let mut tiers = HashMap::new();
    let (mut best_tier, mut best_rank, mut best_pos) = (None, 0usize, UNRATED_POS);
    for id in &option.upsides {
        let (kind, rank, pos) = profile.up_tier(id);
        tiers.insert(
            id.clone(),
            match kind {
                Some(UpKind::Top) => Tier::Top,
                Some(UpKind::Rated) => Tier::Rated,
                None => Tier::None,
            },
        );
        // (rank, -pos) > (best_rank, -best_pos)
        if rank > best_rank || (rank == best_rank && pos < best_pos) {
            best_tier = kind;
            best_rank = rank;
            best_pos = pos;
        }
    }

    let mut worst = None;
    for id in &option.downsides {
        let tier = profile.down_tier(id);
        tiers.insert(
            id.clone(),
            match tier {
                Some(DownTier::Brick) => Tier::Brick,
                Some(DownTier::Dangerous) => Tier::Dangerous,
                None => Tier::None,
            },
        );
        if tier == Some(DownTier::Brick) || (tier == Some(DownTier::Dangerous) && worst.is_none()) {
            worst = tier;
        }
    }

    let rated = best_tier.is_some();
    let verdict = if option.upsides.is_empty() && option.downsides.is_empty() {
        Verdict::Unreadable
    } else if worst == Some(DownTier::Brick) {
        if best_tier == Some(UpKind::Top) { Verdict::Adapt } else { Verdict::Avoid }
    } else if worst == Some(DownTier::Dangerous) {
        if rated { Verdict::Consider } else { Verdict::Avoid }
    } else if rated {
        Verdict::Take
    } else {
        Verdict::Neutral
    };
    OptionVerdict { verdict, up_tier: best_tier, up_pos: best_pos, down_tier: worst, tiers, up_rank: best_rank }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recommendation {
    pub choice: Option<Pos>,
    pub text: String,
    pub about: Option<Pos>,
}

fn idx(p: Pos) -> usize {
    match p {
        Pos::Top => 0,
        Pos::Bottom => 1,
    }
}

/// First maximum of `(up_rank, -up_pos)` among `candidates` (ties keep the earlier one).
fn strongest(candidates: &[Pos], verdicts: &[OptionVerdict; 2]) -> Pos {
    let mut best = candidates[0];
    for &p in &candidates[1..] {
        let (a, b) = (&verdicts[idx(p)], &verdicts[idx(best)]);
        if a.up_rank > b.up_rank || (a.up_rank == b.up_rank && a.up_pos < b.up_pos) {
            best = p;
        }
    }
    best
}

pub fn recommend(options: &[OptionInput; 2], verdicts: &[OptionVerdict; 2]) -> Recommendation {
    const ORDER: [Pos; 2] = [Pos::Top, Pos::Bottom];
    let rec = |choice, text: String, about| Recommendation { choice, text, about };

    if verdicts.iter().any(|v| v.verdict == Verdict::Unreadable) {
        return rec(None, "Can't read both options yet.".into(), None);
    }

    let takes: Vec<Pos> = ORDER.into_iter().filter(|&p| verdicts[idx(p)].verdict == Verdict::Take).collect();
    if !takes.is_empty() {
        let mut best = strongest(&takes, verdicts);
        // A CONSIDER whose upside sits further right beats the safe pick.
        let other = best.other();
        if verdicts[idx(other)].verdict == Verdict::Consider && verdicts[idx(other)].up_rank > verdicts[idx(best)].up_rank {
            best = other;
        }
        return rec(Some(best), format!("Take the {} option.", best.word()), None);
    }

    let risky: Vec<Pos> = ORDER
        .into_iter()
        .filter(|&p| matches!(verdicts[idx(p)].verdict, Verdict::Consider | Verdict::Adapt))
        .collect();
    if !risky.is_empty() {
        let best = strongest(&risky, verdicts);
        let reason = if verdicts[idx(best)].verdict == Verdict::Adapt { "Brick downside, good upside." } else { "Be careful." };
        let other = best.other();
        let tail = if verdicts[idx(other)].verdict == Verdict::Neutral { format!(" {} is safe.", other.title()) } else { String::new() };
        return rec(None, format!("{reason}{tail}"), Some(best));
    }

    if verdicts.iter().all(|v| v.verdict == Verdict::Avoid) {
        return rec(None, "Leave this altar alone.".into(), None);
    }

    // Minions is the safe default; fall back to the first neutral option.
    let neutral = ORDER
        .into_iter()
        .find(|&p| options[idx(p)].target == Some(Target::Minions) && verdicts[idx(p)].verdict == Verdict::Neutral)
        .or_else(|| ORDER.into_iter().find(|&p| verdicts[idx(p)].verdict == Verdict::Neutral))
        .expect("a neutral option exists");
    rec(Some(neutral), format!("Nothing rated. Take the {} option.", neutral.word()), None)
}
