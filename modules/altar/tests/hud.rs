use std::collections::HashMap;
use std::path::PathBuf;

use altar::hud::{HudView, hud_view, HudStyle};
use altar::labels::{KeywordMatcher, load_keywords, load_labels};
use altar::live::Tracker;
use altar::read::Reader;
use altar::verdict::Profile;
use altar::repo_data_dir;
use vision::{Rgb, Scale};

const COLORS: [&str; 6] = ["#4fbf6a", "#e0a33a", "#d65db1", "#e05a4f", "#9aa0a6", "#5d6168"];

fn screen() -> Option<PathBuf> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/screens/screen_02.png");
    if p.exists() {
        Some(p)
    } else {
        eprintln!("skipped: {} missing", p.display());
        None
    }
}

fn view(with_labels: bool) -> Option<(HudView, Vec<altar::Card>)> {
    let path = screen()?;
    let reader = Reader::new().unwrap();
    let img = Rgb::load(&path).unwrap();
    let altars = reader.read_altars(&img).unwrap();
    let mut tracker = Tracker::new();
    let mut queue = altars.clone();
    let icons: Vec<_> = altars.iter().map(|a| a.icon).collect();
    tracker.scan(0.0, &icons, &reader.cards, Scale::for_client(img.w, img.h), &mut |icon| {
        let i = queue.iter().position(|a| a.icon.x == icon.x && a.icon.y == icon.y).unwrap();
        Ok(Some(queue.remove(i)))
    });
    let profile = Profile::load(&repo_data_dir().join("profiles/default.json")).unwrap();
    let views = tracker.views(&reader.cards, &profile);
    let labels = if with_labels { load_labels(&repo_data_dir().join("labels.json")) } else { HashMap::new() };
    let keywords = KeywordMatcher::new(&load_keywords(&repo_data_dir().join("keywords.json")));
    let style = HudStyle { font_size: 15, tiers: true, other_opacity: 0.35, values: true, scale: 1.0, untested: false };
    let hud = hud_view(&views, &reader.cards, &labels, &keywords, style, ((img.w / 2) as i32, (img.h / 2) as i32));
    Some((hud, reader.cards))
}

#[test]
fn hud_view_of_a_screenshot() {
    let Some((hud, _)) = view(true) else { return };
    assert_eq!(hud.altars.len(), 2);
    for a in &hud.altars {
        let positions: Vec<_> = a.options.iter().map(|o| o.position).collect();
        assert_eq!(positions, ["top", "bottom"]);
        assert!(!a.recommendation.text.is_empty());
        for o in &a.options {
            assert!(COLORS.contains(&o.color), "{}", o.color);
            let first_down = o.mods.iter().position(|m| !m.upside).unwrap_or(o.mods.len());
            assert!(o.mods[first_down..].iter().all(|m| !m.upside), "upsides come first");
        }
    }
}

#[test]
fn mod_without_alias_shows_full_text() {
    let Some((hud, _)) = view(false) else { return };
    let mut n = 0;
    for m in hud.altars.iter().flat_map(|a| &a.options).flat_map(|o| &o.mods) {
        let text: String = m.text.iter().map(|s| s.text.as_str()).collect();
        assert!(text.chars().count() <= 64, "{text}");
        assert!(!text.is_empty() && !text.contains('#'), "{text}");
        assert!(m.icon.is_none());
        n += 1;
    }
    assert!(n > 0);
}

#[test]
fn hud_style_carries_scale_and_untested() {
    let s = pov_core::AltarSettings::default();
    let v = serde_json::to_value(HudStyle::from_settings(&s, 3840, 2160, true)).unwrap();
    assert_eq!(v["scale"], 1.5);
    assert_eq!(v["untested"], true);
    let v = serde_json::to_value(HudStyle::from_settings(&s, 1920, 1080, false)).unwrap();
    assert_eq!((v["scale"].as_f64(), v["untested"].as_bool()), (Some(0.75), Some(false)));
    let v = serde_json::to_value(HudStyle::from_settings(&s, 3440, 1440, false)).unwrap();
    assert_eq!((v["scale"].as_f64(), v["untested"].as_bool()), (Some(1.0), Some(false)));
    // Unsupported sizes are untested once the user opts in.
    let v = serde_json::to_value(HudStyle::from_settings(&s, 1280, 720, true)).unwrap();
    assert_eq!(v["untested"], true);
}

use altar::AltarKind;
use altar::detect::Icon;
use altar::live::AltarView;
use altar::read::{AltarRead, OptionRead};
use altar::verdict::{OptionInput, OptionVerdict, Pos, Recommendation, Verdict};

fn synthetic(verdicts: [Verdict; 2], choice: Option<Pos>) -> AltarView {
    let option = |position| OptionRead {
        position,
        target: None,
        lines: vec![],
        matches: vec![],
        unexplained: vec![],
        has_header: true,
    };
    let verdict = |verdict| OptionVerdict {
        verdict,
        up_tier: None,
        up_pos: 0,
        down_tier: None,
        tiers: HashMap::new(),
        up_rank: 0,
    };
    let input = || OptionInput { target: None, upsides: vec![], downsides: vec![] };
    AltarView {
        altar: AltarRead {
            icon: Icon { x: 0, y: 0, score: 1.0 },
            altar: AltarKind::Eater,
            options: [option(Pos::Top), option(Pos::Bottom)],
        },
        complete: true,
        options: [input(), input()],
        verdicts: [verdict(verdicts[0]), verdict(verdicts[1])],
        recommendation: Recommendation { choice, text: "pick".into(), about: None },
    }
}

fn hud_of(view: AltarView) -> HudView {
    let style = HudStyle { font_size: 15, tiers: true, other_opacity: 0.35, values: true, scale: 1.0, untested: false };
    hud_view(&[view], &[], &HashMap::new(), &KeywordMatcher::new(&[]), style, (0, 0))
}

#[test]
fn hud_altar_recommendation_and_dimming() {
    // A firm pick: it is chosen (glow) and bright, the other one is dimmed at hud_other_opacity.
    let hud = hud_of(synthetic([Verdict::Take, Verdict::Neutral], Some(Pos::Top)));
    let a = &hud.altars[0];
    let (top, bottom) = (&a.options[0], &a.options[1]);
    assert_eq!((top.position, bottom.position), ("top", "bottom"));
    assert!(top.chosen && !top.dimmed);
    assert!(!bottom.chosen && bottom.dimmed);
    assert_eq!(a.recommendation.arrow, "\u{25B2}");
    let json = serde_json::to_value(&hud).unwrap();
    assert_eq!(json["style"]["other_opacity"], 0.35);
    assert_eq!(json["altars"][0]["options"][1]["dimmed"], true);

    // ADAPT - BRICK is never dimmed, even when it is not the pick.
    let hud = hud_of(synthetic([Verdict::Take, Verdict::Adapt], Some(Pos::Top)));
    let (top, bottom) = (&hud.altars[0].options[0], &hud.altars[0].options[1]);
    assert_eq!(bottom.verdict, "ADAPT - BRICK");
    assert!(top.chosen && !top.dimmed);
    assert!(!bottom.chosen && !bottom.dimmed);

    // Both AVOID: leave it alone, both dimmed.
    let hud = hud_of(synthetic([Verdict::Avoid, Verdict::Avoid], None));
    assert!(hud.altars[0].options.iter().all(|o| o.dimmed && !o.chosen));
}
