use crate::log::{Area, AreaKind, LogWatcher, log_files};
use crate::window::{PoeWindow, find_poe_window, refresh_window};
use std::path::PathBuf;
use std::time::{Duration, Instant};

#[derive(Clone, Debug, Default)]
pub struct GameContext {
    pub poe: Option<PoeWindow>,
    pub area: Option<Area>,
}

impl GameContext {
    pub fn poe_active(&self) -> bool {
        self.poe.as_ref().is_some_and(|w| w.foreground && !w.minimized)
    }

    pub fn in_map(&self) -> bool {
        self.area.as_ref().is_some_and(|a| a.kind == AreaKind::Map)
    }
}

pub trait ContextSource: Send {
    fn current(&mut self) -> GameContext;
}

pub struct FixedContext(pub GameContext);

impl ContextSource for FixedContext {
    fn current(&mut self) -> GameContext {
        self.0.clone()
    }
}

const LOOKUP_INTERVAL: Duration = Duration::from_secs(1);

pub struct LiveContext {
    log_override: Option<PathBuf>,
    window: Option<PoeWindow>,
    last_lookup: Option<Instant>,
    watcher: Option<LogWatcher>,
    area: Option<Area>,
}

impl LiveContext {
    pub fn new(log_override: Option<PathBuf>) -> Self {
        Self { log_override, window: None, last_lookup: None, watcher: None, area: None }
    }

    fn ensure_watcher(&mut self) {
        if self.watcher.is_some() {
            return;
        }
        let path = match (&self.log_override, &self.window) {
            (Some(p), _) => Some(p.clone()),
            (None, Some(w)) => w
                .exe
                .parent()
                .and_then(|dir| log_files(dir).into_iter().find(|p| p.exists())),
            _ => None,
        };
        if let Some(p) = path {
            self.watcher = Some(LogWatcher::new(p));
        }
    }
}

impl ContextSource for LiveContext {
    fn current(&mut self) -> GameContext {
        // Cheap refresh through the cached hwnd; drop the cache if the window is gone.
        if let Some(w) = &self.window {
            self.window = refresh_window(w);
        }
        let due = self.last_lookup.is_none_or(|t| t.elapsed() >= LOOKUP_INTERVAL);
        if self.window.is_none() && due {
            self.last_lookup = Some(Instant::now());
            self.window = find_poe_window();
        }
        self.ensure_watcher();
        if let Some(wt) = self.watcher.as_mut() {
            if let Some(a) = wt.poll() {
                self.area = Some(a);
            }
        }
        GameContext { poe: self.window.clone(), area: self.area.clone() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::log::{Area, AreaKind};
    use crate::window::{PoeWindow, Rect, find_poe_window};
    use std::path::PathBuf;

    fn win(foreground: bool, minimized: bool) -> PoeWindow {
        PoeWindow {
            hwnd: 1,
            pid: 1,
            exe: PathBuf::from("PathOfExile.exe"),
            client: Rect { x: 0, y: 0, w: 1920, h: 1080 },
            foreground,
            minimized,
        }
    }
    fn area(kind: AreaKind) -> Option<Area> {
        Some(Area { id: "x".into(), level: 1, kind })
    }

    #[test]
    fn in_map_and_poe_active_rules() {
        let cases: Vec<(GameContext, bool, bool)> = vec![
            (GameContext { poe: None, area: area(AreaKind::Map) }, false, true),
            (GameContext { poe: Some(win(true, true)), area: area(AreaKind::Map) }, false, true),
            (GameContext { poe: Some(win(false, false)), area: area(AreaKind::Map) }, false, true),
            (GameContext { poe: Some(win(true, false)), area: area(AreaKind::Map) }, true, true),
            (GameContext { poe: Some(win(true, false)), area: None }, true, false),
            (GameContext { poe: Some(win(true, false)), area: area(AreaKind::Hideout) }, true, false),
        ];
        for (i, (c, active, map)) in cases.iter().enumerate() {
            assert_eq!(c.poe_active(), *active, "case {i} poe_active");
            assert_eq!(c.in_map(), *map, "case {i} in_map");
        }
    }

    #[test]
    fn fixed_context_returns_its_value() {
        let mut f = FixedContext(GameContext { poe: Some(win(true, false)), area: area(AreaKind::Map) });
        let c = f.current();
        assert!(c.poe_active() && c.in_map());
    }

    #[test]
    fn find_poe_window_is_none_or_consistent() {
        if let Some(w) = find_poe_window() {
            assert!(w.minimized || w.client.w > 0);
            assert!(w.exe.to_string_lossy().to_lowercase().ends_with(".exe"));
        }
    }
}
