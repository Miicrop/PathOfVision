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

type Finder = Box<dyn FnMut() -> Option<PoeWindow> + Send>;
type Refresher = Box<dyn Fn(&PoeWindow) -> Option<PoeWindow> + Send>;

pub struct LiveContext {
    log_override: Option<PathBuf>,
    find: Finder,
    refresh: Refresher,
    lookup_interval: Duration,
    window: Option<PoeWindow>,
    last_lookup: Option<Instant>,
    watcher: Option<LogWatcher>,
    /// Install dir the watcher was built for (without override).
    watch_dir: Option<PathBuf>,
    area: Option<Area>,
}

impl LiveContext {
    pub fn new(log_override: Option<PathBuf>) -> Self {
        Self::with_window_source(
            log_override,
            Box::new(find_poe_window),
            Box::new(refresh_window),
            LOOKUP_INTERVAL,
        )
    }

    /// Seam for tests: inject the window lookup/refresh.
    fn with_window_source(
        log_override: Option<PathBuf>,
        find: Finder,
        refresh: Refresher,
        lookup_interval: Duration,
    ) -> Self {
        Self {
            log_override,
            find,
            refresh,
            lookup_interval,
            window: None,
            last_lookup: None,
            watcher: None,
            watch_dir: None,
            area: None,
        }
    }

    fn reset_log(&mut self) {
        self.watcher = None;
        self.watch_dir = None;
        self.area = None;
    }

    fn ensure_watcher(&mut self) {
        if self.watcher.is_some() {
            return;
        }
        if let Some(p) = &self.log_override {
            self.watcher = Some(LogWatcher::new(p.clone()));
        } else if let Some(dir) = &self.watch_dir
            && let Some(p) = log_files(dir).into_iter().next()
        {
            self.watcher = Some(LogWatcher::new(p));
        }
    }
}

impl ContextSource for LiveContext {
    fn current(&mut self) -> GameContext {
        // Cheap refresh through the cached hwnd; drop the cache if the window is gone.
        if let Some(w) = &self.window {
            self.window = (self.refresh)(w);
            if self.window.is_none() && self.log_override.is_none() {
                self.reset_log();
            }
        }
        let due = self.last_lookup.is_none_or(|t| t.elapsed() >= self.lookup_interval);
        if self.window.is_none() && due {
            self.last_lookup = Some(Instant::now());
            self.window = (self.find)();
        }
        if self.log_override.is_none()
            && let Some(w) = &self.window
        {
            let dir = w.exe.parent().map(|d| d.to_path_buf());
            if dir != self.watch_dir {
                self.reset_log();
                self.watch_dir = dir;
            }
        }
        self.ensure_watcher();
        self.area = self.watcher.as_mut().and_then(|w| w.poll());
        GameContext { poe: self.window.clone(), area: self.area.clone() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::log::{Area, AreaKind};
    use crate::window::{PoeWindow, Rect, find_poe_window};
    use std::io::Write;
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex};

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

    const MAP: &str = "2026/11/03 17:12:44 1 1 [DEBUG Client 1] Generating level 83 area \"MapWorldsDunes\" with seed 1\n";
    const HIDEOUT: &str = "2026/11/03 17:13:44 1 1 [DEBUG Client 1] Generating level 1 area \"HideoutFelled\" with seed 1\n";

    fn append(p: &Path, s: &str) {
        let mut f = std::fs::OpenOptions::new().create(true).append(true).open(p).unwrap();
        f.write_all(s.as_bytes()).unwrap();
    }

    fn fake_source(slot: Arc<Mutex<Option<PoeWindow>>>) -> LiveContext {
        let s2 = slot.clone();
        LiveContext::with_window_source(
            None,
            Box::new(move || slot.lock().unwrap().clone()),
            Box::new(move |_| s2.lock().unwrap().clone()),
            Duration::ZERO,
        )
    }

    fn win_in(dir: &Path) -> PoeWindow {
        PoeWindow { exe: dir.join("PathOfExile.exe"), ..win(true, false) }
    }

    fn install(dir: &Path, line: &str) {
        std::fs::create_dir_all(dir.join("logs")).unwrap();
        append(&dir.join("logs/LatestClient.txt"), line);
    }

    #[test]
    fn live_override_tracks_log_without_window() {
        let t = tempfile::tempdir().unwrap();
        let log = t.path().join("c.txt");
        append(&log, MAP);
        let mut lc = LiveContext::with_window_source(
            Some(log.clone()),
            Box::new(|| None),
            Box::new(|_| None),
            Duration::ZERO,
        );
        let c = lc.current();
        assert!(!c.poe_active());
        assert!(c.in_map());
        append(&log, HIDEOUT);
        let c = lc.current();
        assert_eq!(c.area.unwrap().kind, AreaKind::Hideout);
    }

    #[test]
    fn live_window_lost_clears_area_and_new_dir_rebuilds_watcher() {
        let t = tempfile::tempdir().unwrap();
        let (a, b) = (t.path().join("a"), t.path().join("b"));
        install(&a, MAP);
        install(&b, HIDEOUT);
        let slot = Arc::new(Mutex::new(Some(win_in(&a))));
        let mut lc = fake_source(slot.clone());
        assert!(lc.current().in_map());
        *slot.lock().unwrap() = None;
        let c = lc.current();
        assert!(c.poe.is_none() && c.area.is_none());
        *slot.lock().unwrap() = Some(win_in(&b));
        let c = lc.current();
        assert_eq!(c.area.unwrap().kind, AreaKind::Hideout);
    }

    #[test]
    fn live_direct_swap_to_other_dir_rebuilds_and_unknown_area_not_stale() {
        let t = tempfile::tempdir().unwrap();
        let (a, b) = (t.path().join("a"), t.path().join("b"));
        install(&a, MAP);
        install(&b, "no area here\n");
        let slot = Arc::new(Mutex::new(Some(win_in(&a))));
        let mut lc = fake_source(slot.clone());
        assert!(lc.current().in_map());
        *slot.lock().unwrap() = Some(win_in(&b));
        let c = lc.current();
        assert!(c.area.is_none());
    }
}
