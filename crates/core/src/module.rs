//! The contract between the runner and a scanner module (e.g. the altar module).
use std::time::Duration;

use pov_game::GameContext;
use vision::Rgb;

/// What a module produced for one frame.
#[derive(Clone, Debug)]
pub struct ModuleOutput {
    pub module: &'static str,
    /// Human-readable lines (log/overlay text).
    pub lines: Vec<String>,
    /// Module-specific data for the overlay.
    pub view: serde_json::Value,
}

pub trait Module: Send {
    fn id(&self) -> &'static str;
    fn name(&self) -> &'static str;
    /// Minimum time between two `on_frame` calls.
    fn interval(&self) -> Duration;
    /// Whether the module needs frames in this context.
    fn wants_frames(&self, ctx: &GameContext) -> bool;
    fn on_frame(&mut self, frame: &Rgb, ctx: &GameContext) -> anyhow::Result<ModuleOutput>;
    /// Called once when the module stops wanting frames.
    fn on_inactive(&mut self);
}
