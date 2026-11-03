//! Drives modules: reads the game context, captures a frame only while a module needs one
//! and hands it to every module whose interval has elapsed.
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use pov_capture::FrameSource;
use pov_game::ContextSource;

use crate::module::{Module, ModuleOutput};

const IDLE_POLL: Duration = Duration::from_millis(500);
/// Longest single sleep, so `stop` is noticed promptly.
const SLEEP_SLICE: Duration = Duration::from_millis(100);

struct Slot {
    module: Box<dyn Module>,
    active: bool,
    last_run: Option<Instant>,
    error_logged: bool,
}

impl Slot {
    fn due(&self, now: Instant) -> bool {
        self.active && self.last_run.is_none_or(|t| now.saturating_duration_since(t) >= self.module.interval())
    }
}

pub struct Runner {
    slots: Vec<Slot>,
    context: Box<dyn ContextSource>,
    frames: Box<dyn FrameSource>,
    now: Box<dyn FnMut() -> Instant + Send>,
    paused: bool,
    grab_error_logged: bool,
}

impl Runner {
    pub fn new(modules: Vec<Box<dyn Module>>, context: Box<dyn ContextSource>, frames: Box<dyn FrameSource>) -> Runner {
        let slots = modules
            .into_iter()
            .map(|module| Slot { module, active: false, last_run: None, error_logged: false })
            .collect();
        Runner { slots, context, frames, now: Box::new(Instant::now), paused: false, grab_error_logged: false }
    }

    /// Replaces the clock (for tests).
    pub fn with_clock(mut self, now: impl FnMut() -> Instant + Send + 'static) -> Runner {
        self.now = Box::new(now);
        self
    }

    /// One step: returns the outputs of the modules that ran.
    pub fn tick(&mut self) -> Vec<ModuleOutput> {
        let ctx = self.context.current();
        let now = (self.now)();
        let mut any_active = false;
        for slot in &mut self.slots {
            let active = ctx.poe.is_some() && slot.module.wants_frames(&ctx);
            if slot.active && !active {
                slot.module.on_inactive();
                slot.last_run = None;
            }
            slot.active = active;
            any_active |= active;
        }
        let Some(window) = ctx.poe.as_ref().filter(|_| any_active) else {
            if !self.paused {
                self.frames.pause();
                self.paused = true;
            }
            return Vec::new();
        };
        self.paused = false;

        if !self.slots.iter().any(|s| s.due(now)) {
            return Vec::new();
        }
        let frame = match self.frames.grab(window) {
            Ok(Some(frame)) => {
                self.grab_error_logged = false;
                frame
            }
            Ok(None) => {
                if !self.grab_error_logged {
                    eprintln!("capture: no frame available yet");
                    self.grab_error_logged = true;
                }
                return Vec::new();
            }
            Err(e) => {
                if !self.grab_error_logged {
                    eprintln!("capture: grab failed: {e:#}");
                    self.grab_error_logged = true;
                }
                return Vec::new();
            }
        };

        let mut outputs = Vec::new();
        for slot in &mut self.slots {
            if !slot.due(now) {
                continue;
            }
            slot.last_run = Some(now);
            match slot.module.on_frame(&frame, &ctx) {
                Ok(out) => {
                    slot.error_logged = false;
                    outputs.push(out);
                }
                Err(e) => {
                    if !slot.error_logged {
                        eprintln!("module {}: {e:#}", slot.module.id());
                        slot.error_logged = true;
                    }
                }
            }
        }
        outputs
    }

    /// Loops `tick` until `stop` is set, delivering non-empty results to `sink`.
    pub fn run(&mut self, stop: &AtomicBool, sink: &mut dyn FnMut(&[ModuleOutput])) {
        while !stop.load(Ordering::Relaxed) {
            let outputs = self.tick();
            if !outputs.is_empty() {
                sink(&outputs);
            }
            let wait = self.slots.iter().filter(|s| s.active).map(|s| s.module.interval()).min().unwrap_or(IDLE_POLL);
            let end = Instant::now() + wait;
            while !stop.load(Ordering::Relaxed) {
                let left = end.saturating_duration_since(Instant::now());
                if left.is_zero() {
                    break;
                }
                std::thread::sleep(left.min(SLEEP_SLICE));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use pov_capture::FrameSource;
    use pov_game::{Area, AreaKind, ContextSource, GameContext, PoeWindow, Rect};
    use vision::Rgb;

    use super::*;
    use crate::module::{Module, ModuleOutput};

    #[derive(Default)]
    struct Counts {
        frames: usize,
        inactive: usize,
        grabs: usize,
        pauses: usize,
    }
    type Shared = Arc<Mutex<Counts>>;

    struct FakeModule {
        counts: Shared,
        interval: Duration,
        fail: bool,
    }

    impl Module for FakeModule {
        fn id(&self) -> &'static str {
            "fake"
        }
        fn name(&self) -> &'static str {
            "Fake"
        }
        fn interval(&self) -> Duration {
            self.interval
        }
        fn wants_frames(&self, ctx: &GameContext) -> bool {
            ctx.poe_active()
        }
        fn on_frame(&mut self, _f: &Rgb, _c: &GameContext) -> anyhow::Result<ModuleOutput> {
            self.counts.lock().unwrap().frames += 1;
            if self.fail {
                anyhow::bail!("boom");
            }
            Ok(ModuleOutput { module: "fake", lines: vec!["x".into()], view: serde_json::Value::Null })
        }
        fn on_inactive(&mut self) {
            self.counts.lock().unwrap().inactive += 1;
        }
    }

    /// Yields the scripted contexts in order, then repeats the last one.
    struct Scripted(VecDeque<GameContext>);
    impl ContextSource for Scripted {
        fn current(&mut self) -> GameContext {
            if self.0.len() > 1 { self.0.pop_front().unwrap() } else { self.0[0].clone() }
        }
    }

    struct FakeFrames {
        counts: Shared,
        results: VecDeque<Result<Option<Rgb>, String>>,
    }
    impl FrameSource for FakeFrames {
        fn grab(&mut self, _w: &PoeWindow) -> anyhow::Result<Option<Rgb>> {
            self.counts.lock().unwrap().grabs += 1;
            match self.results.pop_front() {
                Some(Ok(f)) => Ok(f),
                Some(Err(e)) => Err(anyhow::anyhow!(e)),
                None => Ok(Some(frame())),
            }
        }
        fn pause(&mut self) {
            self.counts.lock().unwrap().pauses += 1;
        }
    }

    fn frame() -> Rgb {
        Rgb { w: 1, h: 1, px: vec![[0, 0, 0]] }
    }

    fn poe() -> PoeWindow {
        PoeWindow {
            hwnd: 1,
            pid: 1,
            exe: "poe.exe".into(),
            client: Rect { x: 0, y: 0, w: 1, h: 1 },
            foreground: true,
            minimized: false,
        }
    }

    fn active_ctx() -> GameContext {
        GameContext { poe: Some(poe()), area: Some(Area { id: "MapWorldsDunes".into(), level: 83, kind: AreaKind::Map }) }
    }

    fn runner(
        ctxs: Vec<GameContext>,
        results: Vec<Result<Option<Rgb>, String>>,
        interval: Duration,
        fail: bool,
    ) -> (Runner, Shared) {
        let counts = Shared::default();
        let module = FakeModule { counts: counts.clone(), interval, fail };
        let frames = FakeFrames { counts: counts.clone(), results: results.into() };
        let r = Runner::new(vec![Box::new(module)], Box::new(Scripted(ctxs.into())), Box::new(frames));
        (r, counts)
    }

    #[test]
    fn runner_pauses_capture_when_no_module_wants_frames() {
        let ctxs = vec![GameContext::default(), GameContext::default(), active_ctx(), GameContext::default()];
        let (mut r, c) = runner(ctxs, vec![], Duration::ZERO, false);
        assert!(r.tick().is_empty());
        assert!(r.tick().is_empty());
        {
            let c = c.lock().unwrap();
            assert_eq!((c.grabs, c.pauses, c.inactive), (0, 1, 0));
        }
        assert_eq!(r.tick().len(), 1);
        assert_eq!(c.lock().unwrap().grabs, 1);
        assert!(r.tick().is_empty());
        let c = c.lock().unwrap();
        assert_eq!((c.grabs, c.pauses, c.inactive), (1, 2, 1));
    }

    #[test]
    fn runner_survives_frame_errors() {
        let (mut r, _) = runner(vec![active_ctx()], vec![Err("a".into()), Err("b".into())], Duration::ZERO, false);
        assert!(r.tick().is_empty());
        assert!(r.tick().is_empty());
        assert_eq!(r.tick().len(), 1);
    }

    #[test]
    fn module_error_yields_no_output_but_keeps_running() {
        let (mut r, c) = runner(vec![active_ctx()], vec![], Duration::ZERO, true);
        assert!(r.tick().is_empty());
        assert!(r.tick().is_empty());
        assert_eq!(c.lock().unwrap().frames, 2);
    }

    #[test]
    fn interval_is_respected() {
        let t0 = Instant::now();
        let (r, c) = runner(vec![active_ctx()], vec![], Duration::from_secs(3600), false);
        let mut step = 0u64;
        let mut r = r.with_clock(move || {
            step += 1;
            t0 + Duration::from_secs(step)
        });
        let total: usize = (0..5).map(|_| r.tick().len()).sum();
        assert_eq!(total, 1);
        let c = c.lock().unwrap();
        assert_eq!((c.frames, c.grabs), (1, 1));
    }
}
