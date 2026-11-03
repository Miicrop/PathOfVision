use crate::Gray;

pub struct Template {
    pub px: Gray,
    pub mask: Gray,
}

pub struct ScoreMap {
    pub w: usize,
    pub h: usize,
    pub v: Vec<f32>,
}

impl ScoreMap {
    /// First maximum in row-major order.
    pub fn max_loc(&self) -> (usize, usize, f32) {
        let mut best = (0usize, f32::NEG_INFINITY);
        for (i, &s) in self.v.iter().enumerate() {
            if s > best.1 {
                best = (i, s);
            }
        }
        (best.0 % self.w.max(1), best.0 / self.w.max(1), best.1)
    }

    pub fn max_in_place(&mut self, other: &ScoreMap) {
        for (a, &b) in self.v.iter_mut().zip(&other.v) {
            if b > *a {
                *a = b;
            }
        }
    }

    /// Sets a filled disk to 0 (like `cv2.circle(..., 0, -1)`).
    pub fn clear_disk(&mut self, cx: usize, cy: usize, r: usize) {
        let (cx, cy, r) = (cx as i64, cy as i64, r as i64);
        for y in (cy - r).max(0)..=(cy + r).min(self.h as i64 - 1) {
            for x in (cx - r).max(0)..=(cx + r).min(self.w as i64 - 1) {
                if (x - cx).pow(2) + (y - cy).pow(2) <= r * r {
                    self.v[y as usize * self.w + x as usize] = 0.0;
                }
            }
        }
    }
}

/// Zero-mean template values on the mask, and their norm.
struct Prepared {
    t: Vec<f32>,
    norm: f32,
}

/// Masked normalized cross-correlation (TM_CCOEFF_NORMED with a binary mask).
pub fn match_masked(img: &Gray, templates: &[Template]) -> ScoreMap {
    use rayon::prelude::*;
    let first = &templates[0];
    let (tw, th) = (first.px.w, first.px.h);
    assert!(img.w >= tw && img.h >= th, "image smaller than template");
    let (ow, oh) = (img.w - tw + 1, img.h - th + 1);

    let offsets: Vec<usize> = (0..th)
        .flat_map(|y| (0..tw).map(move |x| (y, x)))
        .filter(|&(y, x)| first.mask.px[y * tw + x] > 0)
        .map(|(y, x)| y * img.w + x)
        .collect();
    let toffs: Vec<usize> = (0..th)
        .flat_map(|y| (0..tw).map(move |x| y * tw + x))
        .filter(|&i| first.mask.px[i] > 0)
        .collect();
    let n = offsets.len();
    let prepared: Vec<Prepared> = templates
        .iter()
        .map(|t| {
            let vals: Vec<f32> = toffs.iter().map(|&i| t.px.px[i] as f32).collect();
            let mean = vals.iter().sum::<f32>() / n.max(1) as f32;
            let t: Vec<f32> = vals.iter().map(|v| v - mean).collect();
            let norm = t.iter().map(|v| v * v).sum::<f32>().sqrt();
            Prepared { t, norm }
        })
        .collect();

    let mut v = vec![0f32; ow * oh];
    if n == 0 {
        return ScoreMap { w: ow, h: oh, v };
    }
    let px = &img.px;
    let nf = n as f64;
    v.par_chunks_mut(ow).enumerate().for_each(|(y, row)| {
        for (x, out) in row.iter_mut().enumerate() {
            let base = y * img.w + x;
            let (mut s, mut s2) = (0i64, 0i64);
            for &o in &offsets {
                let p = px[base + o] as i64;
                s += p;
                s2 += p * p;
            }
            let var_n = n as i64 * s2 - s * s;
            if var_n <= 0 {
                continue;
            }
            let win_norm = (var_n as f64 / nf).sqrt();
            let mut best = f32::NEG_INFINITY;
            for p in &prepared {
                if p.norm == 0.0 {
                    best = best.max(0.0);
                    continue;
                }
                let mut num = 0f32;
                for (&o, &tv) in offsets.iter().zip(&p.t) {
                    num += tv * px[base + o] as f32;
                }
                let sc = (num as f64 / (p.norm as f64 * win_norm)) as f32;
                best = best.max(if sc.is_finite() { sc } else { 0.0 });
            }
            *out = best;
        }
    });
    ScoreMap { w: ow, h: oh, v }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rand_gray(w: usize, h: usize, seed: u64) -> Gray {
        let mut s = seed;
        let px = (0..w * h)
            .map(|_| {
                s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                (s >> 56) as u8
            })
            .collect();
        Gray { w, h, px }
    }

    fn full_mask(w: usize, h: usize) -> Gray {
        Gray { w, h, px: vec![255; w * h] }
    }

    fn paste(img: &mut Gray, t: &Gray, x0: usize, y0: usize) {
        for y in 0..t.h {
            for x in 0..t.w {
                img.px[(y0 + y) * img.w + x0 + x] = t.px[y * t.w + x];
            }
        }
    }

    #[test]
    fn exact_copy_scores_one_at_its_position() {
        let img = rand_gray(60, 40, 1);
        let t = Template { px: img.crop(17, 9, 28, 20), mask: full_mask(11, 11) };
        let m = match_masked(&img, &[t]);
        assert_eq!((m.w, m.h), (50, 30));
        let (x, y, s) = m.max_loc();
        assert_eq!((x, y), (17, 9));
        assert!((s - 1.0).abs() < 1e-4, "{s}");
    }

    #[test]
    fn mask_ignores_pixels_outside() {
        let img = rand_gray(60, 40, 2);
        let tp = img.crop(17, 9, 28, 20);
        let mut mask = full_mask(11, 11);
        for y in 0..11 {
            for x in 0..11 {
                if (x + y) % 3 == 0 {
                    mask.px[y * 11 + x] = 0;
                }
            }
        }
        let mut img2 = img.clone();
        for y in 0..11 {
            for x in 0..11 {
                if mask.px[y * 11 + x] == 0 {
                    img2.px[(9 + y) * 60 + 17 + x] = 3;
                }
            }
        }
        let m = match_masked(&img2, &[Template { px: tp, mask }]);
        let (x, y, s) = m.max_loc();
        assert_eq!((x, y), (17, 9));
        assert!((s - 1.0).abs() < 1e-4, "{s}");
    }

    #[test]
    fn inverted_patch_scores_minus_one() {
        let mut img = rand_gray(60, 40, 3);
        let t = rand_gray(11, 11, 4);
        let inv = Gray { w: 11, h: 11, px: t.px.iter().map(|p| 255 - p).collect() };
        paste(&mut img, &inv, 30, 12);
        let m = match_masked(&img, &[Template { px: t, mask: full_mask(11, 11) }]);
        let s = m.v[12 * m.w + 30];
        assert!((s + 1.0).abs() < 1e-4, "{s}");
    }

    #[test]
    fn flat_window_scores_zero() {
        let img = Gray { w: 30, h: 20, px: vec![100; 600] };
        let t = Template { px: rand_gray(5, 5, 5), mask: full_mask(5, 5) };
        let m = match_masked(&img, &[t]);
        assert!(m.v.iter().all(|&s| s == 0.0));
    }

    #[test]
    fn best_of_two_templates_wins() {
        let mut img = rand_gray(60, 40, 6);
        let a = rand_gray(9, 9, 7);
        let b = rand_gray(9, 9, 8);
        paste(&mut img, &b, 20, 25);
        let m = match_masked(
            &img,
            &[
                Template { px: a, mask: full_mask(9, 9) },
                Template { px: b, mask: full_mask(9, 9) },
            ],
        );
        let s = m.v[25 * m.w + 20];
        assert!((s - 1.0).abs() < 1e-4, "{s}");
    }

    #[test]
    fn clear_disk_zeroes_a_circle() {
        let mut m = ScoreMap { w: 12, h: 12, v: vec![1.0; 144] };
        m.clear_disk(5, 5, 2);
        assert_eq!(m.v[3 * 12 + 5], 0.0);
        assert_eq!(m.v[5 * 12 + 7], 0.0);
        assert_eq!(m.v[5 * 12 + 8], 1.0);
        assert_eq!(m.v[3 * 12 + 3], 1.0);
    }

    #[test]
    fn max_in_place_and_first_max_on_ties() {
        let mut a = ScoreMap { w: 3, h: 1, v: vec![0.5, 0.9, 0.9] };
        let b = ScoreMap { w: 3, h: 1, v: vec![0.7, 0.1, 0.2] };
        a.max_in_place(&b);
        assert_eq!(a.v, vec![0.7, 0.9, 0.9]);
        assert_eq!(a.max_loc(), (1, 0, 0.9));
    }

    #[test]
    #[ignore]
    fn bench_ring() {
        let img = rand_gray(860, 360, 9);
        let mut mask = Gray { w: 35, h: 35, px: vec![0; 35 * 35] };
        for y in 0..35 {
            for x in 0..35 {
                let d = (((x as f64 - 17.0).powi(2) + (y as f64 - 17.0).powi(2)).sqrt()) as i32;
                if (14..=17).contains(&d) {
                    mask.px[y * 35 + x] = 255;
                }
            }
        }
        let ts = [
            Template { px: rand_gray(35, 35, 10), mask: mask.clone() },
            Template { px: rand_gray(35, 35, 11), mask },
        ];
        match_masked(&img, &ts);
        let t0 = std::time::Instant::now();
        for _ in 0..10 {
            match_masked(&img, &ts);
        }
        println!("BENCH per call: {:?}", t0.elapsed() / 10);
    }
}
