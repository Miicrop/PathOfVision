//! Stage 1: find Eldritch Altar icons on a full screenshot via template matching on the ring.

use std::path::{Path, PathBuf};

use vision::template::{Template, match_masked};
use vision::{Gray, Rgb, hsv};

use crate::data::AltarKind;

const COARSE_SCALE: f64 = 0.25;
const FINE_SCALE: f64 = 0.5;
const COARSE_THRESHOLD: f32 = 0.35;
const THRESHOLD: f32 = 0.55;
/// Ring radius band (px, full resolution) that both altar types share.
pub const RING: (i32, i32) = (52, 63);
const FINE_WINDOW: i32 = 12;
const MAX_ICONS: usize = 6;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Icon {
    /// Ring center, full-resolution pixels.
    pub x: i32,
    pub y: i32,
    pub score: f32,
}

pub fn assets_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("assets")
}

fn ring_mask(radius: i32) -> Gray {
    let n = (2 * radius + 1) as usize;
    let mut px = Vec::with_capacity(n * n);
    for yy in -radius..=radius {
        for xx in -radius..=radius {
            let rr = f64::from(xx * xx + yy * yy).sqrt();
            let ring = rr >= f64::from(RING.0) && rr <= f64::from(RING.1);
            let bar = yy.abs() <= 4 && rr < f64::from(RING.0);
            px.push(if ring || bar { 255 } else { 0 });
        }
    }
    Gray { w: n, h: n, px }
}

pub struct Detector {
    radius: i32,
    coarse: Vec<Template>,
    fine: Vec<Template>,
}

impl Detector {
    pub fn new(assets: &Path) -> anyhow::Result<Detector> {
        let grays = ["icon_eater.png", "icon_exarch.png"]
            .iter()
            .map(|n| Ok(Rgb::load(&assets.join(n))?.to_gray()))
            .collect::<anyhow::Result<Vec<Gray>>>()?;
        let radius = (grays[0].h / 2) as i32;
        let build = |scale: f64| -> Vec<Template> {
            grays
                .iter()
                .map(|g| {
                    let px = g.resize_area(scale);
                    let mask = ring_mask((g.h / 2) as i32).resize_nearest(px.w, px.h);
                    Template { px, mask }
                })
                .collect()
        };
        Ok(Detector { radius, coarse: build(COARSE_SCALE), fine: build(FINE_SCALE) })
    }

    pub fn find_icons(&self, gray: &Gray) -> Vec<Icon> {
        let (w, h) = (gray.w as i32, gray.h as i32);
        let small = gray.resize_area(COARSE_SCALE);
        let ct = &self.coarse[0].px;
        if small.w < ct.w || small.h < ct.h {
            return Vec::new();
        }
        let mut coarse = match_masked(&small, &self.coarse);
        let suppress_r = (f64::from(RING.1) * COARSE_SCALE * 1.5) as usize;
        let ft = &self.fine[0].px;

        let mut icons: Vec<Icon> = Vec::new();
        for _ in 0..MAX_ICONS * 2 {
            let (mx, my, score) = coarse.max_loc();
            if score < COARSE_THRESHOLD || icons.len() == MAX_ICONS {
                break;
            }
            coarse.clear_disk(mx, my, suppress_r);

            let bx = (mx as f64 / COARSE_SCALE) as i32;
            let by = (my as f64 / COARSE_SCALE) as i32;
            let x0 = (bx - FINE_WINDOW).max(0);
            let y0 = (by - FINE_WINDOW).max(0);
            let x1 = (bx + 2 * self.radius + FINE_WINDOW + 1).min(w);
            let y1 = (by + 2 * self.radius + FINE_WINDOW + 1).min(h);
            let window = gray.crop(x0 as usize, y0 as usize, x1 as usize, y1 as usize).resize_area(FINE_SCALE);
            if window.w < ft.w || window.h < ft.h {
                continue; // icon cut off by the screen edge
            }
            let fine = match_masked(&window, &self.fine);
            let (fx, fy, fscore) = fine.max_loc();
            if fscore < THRESHOLD {
                continue;
            }
            let cx = x0 + (fx as f64 / FINE_SCALE) as i32 + self.radius;
            let cy = y0 + (fy as f64 / FINE_SCALE) as i32 + self.radius;
            if icons.iter().all(|i| (cx - i.x).abs() > RING.1 || (cy - i.y).abs() > RING.1) {
                icons.push(Icon { x: cx, y: cy, score: (fscore * 1000.0).round() / 1000.0 });
            }
        }
        icons
    }
}

/// Eater icons are blue/teal inside the ring, Exarch icons are red/orange.
pub fn altar_type(img: &Rgb, icon: &Icon) -> AltarKind {
    let r = RING.0 - 4;
    let (x0, y0) = ((icon.x - r).max(0) as usize, (icon.y - r).max(0) as usize);
    let patch = img.crop(x0, y0, (icon.x + r).max(0) as usize, (icon.y + r).max(0) as usize);
    let (mut red, mut blue) = (0usize, 0usize);
    for &p in &patch.px {
        let (h, s, v) = hsv(p);
        if s > 90 && v > 70 {
            if h < 20 || h > 165 {
                red += 1;
            }
            if h > 80 && h < 125 {
                blue += 1;
            }
        }
    }
    if red > blue { AltarKind::Exarch } else { AltarKind::Eater }
}
