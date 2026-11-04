//! Stage 2: read both options of every altar on a screenshot.
//!
//! All px constants are reference values (1440 px client) converted with the frame's [`Scale`].

use ocr::Ocr;
use vision::{Gray, Rgb, Scale};

use crate::data::{AltarKind, Card, Target, load_cards, repo_data_dir};
use crate::detect::{Detector, Icon, RING, altar_type, assets_dir};
use crate::matching::{Match, match_box};
use crate::text::parse_header;
use crate::verdict::Pos;

/// Reference px left/right of the icon center to OCR (widest box is ~1000 px).
const SEARCH_HALF_WIDTH: i32 = 650;
/// Reference px above/below the icon to OCR (tallest box is ~250 px).
const SEARCH_HEIGHT: i32 = 320;
/// OCR upscale at the reference scale; smaller clients are upscaled more (`UPSCALE / s`).
const UPSCALE: f32 = 2.0;
/// max(R, G, B) above this counts as text.
const TEXT_BRIGHTNESS: u8 = 110;
/// Slack (reference px) when testing that a line crosses the icon center column.
const CENTER_TOLERANCE: f32 = 20.0;
/// Reference px between pieces of one row split by an icon or life bar.
const MAX_FRAGMENT_GAP: f32 = 220.0;
/// Fragments are one row when their vertical centers differ by at most this share of the smaller height.
const ROW_ALIGN: f32 = 1.0 / 3.0;
/// Max vertical gap (reference px) between lines of one box.
const LINE_GAP: f32 = 22.0;

#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    pub text: String,
    pub x0: f32,
    pub x1: f32,
    pub y0: f32,
    pub y1: f32,
}

#[derive(Debug, Clone)]
pub struct OptionRead {
    pub position: Pos,
    pub target: Option<Target>,
    pub lines: Vec<String>,
    pub matches: Vec<Match>,
    pub unexplained: Vec<String>,
    pub has_header: bool,
}

impl OptionRead {
    /// Everything in the box was read and explained; nothing hidden or cut off.
    pub fn complete(&self) -> bool {
        self.has_header
            && !self.matches.is_empty()
            && self.unexplained.is_empty()
            && !self.matches.iter().any(|m| m.partial)
    }
}

#[derive(Debug, Clone)]
pub struct AltarRead {
    pub icon: Icon,
    pub altar: AltarKind,
    pub options: [OptionRead; 2],
}

/// OpenCV's bicubic kernel (A = -0.75) for a fractional offset `x` in [0, 1).
fn cubic_coeffs(x: f32) -> [f32; 4] {
    const A: f32 = -0.75;
    let c0 = ((A * (x + 1.0) - 5.0 * A) * (x + 1.0) + 8.0 * A) * (x + 1.0) - 4.0 * A;
    let c1 = ((A + 2.0) * x - (A + 3.0)) * x * x + 1.0;
    let c2 = ((A + 2.0) * (1.0 - x) - (A + 3.0)) * (1.0 - x) * (1.0 - x) + 1.0;
    [c0, c1, c2, 1.0 - c0 - c1 - c2]
}

/// Source taps (clamped, like OpenCV's border replicate) and fixed-point weights
/// for each destination index along one axis.
fn cubic_axis(src: usize, dst: usize, scale: f64) -> Vec<([usize; 4], [i64; 4])> {
    const COEF_SCALE: f32 = 2048.0;
    (0..dst)
        .map(|d| {
            let f = ((d as f64 + 0.5) / scale - 0.5) as f32;
            let s = f.floor();
            let c = cubic_coeffs(f - s);
            let s = s as i64;
            let taps = [0, 1, 2, 3].map(|k| (s - 1 + k).clamp(0, src as i64 - 1) as usize);
            (taps, c.map(|v| (v * COEF_SCALE).round_ties_even() as i64))
        })
        .collect()
}

/// Bicubic resize in OpenCV's 8-bit fixed-point form (INTER_CUBIC).
fn resize_cubic(img: &Gray, scale: f64) -> Gray {
    let dw = (img.w as f64 * scale).round_ties_even() as usize;
    let dh = (img.h as f64 * scale).round_ties_even() as usize;
    if img.w == 0 || img.h == 0 || dw == 0 || dh == 0 {
        return Gray { w: 0, h: 0, px: Vec::new() };
    }
    let xs = cubic_axis(img.w, dw, scale);
    let ys = cubic_axis(img.h, dh, scale);
    // Horizontal pass on every source row.
    let rows: Vec<Vec<i64>> = (0..img.h)
        .map(|y| {
            let row = &img.px[y * img.w..(y + 1) * img.w];
            xs.iter().map(|(t, c)| (0..4).map(|k| row[t[k]] as i64 * c[k]).sum()).collect()
        })
        .collect();
    let mut px = Vec::with_capacity(dw * dh);
    for (t, c) in &ys {
        let taps = [0, 1, 2, 3].map(|k| &rows[t[k]]);
        px.extend((0..dw).map(|x| {
            let v: i64 = (0..4).map(|k| taps[k][x] * c[k]).sum();
            ((v + (1 << 21)) >> 22).clamp(0, 255) as u8
        }));
    }
    Gray { w: dw, h: dh, px }
}

/// Box text is bright; the semi-transparent background lets dimmer game art through.
/// Keeps only bright pixels, as dark text on white, upscaled for OCR.
pub fn preprocess(crop: &Rgb, scale: Scale) -> Gray {
    let px = crop.px.iter().map(|p| if p.iter().max().copied().unwrap_or(0) > TEXT_BRIGHTNESS { 0 } else { 255 }).collect();
    resize_cubic(&Gray { w: crop.w, h: crop.h, px }, upscale(scale))
}

/// Factor from native crop px to OCR image px.
fn upscale(scale: Scale) -> f64 {
    UPSCALE as f64 / scale.0
}

/// Joins fragments of one text row that an icon or life bar split into pieces.
pub fn merge_rows(mut lines: Vec<Line>, scale: Scale) -> Vec<Line> {
    let max_gap = scale.px(MAX_FRAGMENT_GAP as f64) as f32;
    lines.sort_by(|a, b| a.x0.total_cmp(&b.x0)); // stable, like Python's sorted()
    let mut merged: Vec<Line> = Vec::new();
    for line in lines {
        let row = merged.iter_mut().find(|row| {
            // Same row: vertical centers within a third of the smaller height. (Mere overlap let
            // taller HUD text next to a box bridge two box rows.)
            let offset = ((row.y0 + row.y1) - (line.y0 + line.y1)).abs() / 2.0;
            offset <= ROW_ALIGN * (row.y1 - row.y0).min(line.y1 - line.y0) && line.x0 - row.x1 <= max_gap
        });
        match row {
            Some(row) => {
                row.text = format!("{} {}", row.text, line.text);
                row.x1 = row.x1.max(line.x1);
                row.y0 = row.y0.min(line.y0);
                row.y1 = row.y1.max(line.y1);
            }
            None => merged.push(line),
        }
    }
    merged
}

/// Lines crossing the icon's center column that form one contiguous block next to the icon.
///
/// Box text is centered on the icon, but lines cut off by HUD or screen edges are not,
/// so the test is only that the line spans the center column, or, once the block has
/// started, that it lies within the block's width.
pub fn box_lines(lines: Vec<Line>, cx: f32, near_y: f32, upward: bool, scale: Scale) -> Vec<Line> {
    let tolerance = scale.px(CENTER_TOLERANCE as f64) as f32;
    let line_gap = scale.px(LINE_GAP as f64) as f32;
    let mut rows = merge_rows(lines, scale);
    if upward {
        rows.sort_by(|a, b| b.y1.total_cmp(&a.y1));
    } else {
        rows.sort_by(|a, b| a.y0.total_cmp(&b.y0));
    }
    let mut block: Vec<Line> = Vec::new();
    let (mut bx0, mut bx1) = (f32::INFINITY, f32::NEG_INFINITY);
    let mut edge = near_y;
    for line in rows {
        // A row whose readable part misses the center column (a life bar hides the rest) still
        // belongs to the box when it lies within the lines taken so far; without it the gap to
        // the next row would end the box early.
        let centered = line.x0 - tolerance <= cx && cx <= line.x1 + tolerance;
        let inside = !block.is_empty() && line.x0 >= bx0 - tolerance && line.x1 <= bx1 + tolerance;
        if !centered && !inside {
            continue;
        }
        let gap = if upward { edge - line.y1 } else { line.y0 - edge };
        if gap > if block.is_empty() { line_gap * 3.0 } else { line_gap } {
            break;
        }
        edge = if upward { line.y0 } else { line.y1 };
        bx0 = bx0.min(line.x0);
        bx1 = bx1.max(line.x1);
        block.push(line);
    }
    block.sort_by(|a, b| a.y0.total_cmp(&b.y0));
    block
}

pub struct Reader {
    pub detector: Detector,
    pub ocr: Ocr,
    pub cards: Vec<Card>,
}

impl Reader {
    pub fn new() -> anyhow::Result<Reader> {
        Self::with_paths(&assets_dir(), &repo_data_dir().join("mods.json"), "en-US")
    }

    pub fn with_paths(assets: &std::path::Path, mods_json: &std::path::Path, language: &str) -> anyhow::Result<Reader> {
        Ok(Reader { detector: Detector::new(assets)?, ocr: Ocr::new(language)?, cards: load_cards(mods_json)? })
    }

    /// OCRs the region `[x0, x1) x [y0, y1)` (clamped); returns lines in native coords.
    fn ocr_region(&self, img: &Rgb, x0: i32, y0: i32, x1: i32, y1: i32, scale: Scale) -> anyhow::Result<Vec<Line>> {
        let clamp = |v: i32, hi: usize| v.clamp(0, hi as i32);
        let (x0, x1) = (clamp(x0, img.w), clamp(x1, img.w));
        let (y0, y1) = (clamp(y0, img.h), clamp(y1, img.h));
        if x1 <= x0 || y1 <= y0 {
            return Ok(Vec::new());
        }
        let big = preprocess(&img.crop(x0 as usize, y0 as usize, x1 as usize, y1 as usize), scale);
        let (ox, oy) = (x0 as f32, y0 as f32);
        let up = upscale(scale) as f32;
        Ok(self
            .ocr
            .read(&big)?
            .into_iter()
            .map(|l| Line {
                text: l.text,
                x0: ox + l.x as f32 / up,
                x1: ox + (l.x + l.w) as f32 / up,
                y0: oy + l.y as f32 / up,
                y1: oy + (l.y + l.h) as f32 / up,
            })
            .collect())
    }

    fn read_option(&self, lines: Vec<Line>, position: Pos, cx: f32, edge: f32, upward: bool, scale: Scale) -> OptionRead {
        let mut block: Vec<String> = box_lines(lines, cx, edge, upward, scale).into_iter().map(|l| l.text).collect();
        // The header is the box's first line; centered strays before it (gold labels, timers) go.
        let header = block.iter().enumerate().find_map(|(i, t)| parse_header(t).map(|tg| (i, tg)));
        let mut target = None;
        if let Some((i, tg)) = header {
            target = Some(tg);
            block.drain(..=i);
        }
        // No altar filter: mods of the other altar type do show up (e.g. Embers on an Eater altar).
        let (matches, rest) = match_box(&block, &self.cards, target);
        if target.is_none() {
            target = matches.first().map(|m| self.cards[m.card].target);
        }
        let unexplained = rest.iter().map(|&i| block[i].clone()).collect();
        OptionRead { position, target, lines: block, matches, unexplained, has_header: header.is_some() }
    }

    /// Reads both boxes of one altar; `None` if no box text matched (not a real altar).
    /// `img` is a native frame of the given scale; `icon` is in native px.
    pub fn read_altar(&self, img: &Rgb, icon: &Icon, scale: Scale) -> anyhow::Result<Option<AltarRead>> {
        let (half_width, height, ring) = (scale.px_i(SEARCH_HALF_WIDTH), scale.px_i(SEARCH_HEIGHT), scale.px_i(RING.1));
        let (x0, x1) = (icon.x - half_width, icon.x + half_width);
        let (top_edge, bottom_edge) = (icon.y - ring, icon.y + ring);
        let top_lines = self.ocr_region(img, x0, top_edge - height, x1, top_edge, scale)?;
        let bottom_lines = self.ocr_region(img, x0, bottom_edge, x1, bottom_edge + height, scale)?;
        let cx = icon.x as f32;
        let options = [
            self.read_option(top_lines, Pos::Top, cx, top_edge as f32, true, scale),
            self.read_option(bottom_lines, Pos::Bottom, cx, bottom_edge as f32, false, scale),
        ];
        if options.iter().all(|o| o.matches.is_empty()) {
            return Ok(None);
        }
        Ok(Some(AltarRead { icon: *icon, altar: altar_type(img, icon, scale), options }))
    }

    /// Reads a whole client frame; the scale follows from its size. Frames too small to scan
    /// ([`Scale::is_scannable`]) give no altars.
    pub fn read_altars(&self, img: &Rgb) -> anyhow::Result<Vec<AltarRead>> {
        if !Scale::is_scannable(img.w, img.h) {
            return Ok(Vec::new());
        }
        let scale = Scale::for_client(img.w, img.h);
        let mut altars = Vec::new();
        for icon in self.detector.find_icons(&img.to_gray(), scale) {
            if let Some(altar) = self.read_altar(img, &icon, scale)? {
                altars.push(altar);
            }
        }
        Ok(altars)
    }
}
