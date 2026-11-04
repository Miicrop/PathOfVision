//! Live altar tracking: keeps altars across scans, rereads incomplete ones, applies the profile.
//! Port of `Scanner.scan` from `prototype/live.py`. Time is passed in, so tests control it.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use vision::Scale;

use crate::data::{Card, Side};
use crate::detect::Icon;
use crate::read::AltarRead;
use crate::verdict::{OptionInput, OptionVerdict, Profile, Recommendation, judge, recommend};

/// Reference px an icon may move between scans (the camera follows the character).
const TRACK_DISTANCE: i32 = 250;
/// s without seeing an icon before its altar is dropped.
const TRACK_TIMEOUT: f64 = 1.5;
/// s between reads of an altar that is not completely readable yet.
const REREAD_INTERVAL: f64 = 0.5;
/// Share of mods two reads must have in common to be the same altar.
const SAME_ALTAR_OVERLAP: f64 = 0.6;
/// Relative scale change that counts as a new client size.
const SCALE_CHANGE: f64 = 0.01;

struct TrackedAltar {
    altar: AltarRead,
    last_seen: f64,
    last_read: f64,
    complete: bool,
}

/// What the HUD shows for one altar.
#[derive(Debug, Clone)]
pub struct AltarView {
    pub altar: AltarRead,
    pub complete: bool,
    pub options: [OptionInput; 2],
    pub verdicts: [OptionVerdict; 2],
    pub recommendation: Recommendation,
}

#[derive(Default)]
pub struct Tracker {
    tracks: Vec<TrackedAltar>,
    /// Icon positions whose read failed and was already logged.
    failed: Vec<(i32, i32)>,
    /// Scale of the previous scan; icon positions are only comparable at the same scale.
    scale: Option<Scale>,
}

fn distance(altar: &AltarRead, icon: &Icon) -> i32 {
    (altar.icon.x - icon.x).abs() + (altar.icon.y - icon.y).abs()
}

fn is_complete(altar: &AltarRead) -> bool {
    altar.options.iter().all(|o| o.complete())
}

/// Which of two reads of the same altar is better: complete, more mods, fewer partial ones.
fn quality(altar: &AltarRead) -> (bool, usize, i64, f64) {
    let matches: Vec<_> = altar.options.iter().flat_map(|o| &o.matches).collect();
    let partials = matches.iter().filter(|m| m.partial).count() as i64;
    (is_complete(altar), matches.len(), -partials, matches.iter().map(|m| m.score).sum())
}

fn better(a: (bool, usize, i64, f64), b: (bool, usize, i64, f64)) -> bool {
    (a.0, a.1, a.2).cmp(&(b.0, b.1, b.2)).then(a.3.total_cmp(&b.3)).is_ge()
}

/// Same mods on the same positions; values are ignored since partly hidden reads differ.
fn same_altar(a: &AltarRead, b: &AltarRead) -> bool {
    let sig = |altar: &AltarRead| {
        let mut counts: HashMap<(bool, usize), usize> = HashMap::new();
        for o in &altar.options {
            for m in &o.matches {
                *counts.entry((o.position == crate::verdict::Pos::Top, m.card)).or_default() += 1;
            }
        }
        counts
    };
    let (sa, sb) = (sig(a), sig(b));
    let smaller = sa.values().sum::<usize>().min(sb.values().sum());
    let common: usize = sa.iter().map(|(k, n)| (*n).min(sb.get(k).copied().unwrap_or(0))).sum();
    smaller > 0 && common as f64 / smaller as f64 >= SAME_ALTAR_OVERLAP
}

impl Tracker {
    pub fn new() -> Tracker {
        Tracker::default()
    }

    pub fn clear(&mut self) {
        self.tracks.clear();
        self.failed.clear();
        self.scale = None;
    }

    /// Matches the icons of one frame to the tracked altars, reads new or incomplete ones,
    /// and returns the altars currently tracked. A failing read skips only that icon.
    /// Icons are in native px of a frame at `scale`; a scale change (resized client) starts over.
    pub fn scan(
        &mut self,
        now: f64,
        icons: &[Icon],
        _cards: &[Card],
        scale: Scale,
        read: &mut dyn FnMut(&Icon) -> anyhow::Result<Option<AltarRead>>,
    ) -> Vec<&AltarRead> {
        if self.scale.is_some_and(|prev| (scale.0 - prev.0).abs() > SCALE_CHANGE * prev.0) {
            self.clear();
        }
        self.scale = Some(scale);
        let track_distance = scale.px_i(TRACK_DISTANCE);
        let mut unclaimed: Vec<usize> = (0..self.tracks.len()).collect();
        let mut still_failing = Vec::new();
        for icon in icons {
            let nearest = unclaimed
                .iter()
                .copied()
                .min_by_key(|&i| distance(&self.tracks[i].altar, icon))
                .filter(|&i| distance(&self.tracks[i].altar, icon) <= track_distance);
            let mut track = nearest;
            if let Some(i) = track {
                unclaimed.retain(|&u| u != i);
                let t = &mut self.tracks[i];
                t.last_seen = now;
                t.altar.icon = *icon; // follow the camera
                if t.complete || now - t.last_read < REREAD_INTERVAL {
                    continue;
                }
            }
            let altar = match read(icon) {
                Ok(Some(altar)) => altar,
                Ok(None) => continue,
                Err(e) => {
                    let known = self.failed.iter().any(|&(x, y)| (x - icon.x).abs() + (y - icon.y).abs() <= track_distance);
                    if !known {
                        eprintln!("altar read failed at ({}, {}): {e:#}", icon.x, icon.y);
                    }
                    still_failing.push((icon.x, icon.y));
                    continue;
                }
            };
            // Running moves the icon further than TRACK_DISTANCE between scans, so an altar
            // is also recognized by its content: the same mods on the same positions.
            if track.is_none() {
                track = self.tracks.iter().position(|t| same_altar(&t.altar, &altar));
            }
            match track {
                None => {
                    let complete = is_complete(&altar);
                    self.tracks.push(TrackedAltar { altar, last_seen: now, last_read: now, complete });
                }
                Some(i) => {
                    let t = &mut self.tracks[i];
                    t.last_seen = now;
                    t.last_read = now;
                    if better(quality(&altar), quality(&t.altar)) {
                        t.complete = is_complete(&altar);
                        t.altar = altar;
                    } else {
                        t.altar.icon = altar.icon;
                    }
                }
            }
        }
        self.failed = still_failing;
        self.tracks.retain(|t| now - t.last_seen <= TRACK_TIMEOUT);
        self.tracks.iter().map(|t| &t.altar).collect()
    }

    pub fn views(&self, cards: &[Card], profile: &Profile) -> Vec<AltarView> {
        self.tracks
            .iter()
            .map(|t| {
                let input = |i: usize| {
                    let o = &t.altar.options[i];
                    let ids = |side: Side| {
                        o.matches.iter().map(|m| &cards[m.card]).filter(|c| c.side == side).map(|c| c.id.clone()).collect()
                    };
                    OptionInput { target: o.target, upsides: ids(Side::Upside), downsides: ids(Side::Downside) }
                };
                let options = [input(0), input(1)];
                let verdicts = [judge(&options[0], profile), judge(&options[1], profile)];
                let recommendation = recommend(&options, &verdicts);
                AltarView { altar: t.altar.clone(), complete: t.complete, options, verdicts, recommendation }
            })
            .collect()
    }
}

/// Loads the active profile and reloads it when the file changes.
pub struct ProfileCache {
    fallback: PathBuf,
    current: Option<Profile>,
    key: Option<(PathBuf, Option<SystemTime>)>,
}

impl ProfileCache {
    pub fn new(fallback: PathBuf) -> ProfileCache {
        ProfileCache { fallback, current: None, key: None }
    }

    /// A file that fails to parse keeps the last good profile, or the fallback if there never was one.
    pub fn get(&mut self, path: &Path) -> &Profile {
        let mtime = std::fs::metadata(path).and_then(|m| m.modified()).ok();
        let key = (path.to_path_buf(), mtime);
        if self.current.is_none() || self.key.as_ref() != Some(&key) {
            match Profile::load(path) {
                Ok(profile) => self.current = Some(profile),
                Err(e) => {
                    eprintln!("profile {} unusable: {e:#}", path.display());
                    if self.current.is_none() {
                        self.current = Some(Profile::load(&self.fallback).unwrap_or_else(|e| {
                            eprintln!("shipped default profile {} unusable: {e:#}", self.fallback.display());
                            Profile::from_json(r#"{"name":"Empty"}"#).expect("literal profile parses")
                        }));
                    }
                }
            }
            self.key = Some(key);
        }
        self.current.as_ref().expect("set above")
    }
}
