//! Scan timing and process CPU usage for the `--stats` line.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::module::{Module, ModuleOutput};
use pov_game::GameContext;
use vision::Rgb;
use windows::Win32::Foundation::FILETIME;
use windows::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};

/// Scans seen since the last report.
#[derive(Default, Debug, Clone, PartialEq)]
pub struct ScanWindow {
    pub scans: u32,
    pub total: Duration,
    pub max: Duration,
    /// Altars in the last scan.
    pub altars: usize,
}

impl ScanWindow {
    pub fn record(&mut self, took: Duration, altars: usize) {
        self.scans += 1;
        self.total += took;
        self.max = self.max.max(took);
        self.altars = altars;
    }
}

pub type SharedScans = Arc<Mutex<ScanWindow>>;

/// Wraps a module and records how long each `on_frame` takes.
pub struct Timed {
    pub inner: Box<dyn Module>,
    pub scans: SharedScans,
}

impl Module for Timed {
    fn id(&self) -> &'static str {
        self.inner.id()
    }
    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn interval(&self) -> Duration {
        self.inner.interval()
    }
    fn wants_frames(&self, ctx: &GameContext) -> bool {
        self.inner.wants_frames(ctx)
    }
    fn on_frame(&mut self, frame: &Rgb, ctx: &GameContext) -> anyhow::Result<ModuleOutput> {
        let t = Instant::now();
        let out = self.inner.on_frame(frame, ctx);
        let altars = out.as_ref().map(|o| o.lines.len()).unwrap_or(0);
        self.scans.lock().unwrap_or_else(|e| e.into_inner()).record(t.elapsed(), altars);
        out
    }
    fn on_inactive(&mut self) {
        self.scans.lock().unwrap_or_else(|e| e.into_inner()).altars = 0;
        self.inner.on_inactive();
    }
}

fn filetime(f: FILETIME) -> Duration {
    // 100 ns units.
    Duration::from_nanos(((u64::from(f.dwHighDateTime) << 32) | u64::from(f.dwLowDateTime)) * 100)
}

/// Kernel + user time of this process.
pub fn process_cpu_time() -> Duration {
    let mut t = [FILETIME::default(); 4];
    let [c, e, k, u] = &mut t;
    // SAFETY: valid out pointers; the pseudo handle needs no closing.
    let ok = unsafe { GetProcessTimes(GetCurrentProcess(), c, e, k, u) };
    if ok.is_err() {
        return Duration::ZERO;
    }
    filetime(t[2]) + filetime(t[3])
}

/// CPU share over `wall`: (% of one core, % of all `cores`).
pub fn cpu_percent(cpu: Duration, wall: Duration, cores: usize) -> (f64, f64) {
    if wall.is_zero() {
        return (0.0, 0.0);
    }
    let one = cpu.as_secs_f64() / wall.as_secs_f64() * 100.0;
    (one, one / cores.max(1) as f64)
}

/// Produces one stats line per call, covering the time since the previous call.
pub struct Reporter {
    scans: SharedScans,
    cores: usize,
    last_wall: Instant,
    last_cpu: Duration,
}

impl Reporter {
    pub fn new(scans: SharedScans) -> Reporter {
        let cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1);
        Reporter { scans, cores, last_wall: Instant::now(), last_cpu: process_cpu_time() }
    }

    pub fn line(&mut self) -> String {
        let (wall_now, cpu_now) = (Instant::now(), process_cpu_time());
        let wall = wall_now - self.last_wall;
        let cpu = cpu_now.saturating_sub(self.last_cpu);
        (self.last_wall, self.last_cpu) = (wall_now, cpu_now);
        let w = {
            let mut s = self.scans.lock().unwrap_or_else(|e| e.into_inner());
            let w = s.clone();
            *s = ScanWindow { altars: s.altars, ..Default::default() };
            w
        };
        format_line(&w, wall, cpu, self.cores)
    }
}

pub fn format_line(w: &ScanWindow, wall: Duration, cpu: Duration, cores: usize) -> String {
    let secs = wall.as_secs_f64().max(f64::EPSILON);
    let avg = if w.scans == 0 { 0.0 } else { w.total.as_secs_f64() * 1000.0 / f64::from(w.scans) };
    let (one, total) = cpu_percent(cpu, wall, cores);
    format!(
        "stats {:.0} s: {:.1} scans/s, scan avg {:.1} ms / max {:.1} ms, CPU {:.2} % of total ({:.1} % of one core, {} cores), altars {}",
        wall.as_secs_f64(),
        f64::from(w.scans) / secs,
        avg,
        w.max.as_secs_f64() * 1000.0,
        total,
        one,
        cores,
        w.altars
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_percent_divides_by_wall_and_cores() {
        let (one, total) = cpu_percent(Duration::from_millis(500), Duration::from_secs(10), 4);
        assert!((one - 5.0).abs() < 1e-9 && (total - 1.25).abs() < 1e-9);
        assert_eq!(cpu_percent(Duration::from_secs(1), Duration::ZERO, 4), (0.0, 0.0));
    }

    #[test]
    fn format_line_reports_rates_and_times() {
        let mut w = ScanWindow::default();
        w.record(Duration::from_millis(10), 2);
        w.record(Duration::from_millis(30), 2);
        let line = format_line(&w, Duration::from_secs(10), Duration::from_millis(200), 2);
        assert_eq!(
            line,
            "stats 10 s: 0.2 scans/s, scan avg 20.0 ms / max 30.0 ms, CPU 1.00 % of total (2.0 % of one core, 2 cores), altars 2"
        );
    }

    #[test]
    fn process_cpu_time_grows() {
        let a = process_cpu_time();
        let t = Instant::now();
        let mut x = 0u64;
        while t.elapsed() < Duration::from_millis(50) {
            x = x.wrapping_add(std::hint::black_box(1));
        }
        assert!(x > 0 && process_cpu_time() > a);
    }
}
