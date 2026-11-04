//! The HUD's view model: everything the page needs, ready to render with `textContent`.
//! Port of the view logic of `prototype/overlay.py`.

use std::collections::HashMap;

use pov_core::{AltarSettings, Support, support};
use serde::Serialize;
use vision::Scale;

use crate::data::{AltarKind, Card, Side, Target};
use crate::labels::{Label, RolledValue, Segment, KeywordMatcher, render_alias, rolled_values};
use crate::live::AltarView;
use crate::verdict::{Pos, Tier, Verdict};

const MAX_ALTARS: usize = 2;
const MAX_LINE: usize = 64;
const NO_PICK_COLOR: &str = "#e0a33a";

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct HudStyle {
    pub font_size: i64,
    pub tiers: bool,
    pub other_opacity: f64,
    pub values: bool,
    /// Client height over 1440: the page multiplies every size by it.
    pub scale: f64,
    /// The client size has no verified acceptance run.
    pub untested: bool,
}

impl HudStyle {
    /// `w`/`h`: the client size the frame was taken at; `try_anyway`: `general.try_unsupported_size`.
    pub fn from_settings(s: &AltarSettings, w: usize, h: usize, try_anyway: bool) -> HudStyle {
        HudStyle {
            font_size: s.hud_font_size,
            tiers: s.hud_tiers == "highlight",
            other_opacity: s.hud_other_opacity,
            values: s.hud_values,
            scale: Scale::for_client(w, h).0,
            untested: support(w as u32, h as u32, try_anyway) != Support::Verified,
        }
    }
}

#[derive(Serialize, Debug)]
pub struct HudView {
    pub style: HudStyle,
    pub altars: Vec<HudAltar>,
}

#[derive(Serialize, Debug)]
pub struct HudAltar {
    pub altar: &'static str,
    pub complete: bool,
    pub recommendation: HudRec,
    /// Top, bottom.
    pub options: Vec<HudOption>,
}

#[derive(Serialize, Debug)]
pub struct HudRec {
    pub text: String,
    pub color: &'static str,
    pub arrow: &'static str,
}

#[derive(Serialize, Debug)]
pub struct HudOption {
    pub position: &'static str,
    pub target_icon: Option<String>,
    pub verdict: &'static str,
    pub color: &'static str,
    pub chosen: bool,
    pub dimmed: bool,
    pub mods: Vec<HudMod>,
}

#[derive(Serialize, Debug)]
pub struct HudMod {
    pub upside: bool,
    pub icon: Option<String>,
    pub text: Vec<Segment>,
    pub values: Vec<RolledValue>,
    pub tint: Option<&'static str>,
}

fn verdict_color(v: Verdict) -> &'static str {
    match v {
        Verdict::Take => "#4fbf6a",
        Verdict::Consider => "#e0a33a",
        Verdict::Adapt => "#d65db1",
        Verdict::Avoid => "#e05a4f",
        Verdict::Neutral => "#9aa0a6",
        Verdict::Unreadable => "#5d6168",
    }
}

fn tier_tint(t: Option<&Tier>) -> Option<&'static str> {
    match t? {
        Tier::Top => Some("#f2c84b"),
        Tier::Rated => Some("#b48cf0"),
        Tier::Dangerous => Some("#e0a33a"),
        Tier::Brick => Some("#e05a4f"),
        Tier::None => None,
    }
}

fn pos_name(p: Pos) -> &'static str {
    match p {
        Pos::Top => "top",
        Pos::Bottom => "bottom",
    }
}

/// The (at most 2) icons closest to `center` (Manhattan distance), left to right.
pub fn pick_altars(icons: &[(i32, i32)], center: (i32, i32)) -> Vec<usize> {
    let mut order: Vec<usize> = (0..icons.len()).collect();
    order.sort_by_key(|&i| (icons[i].0 - center.0).abs() + (icons[i].1 - center.1).abs());
    order.truncate(MAX_ALTARS);
    order.sort_by_key(|&i| icons[i].0);
    order
}

/// An option is dimmed when the pick is the other one, unless it is ADAPT - BRICK (its S tier
/// upside is worth a look); when both options are AVOID ("Leave this altar alone") both are dimmed.
pub fn is_dimmed(pick: Option<Pos>, position: Pos, verdict: Verdict, leave_alone: bool) -> bool {
    (pick.is_some() && pick != Some(position) && verdict != Verdict::Adapt) || leave_alone
}

/// The alias with the read values filled in, or the full mod text if the card has no alias.
fn mod_text(alias: Option<&str>, display: &[String]) -> String {
    let text = match alias {
        Some(a) if !a.is_empty() => render_alias(a, display),
        _ => display.join(" / "),
    };
    if text.chars().count() <= MAX_LINE {
        return text;
    }
    let cut: String = text.chars().take(MAX_LINE - 1).collect();
    format!("{cut}…")
}

fn plain(text: &str) -> Vec<Segment> {
    if text.is_empty() { Vec::new() } else { vec![Segment { text: text.to_string(), color: None }] }
}

fn target_icon(target: Option<Target>, position: Pos, altar: AltarKind) -> Option<String> {
    let target = match target? {
        Target::Player => "player",
        Target::Minions => "minions",
        Target::Boss => "boss",
    };
    let kind = match altar {
        AltarKind::Eater => "eater",
        AltarKind::Exarch => "exarch",
    };
    Some(format!("target_{target}_{}_{kind}.png", pos_name(position)))
}

fn hud_altar(view: &AltarView, cards: &[Card], labels: &HashMap<String, Label>, keywords: &KeywordMatcher) -> HudAltar {
    let verdict_at = |pos: Pos| (0..2).find(|&i| view.altar.options[i].position == pos).map(|i| view.verdicts[i].verdict);
    let pick = view.recommendation.choice.or(view.recommendation.about);
    let leave_alone = view.verdicts.iter().all(|v| v.verdict == Verdict::Avoid);

    let mut order = [0usize, 1];
    order.sort_by_key(|&i| view.altar.options[i].position == Pos::Bottom);
    let options = order
        .into_iter()
        .map(|i| {
            let read = &view.altar.options[i];
            let verdict = &view.verdicts[i];
            let mut matches: Vec<_> = read.matches.iter().collect();
            matches.sort_by_key(|m| cards[m.card].side != Side::Upside); // stable: upsides first
            let mods = matches
                .into_iter()
                .map(|m| {
                    let card = &cards[m.card];
                    let label = labels.get(&card.id);
                    let display = m.display(cards, &read.lines);
                    let text = mod_text(label.map(|l| l.alias.as_str()), &display);
                    let upside = card.side == Side::Upside;
                    // Keyword colors only on downsides: upsides like "Chaos Orb" would light up as Chaos.
                    let segments = if upside { plain(&text) } else { keywords.color(&text) };
                    HudMod {
                        upside,
                        icon: label.filter(|l| !l.icon.is_empty()).map(|l| format!("mods/{}.png", l.icon)),
                        text: segments,
                        values: rolled_values(&card.lines[m.variant], &display),
                        tint: tier_tint(verdict.tiers.get(&card.id)),
                    }
                })
                .collect();
            HudOption {
                position: pos_name(read.position),
                target_icon: target_icon(read.target, read.position, view.altar.altar),
                verdict: verdict.verdict.label(),
                color: verdict_color(verdict.verdict),
                chosen: pick == Some(read.position),
                dimmed: is_dimmed(pick, read.position, verdict.verdict, leave_alone),
                mods,
            }
        })
        .collect();

    let (mut color, mut arrow) = match pick {
        Some(p) => (
            verdict_at(p).map_or(NO_PICK_COLOR, verdict_color),
            if p == Pos::Top { "▲" } else { "▼" },
        ),
        None => (NO_PICK_COLOR, ""),
    };
    if leave_alone {
        (color, arrow) = (verdict_color(Verdict::Avoid), "⚠");
    }
    HudAltar {
        altar: match view.altar.altar {
            AltarKind::Eater => "eater",
            AltarKind::Exarch => "exarch",
        },
        complete: view.complete,
        recommendation: HudRec { text: view.recommendation.text.clone(), color, arrow },
        options,
    }
}

/// The view model for the HUD: the 2 altars closest to `center`, left to right.
pub fn hud_view(
    views: &[AltarView],
    cards: &[Card],
    labels: &HashMap<String, Label>,
    keywords: &KeywordMatcher,
    style: HudStyle,
    center: (i32, i32),
) -> HudView {
    let icons: Vec<(i32, i32)> = views.iter().map(|v| (v.altar.icon.x, v.altar.icon.y)).collect();
    let altars = pick_altars(&icons, center).into_iter().map(|i| hud_altar(&views[i], cards, labels, keywords)).collect();
    HudView { style, altars }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pick_two_closest_then_left_to_right() {
        let icons = [(100, 500), (1700, 700), (1800, 720), (3000, 700)];
        assert_eq!(pick_altars(&icons, (1720, 720)), vec![1, 2]);
        assert_eq!(pick_altars(&[(900, 0), (100, 0)], (0, 0)), vec![1, 0]);
        assert!(pick_altars(&[], (0, 0)).is_empty());
    }

    #[test]
    fn dimming_rules() {
        let top = Some(Pos::Top);
        assert!(is_dimmed(top, Pos::Bottom, Verdict::Neutral, false));
        assert!(!is_dimmed(top, Pos::Bottom, Verdict::Adapt, false));
        assert!(!is_dimmed(top, Pos::Top, Verdict::Take, false));
        assert!(is_dimmed(top, Pos::Top, Verdict::Avoid, true));
        assert!(is_dimmed(None, Pos::Top, Verdict::Avoid, true));
        assert!(!is_dimmed(None, Pos::Bottom, Verdict::Neutral, false));
    }

    #[test]
    fn long_text_is_cut_to_64_chars() {
        let long = "x".repeat(80);
        let cut = mod_text(None, &[long]);
        assert_eq!(cut.chars().count(), 64);
        assert!(cut.ends_with('…'));
        assert_eq!(mod_text(Some("Qty"), &["whatever 5".to_string()]), "Qty");
        assert_eq!(mod_text(Some(""), &["a".to_string(), "b".to_string()]), "a / b");
    }
}
