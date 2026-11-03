use std::path::Path;

use anyhow::Context;

/// 8-bit grayscale image, row-major.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Gray {
    pub w: usize,
    pub h: usize,
    pub px: Vec<u8>,
}

/// 8-bit RGB image, row-major.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rgb {
    pub w: usize,
    pub h: usize,
    pub px: Vec<[u8; 3]>,
}

fn clamp_rect(w: usize, h: usize, x0: usize, y0: usize, x1: usize, y1: usize) -> (usize, usize, usize, usize) {
    let x1 = x1.min(w);
    let y1 = y1.min(h);
    (x0.min(x1), y0.min(y1), x1, y1)
}

impl Gray {
    pub fn get(&self, x: usize, y: usize) -> u8 {
        self.px[y * self.w + x]
    }

    /// Half-open crop `[x0, x1) x [y0, y1)`, clamped to the image.
    pub fn crop(&self, x0: usize, y0: usize, x1: usize, y1: usize) -> Gray {
        let (x0, y0, x1, y1) = clamp_rect(self.w, self.h, x0, y0, x1, y1);
        let mut px = Vec::with_capacity((x1 - x0) * (y1 - y0));
        for y in y0..y1 {
            px.extend_from_slice(&self.px[y * self.w + x0..y * self.w + x1]);
        }
        Gray { w: x1 - x0, h: y1 - y0, px }
    }

    /// Fractional box filter (like OpenCV INTER_AREA): each destination pixel is the
    /// area-weighted mean of the source pixels it covers, rounded.
    ///
    /// When `1/scale` is an integer `k` dividing both dimensions, exact `k x k` blocks are
    /// averaged with integer sums (same result as the general path, much faster).
    pub fn resize_area(&self, scale: f64) -> Gray {
        let k = (1.0 / scale).round();
        if k >= 1.0 && k * scale == 1.0 {
            let k = k as usize;
            if self.w.is_multiple_of(k) && self.h.is_multiple_of(k) && self.w > 0 && self.h > 0 {
                return self.resize_blocks(k);
            }
        }
        self.resize_area_general(scale)
    }

    /// Mean of exact `k x k` blocks, rounded half up: `floor(sum / k^2 + 0.5)`.
    fn resize_blocks(&self, k: usize) -> Gray {
        let (dw, dh) = (self.w / k, self.h / k);
        let area = (k * k) as u32;
        let mut sums = vec![0u32; dw];
        let mut px = Vec::with_capacity(dw * dh);
        for by in 0..dh {
            sums.fill(0);
            for row in self.px[by * k * self.w..(by + 1) * k * self.w].chunks_exact(self.w) {
                for (sum, block) in sums.iter_mut().zip(row.chunks_exact(k)) {
                    *sum += block.iter().map(|&p| p as u32).sum::<u32>();
                }
            }
            px.extend(sums.iter().map(|&s| ((2 * s + area) / (2 * area)) as u8));
        }
        Gray { w: dw, h: dh, px }
    }

    /// General fractional path of [`Gray::resize_area`].
    fn resize_area_general(&self, scale: f64) -> Gray {
        let dw = ((self.w as f64 * scale).round_ties_even() as usize).max(1);
        let dh = ((self.h as f64 * scale).round_ties_even() as usize).max(1);
        let sx = self.w as f64 / dw as f64;
        let sy = self.h as f64 / dh as f64;
        let mut px = Vec::with_capacity(dw * dh);
        for dy in 0..dh {
            let (y0, y1) = (dy as f64 * sy, ((dy + 1) as f64 * sy).min(self.h as f64));
            for dx in 0..dw {
                let (x0, x1) = (dx as f64 * sx, ((dx + 1) as f64 * sx).min(self.w as f64));
                let mut sum = 0.0;
                let mut area = 0.0;
                for y in y0.floor() as usize..(y1.ceil() as usize).min(self.h) {
                    let wy = (y1.min(y as f64 + 1.0) - y0.max(y as f64)).max(0.0);
                    for x in x0.floor() as usize..(x1.ceil() as usize).min(self.w) {
                        let wx = (x1.min(x as f64 + 1.0) - x0.max(x as f64)).max(0.0);
                        sum += self.px[y * self.w + x] as f64 * wx * wy;
                        area += wx * wy;
                    }
                }
                px.push((sum / area + 0.5).floor().clamp(0.0, 255.0) as u8);
            }
        }
        Gray { w: dw, h: dh, px }
    }

    pub fn resize_nearest(&self, w: usize, h: usize) -> Gray {
        let mut px = Vec::with_capacity(w * h);
        for y in 0..h {
            let sy = (y * self.h / h).min(self.h - 1);
            for x in 0..w {
                px.push(self.px[sy * self.w + (x * self.w / w).min(self.w - 1)]);
            }
        }
        Gray { w, h, px }
    }
}

impl Rgb {
    pub fn load(path: &Path) -> anyhow::Result<Rgb> {
        let img = ::image::open(path).with_context(|| format!("loading {}", path.display()))?.to_rgb8();
        let (w, h) = (img.width() as usize, img.height() as usize);
        let px = img.pixels().map(|p| p.0).collect();
        Ok(Rgb { w, h, px })
    }

    /// Builds from BGRA bytes with a row pitch of `stride` bytes (screen captures); alpha is dropped.
    pub fn from_bgra(w: usize, h: usize, stride: usize, bytes: &[u8]) -> anyhow::Result<Rgb> {
        anyhow::ensure!(stride >= w * 4, "stride {stride} is smaller than a row ({} bytes)", w * 4);
        if h > 0 {
            let need = stride * (h - 1) + w * 4;
            anyhow::ensure!(bytes.len() >= need, "buffer too short: {} bytes, need {need}", bytes.len());
        }
        let mut px = Vec::with_capacity(w * h);
        for y in 0..h {
            let row = &bytes[y * stride..y * stride + w * 4];
            px.extend(row.as_chunks::<4>().0.iter().map(|c| [c[2], c[1], c[0]]));
        }
        Ok(Rgb { w, h, px })
    }

    pub fn crop(&self, x0: usize, y0: usize, x1: usize, y1: usize) -> Rgb {
        let (x0, y0, x1, y1) = clamp_rect(self.w, self.h, x0, y0, x1, y1);
        let mut px = Vec::with_capacity((x1 - x0) * (y1 - y0));
        for y in y0..y1 {
            px.extend_from_slice(&self.px[y * self.w + x0..y * self.w + x1]);
        }
        Rgb { w: x1 - x0, h: y1 - y0, px }
    }

    /// OpenCV weights: `Y = (R*4899 + G*9617 + B*1868 + 8192) >> 14`.
    pub fn to_gray(&self) -> Gray {
        let px = self
            .px
            .iter()
            .map(|&[r, g, b]| ((r as u32 * 4899 + g as u32 * 9617 + b as u32 * 1868 + 8192) >> 14) as u8)
            .collect();
        Gray { w: self.w, h: self.h, px }
    }
}

/// RGB to HSV in OpenCV's 8-bit convention (H 0-179, S and V 0-255).
pub fn hsv(p: [u8; 3]) -> (u8, u8, u8) {
    let [r, g, b] = p.map(|c| c as f32);
    let v = r.max(g).max(b);
    let diff = v - r.min(g).min(b);
    if diff == 0.0 {
        return (0, 0, v as u8);
    }
    let s = (255.0 * diff / v).round();
    let mut h = if v == r {
        (g - b) * 60.0 / diff
    } else if v == g {
        120.0 + (b - r) * 60.0 / diff
    } else {
        240.0 + (r - g) * 60.0 / diff
    };
    if h < 0.0 {
        h += 360.0;
    }
    ((h / 2.0).round() as u8 % 180, s as u8, v as u8)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn fixture(name: &str) -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)
    }

    #[test]
    fn loads_rgba_and_rgb_png() {
        let a = Rgb::load(&fixture("rgba_2x2.png")).unwrap();
        let b = Rgb::load(&fixture("rgb_2x2.png")).unwrap();
        assert_eq!((a.w, a.h), (2, 2));
        assert_eq!((b.w, b.h), (2, 2));
        assert_eq!(a.px, b.px);
        assert_eq!(a.px, vec![[255, 0, 0], [0, 255, 0], [0, 0, 255], [10, 20, 30]]);
    }

    #[test]
    fn gray_uses_opencv_weights() {
        assert_eq!(Rgb { w: 1, h: 1, px: vec![[255, 0, 0]] }.to_gray().px, vec![76]);
        assert_eq!(Rgb { w: 1, h: 1, px: vec![[0, 255, 0]] }.to_gray().px, vec![150]);
        assert_eq!(Rgb { w: 1, h: 1, px: vec![[0, 0, 255]] }.to_gray().px, vec![29]);
    }

    #[test]
    fn resize_area_averages_blocks() {
        let g = Gray { w: 4, h: 4, px: (0..16).collect() };
        let r = g.resize_area(0.5);
        assert_eq!((r.w, r.h), (2, 2));
        // block means 2.5, 4.5, 10.5, 12.5 rounded half up
        assert_eq!(r.px, vec![3, 5, 11, 13]);
    }

    #[test]
    fn resize_area_rounds_dims_like_opencv() {
        let g = Gray { w: 141, h: 141, px: vec![0; 141 * 141] };
        let q = g.resize_area(0.25);
        assert_eq!((q.w, q.h), (35, 35));
        let h = g.resize_area(0.5);
        assert_eq!((h.w, h.h), (70, 70));
    }

    #[test]
    fn resize_area_fast_path_matches_general_path() {
        let mut s = 42u64;
        let px = (0..64 * 48)
            .map(|_| {
                s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                (s >> 56) as u8
            })
            .collect();
        let g = Gray { w: 64, h: 48, px };
        for scale in [0.25, 0.5] {
            assert_eq!(g.resize_area(scale), g.resize_area_general(scale), "scale {scale}");
        }
        // Non-divisible sizes fall back to the general path.
        let odd = g.crop(0, 0, 63, 47);
        assert_eq!(odd.resize_area(0.5), odd.resize_area_general(0.5));
    }

    #[test]
    fn resize_nearest_and_crop() {
        let g = Gray { w: 2, h: 2, px: vec![1, 2, 3, 4] };
        assert_eq!(g.resize_nearest(4, 4).px[..4], [1, 1, 2, 2]);
        let c = Gray { w: 3, h: 3, px: (0..9).collect() }.crop(1, 1, 10, 10);
        assert_eq!((c.w, c.h, c.px), (2, 2, vec![4, 5, 7, 8]));
    }

    #[test]
    fn from_bgra_drops_alpha() {
        let r = Rgb::from_bgra(1, 1, 4, &[3, 2, 1, 255]).unwrap();
        assert_eq!(r.px, vec![[1, 2, 3]]);
    }

    #[test]
    fn from_bgra_handles_row_padding() {
        // 2x2 image, stride 12: each row is 8 pixel bytes + 4 padding bytes (last row unpadded).
        let bytes = [3, 2, 1, 255, 6, 5, 4, 255, 0, 0, 0, 0, 9, 8, 7, 255, 12, 11, 10, 255];
        let r = Rgb::from_bgra(2, 2, 12, &bytes).unwrap();
        assert_eq!(r.px, vec![[1, 2, 3], [4, 5, 6], [7, 8, 9], [10, 11, 12]]);
    }

    #[test]
    fn from_bgra_rejects_short_buffer_and_small_stride() {
        assert!(Rgb::from_bgra(2, 2, 8, &[0u8; 15]).is_err());
        assert!(Rgb::from_bgra(2, 1, 4, &[0u8; 8]).is_err());
    }

    #[test]
    fn hsv_matches_opencv() {
        assert_eq!(hsv([255, 0, 0]), (0, 255, 255));
        assert_eq!(hsv([0, 0, 255]), (120, 255, 255));
        assert_eq!(hsv([0, 128, 128]), (90, 255, 128));
    }
}
