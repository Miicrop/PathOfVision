use std::collections::HashMap;
use std::path::PathBuf;

use altar::hud::{HudView, hud_view, HudStyle};
use altar::labels::{load_keywords, load_labels};
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
    let keywords = load_keywords(&repo_data_dir().join("keywords.json"));
    let style = HudStyle { font_size: 15, tiers: true, other_opacity: 0.6, values: true };
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
