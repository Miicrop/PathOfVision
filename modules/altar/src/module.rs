//! The altar module for the runner: detect icons, track and read altars, judge them with the
//! active profile.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use pov_core::{AppPaths, Module, ModuleOutput, SettingsStore, active_profile_path};
use pov_game::GameContext;
use vision::{Rgb, Scale};

use crate::data::AltarKind;
use crate::hud::{HudStyle, hud_view};
use crate::labels::{Label, Watched, load_keywords, load_labels};
use crate::live::{AltarView, ProfileCache, Tracker};
use crate::read::Reader;

const ID: &str = "altar";
const INTERVAL: Duration = Duration::from_millis(250);

pub struct AltarModule {
    paths: AppPaths,
    settings: Arc<Mutex<SettingsStore>>,
    reader: Reader,
    tracker: Tracker,
    profiles: ProfileCache,
    labels: Watched<HashMap<String, Label>>,
    keywords: Watched<Vec<(String, String)>>,
    start: Instant,
}

impl AltarModule {
    pub fn new(paths: &AppPaths, settings: Arc<Mutex<SettingsStore>>) -> anyhow::Result<AltarModule> {
        let reader = Reader::with_paths(&paths.module_assets(ID), &paths.data.join("mods.json"), "en-US")?;
        let fallback: PathBuf = paths.shipped_profiles_dir(ID).join("default.json");
        Ok(AltarModule {
            paths: paths.clone(),
            settings,
            reader,
            tracker: Tracker::new(),
            profiles: ProfileCache::new(fallback),
            labels: Watched::new(paths.data.join("labels.json"), load_labels),
            keywords: Watched::new(paths.data.join("keywords.json"), load_keywords),
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
        let (profile_path, style) = {
            let mut store = self.store();
            store.reload_if_changed();
            (active_profile_path(&self.paths, store.get()), HudStyle::from_settings(&store.get().altar))
        };
        self.labels.refresh();
        self.keywords.refresh();
        let now = self.start.elapsed().as_secs_f64();
        let scale = Scale::for_client(frame.w, frame.h);
        let icons = self.reader.detector.find_icons(&frame.to_gray(), scale);
        let reader = &self.reader;
        self.tracker.scan(now, &icons, &reader.cards, scale, &mut |icon| reader.read_altar(frame, icon, scale));
        let profile = self.profiles.get(&profile_path);
        let views = self.tracker.views(&self.reader.cards, profile);
        let center = ((frame.w / 2) as i32, (frame.h / 2) as i32);
        let hud = hud_view(&views, &self.reader.cards, self.labels.get(), self.keywords.get(), style, center);
        Ok(ModuleOutput {
            module: ID,
            lines: views.iter().map(view_line).collect(),
            view: serde_json::to_value(hud)?,
        })
    }

    fn on_inactive(&mut self) {
        self.tracker.clear();
    }
}
