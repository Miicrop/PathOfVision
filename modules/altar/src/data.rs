use std::path::{Path, PathBuf};

use anyhow::Context;
use serde::Deserialize;

use crate::text::normalize;

/// Shared data directory of the repository (dev and tests).
pub fn repo_data_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Target {
    Player,
    Minions,
    Boss,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AltarKind {
    Eater,
    Exarch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    Upside,
    Downside,
}

#[derive(Debug, Clone)]
pub struct Card {
    pub id: String,
    pub side: Side,
    pub target: Target,
    pub altars: Vec<AltarKind>,
    /// Per variant, normalized.
    pub templates: Vec<Vec<String>>,
    pub ranges: Vec<Vec<[f64; 2]>>,
    pub lines: Vec<Vec<String>>,
}

#[derive(Deserialize)]
struct ModsFile {
    mods: Vec<RawMod>,
}

#[derive(Deserialize)]
struct RawMod {
    id: String,
    side: Side,
    target: Target,
    altars: Vec<AltarKind>,
    variants: Vec<RawVariant>,
}

#[derive(Deserialize)]
struct RawVariant {
    templates: Vec<String>,
    ranges: Vec<[f64; 2]>,
    lines: Vec<String>,
}

pub fn load_cards(path: &Path) -> anyhow::Result<Vec<Card>> {
    let text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let file: ModsFile = serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
    Ok(file
        .mods
        .into_iter()
        .map(|m| Card {
            id: m.id,
            side: m.side,
            target: m.target,
            altars: m.altars,
            templates: m
                .variants
                .iter()
                .map(|v| v.templates.iter().map(|t| normalize(t)).collect())
                .collect(),
            ranges: m.variants.iter().map(|v| v.ranges.clone()).collect(),
            lines: m.variants.iter().map(|v| v.lines.clone()).collect(),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_all_cards() {
        let cards = load_cards(&repo_data_dir().join("mods.json")).unwrap();
        assert_eq!(cards.len(), 201);
        assert_eq!(cards[0].id, "CleansingAltarUpsideBossChaosOrb");
        assert_eq!(cards[0].ranges[0], vec![[2.0, 4.0]]);
        assert_eq!(cards[0].templates[0], vec!["FINAL BOSS DROPS # ADDITIONAL CHAOS ORBS"]);
    }
}
