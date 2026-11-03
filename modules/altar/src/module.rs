//! The altar module for the runner: detect icons, track and read altars, judge them with the
//! active profile.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use pov_core::{AppPaths, Module, ModuleOutput, SettingsStore, active_profile_path};
use pov_game::GameContext;
use serde_json::{Value, json};
use vision::Rgb;

use crate::data::{AltarKind, Target};
use crate::detect::assets_dir;
use crate::live::{AltarView, ProfileCache, Tracker};
use crate::read::Reader;
use crate::verdict::Pos;

const ID: &str = "altar";
const INTERVAL: Duration = Duration::from_millis(250);

pub struct AltarModule {
    paths: AppPaths,
    settings: Arc<Mutex<SettingsStore>>,
    reader: Reader,
    tracker: Tracker,
    profiles: ProfileCache,
    start: Instant,
}

impl AltarModule {
    pub fn new(paths: &AppPaths, settings: Arc<Mutex<SettingsStore>>) -> anyhow::Result<AltarModule> {
        let reader = Reader::with_paths(&assets_dir(), &paths.data.join("mods.json"), "en-US")?;
        let fallback: PathBuf = paths.shipped_profiles_dir(ID).join("default.json");
        Ok(AltarModule {
            paths: paths.clone(),
            settings,
            reader,
            tracker: Tracker::new(),
            profiles: ProfileCache::new(fallback),
            start: Instant::now(),
        })
    }

    fn store(&self) -> MutexGuard<'_, SettingsStore> {
        // A panic elsewhere must not take the scanner down; the settings stay readable.
        self.settings.lock().unwrap_or_else(|e| e.into_inner())
    }
}

fn kind_name(k: AltarKind) -> &'static str {
    match k {
        AltarKind::Eater => "Eater",
        AltarKind::Exarch => "Exarch",
    }
}

fn pos_name(p: Pos) -> &'static str {
    match p {
        Pos::Top => "top",
        Pos::Bottom => "bottom",
    }
}

fn target_name(t: Option<Target>) -> Option<&'static str> {
    t.map(|t| match t {
        Target::Player => "player",
        Target::Minions => "minions",
        Target::Boss => "boss",
    })
}

fn option_ids(v: &AltarView, i: usize) -> Vec<String> {
    let o = &v.options[i];
    o.upsides.iter().chain(&o.downsides).cloned().collect()
}

/// `"<altar> top: <verdict> [<ids>] | bottom: <verdict> [<ids>] -> <recommendation>"`
pub fn view_line(v: &AltarView) -> String {
    let side = |i: usize| format!("{} [{}]", v.verdicts[i].verdict.label(), option_ids(v, i).join(", "));
    format!(
        "{} top: {} | bottom: {} -> {}{}",
        kind_name(v.altar.altar),
        side(0),
        side(1),
        v.recommendation.text,
        completeness_suffix(v.complete)
    )
}

/// Marks an altar whose text was only partly readable.
pub fn completeness_suffix(complete: bool) -> &'static str {
    if complete { "" } else { " (partly hidden)" }
}

pub fn view_json(views: &[AltarView]) -> Value {
    let altars: Vec<Value> = views
        .iter()
        .map(|v| {
            let options: Vec<Value> = (0..2)
                .map(|i| {
                    let o = &v.options[i];
                    json!({
                        "position": pos_name(v.altar.options[i].position),
                        "target": target_name(o.target),
                        "upsides": o.upsides,
                        "downsides": o.downsides,
                        "verdict": v.verdicts[i].verdict.label(),
                    })
                })
                .collect();
            json!({
                "altar": kind_name(v.altar.altar),
                "x": v.altar.icon.x,
                "y": v.altar.icon.y,
                "complete": v.complete,
                "options": options,
                "recommendation": {
                    "text": v.recommendation.text,
                    "choice": v.recommendation.choice.map(pos_name),
                },
            })
        })
        .collect();
    json!({ "altars": altars })
}

impl Module for AltarModule {
    fn id(&self) -> &'static str {
        ID
    }

    fn name(&self) -> &'static str {
        "Altars"
    }

    fn interval(&self) -> Duration {
        INTERVAL
    }

    fn wants_frames(&self, ctx: &GameContext) -> bool {
        // Reloaded here too: while idle no `on_frame` runs, and re-enabling must still take effect.
        let enabled = {
            let mut store = self.store();
            store.reload_if_changed();
            store.get().altar.enabled
        };
        enabled && ctx.poe_active() && (ctx.area.is_none() || ctx.in_map())
    }

    fn on_frame(&mut self, frame: &Rgb, _ctx: &GameContext) -> anyhow::Result<ModuleOutput> {
        let profile_path = {
            let mut store = self.store();
            store.reload_if_changed();
            active_profile_path(&self.paths, store.get())
        };
        let now = self.start.elapsed().as_secs_f64();
        let icons = self.reader.detector.find_icons(&frame.to_gray());
        let reader = &self.reader;
        self.tracker.scan(now, &icons, &reader.cards, &mut |icon| reader.read_altar(frame, icon));
        let profile = self.profiles.get(&profile_path);
        let views = self.tracker.views(&self.reader.cards, profile);
        Ok(ModuleOutput { module: ID, lines: views.iter().map(view_line).collect(), view: view_json(&views) })
    }

    fn on_inactive(&mut self) {
        self.tracker.clear();
    }
}
