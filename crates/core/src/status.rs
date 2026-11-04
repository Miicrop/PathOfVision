//! What the runner currently sees, shared by the app and the console tool.
use pov_game::{AreaKind, GameContext};
use serde::Serialize;

use crate::resolution::{Support, support};

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PoeState {
    NotFound,
    Background,
    Foreground,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RunnerStatus {
    pub poe: PoeState,
    /// Area id from the game log.
    pub area: Option<String>,
    /// `map`, `hideout`, `town` or `other`.
    pub area_kind: Option<String>,
    /// Ids of the modules currently scanning.
    pub active: Vec<String>,
    /// Support level of the game client's size; `None` without a PoE window.
    pub resolution: Option<Support>,
    /// Client size (width, height) in px; `None` without a PoE window.
    pub client_size: Option<(u32, u32)>,
    /// `describe()` of the other fields.
    pub text: String,
}

impl RunnerStatus {
    pub fn from_context(ctx: &GameContext, active: Vec<String>, try_anyway: bool) -> RunnerStatus {
        let poe = match &ctx.poe {
            None => PoeState::NotFound,
            Some(w) if w.minimized || !w.foreground => PoeState::Background,
            Some(_) => PoeState::Foreground,
        };
        let area_kind = ctx.area.as_ref().map(|a| {
            match a.kind {
                AreaKind::Map => "map",
                AreaKind::Hideout => "hideout",
                AreaKind::Town => "town",
                AreaKind::Other => "other",
            }
            .to_string()
        });
        let client_size = ctx.poe.as_ref().map(|w| (w.client.w.max(0) as u32, w.client.h.max(0) as u32));
        let resolution = client_size.map(|(w, h)| support(w, h, try_anyway));
        let mut s = RunnerStatus {
            poe,
            area: ctx.area.as_ref().map(|a| a.id.clone()),
            area_kind,
            active,
            resolution,
            client_size,
            text: String::new(),
        };
        s.text = s.describe();
        s
    }

    pub fn describe(&self) -> String {
        match self.poe {
            PoeState::NotFound => return "PoE not found".into(),
            PoeState::Background => return "PoE in background \u{2013} idle".into(),
            PoeState::Foreground => {}
        }
        let place = match self.area_kind.as_deref() {
            None => "area unknown",
            Some("map") => "in map",
            Some("hideout") => "in hideout",
            Some("town") => "in town",
            Some(_) => "outside maps",
        };
        if !self.active.is_empty() {
            format!("{place} \u{2013} scanning")
        } else if matches!(self.area_kind.as_deref(), None | Some("map")) {
            format!("{place} \u{2013} idle (no module enabled)")
        } else {
            format!("{place} \u{2013} idle")
        }
    }
}

#[cfg(test)]
mod tests {
    use pov_game::{Area, PoeWindow, Rect};

    use super::*;

    fn window(foreground: bool, minimized: bool) -> PoeWindow {
        PoeWindow {
            hwnd: 1,
            pid: 1,
            exe: "poe.exe".into(),
            client: Rect { x: 0, y: 0, w: 1, h: 1 },
            foreground,
            minimized,
        }
    }

    fn ctx(poe: Option<PoeWindow>, kind: Option<AreaKind>) -> GameContext {
        GameContext { poe, area: kind.map(|kind| Area { id: "A1".into(), level: 1, kind }) }
    }

    fn text(c: &GameContext, active: &[&str]) -> String {
        RunnerStatus::from_context(c, active.iter().map(|s| s.to_string()).collect(), false).describe()
    }

    #[test]
    fn status_describes_every_state() {
        let fg = Some(window(true, false));
        assert_eq!(text(&ctx(None, None), &[]), "PoE not found");
        assert_eq!(text(&ctx(Some(window(false, false)), Some(AreaKind::Map)), &[]), "PoE in background \u{2013} idle");
        assert_eq!(text(&ctx(Some(window(true, true)), None), &[]), "PoE in background \u{2013} idle");
        assert_eq!(text(&ctx(fg.clone(), Some(AreaKind::Map)), &["altar"]), "in map \u{2013} scanning");
        assert_eq!(text(&ctx(fg.clone(), None), &["altar"]), "area unknown \u{2013} scanning");
        assert_eq!(text(&ctx(fg.clone(), Some(AreaKind::Hideout)), &[]), "in hideout \u{2013} idle");
        assert_eq!(text(&ctx(fg.clone(), Some(AreaKind::Town)), &[]), "in town \u{2013} idle");
        assert_eq!(text(&ctx(fg.clone(), Some(AreaKind::Other)), &[]), "outside maps \u{2013} idle");
        assert_eq!(text(&ctx(fg.clone(), Some(AreaKind::Map)), &[]), "in map \u{2013} idle (no module enabled)");
        assert_eq!(text(&ctx(fg, None), &[]), "area unknown \u{2013} idle (no module enabled)");
    }

    #[test]
    fn from_context_fills_the_fields() {
        let s = RunnerStatus::from_context(&ctx(Some(window(true, false)), Some(AreaKind::Map)), vec!["altar".into()], false);
        assert_eq!(s.poe, PoeState::Foreground);
        assert_eq!((s.area.as_deref(), s.area_kind.as_deref()), (Some("A1"), Some("map")));
        assert_eq!(s.text, s.describe());
        let v = serde_json::to_value(&s).unwrap();
        assert_eq!(v["poe"], "foreground");
        assert_eq!(serde_json::to_value(PoeState::NotFound).unwrap(), "not_found");
    }
}
