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
///
/// Single-threaded on purpose (it runs next to the game). Each output row is accumulated
/// in row-sized buffers whose inner loops run along x so they auto-vectorize: the window
/// sums of I and I^2 come from per-row prefix sums over the mask's horizontal runs, the
/// numerators from one `acc[x] += t_k * I[y+dy][x+dx]` pass per mask pixel.
pub fn match_masked(img: &Gray, templates: &[Template]) -> ScoreMap {
    let first = &templates[0];
    let (tw, th) = (first.px.w, first.px.h);
    assert!(img.w >= tw && img.h >= th, "image smaller than template");
    let (ow, oh) = (img.w - tw + 1, img.h - th + 1);

    // Mask pixels (dy, dx) in row-major order, and horizontal runs (dy, x0, x1).
    let on = |y: usize, x: usize| first.mask.px[y * tw + x] > 0;
    let offsets: Vec<(usize, usize)> =
        (0..th).flat_map(|y| (0..tw).map(move |x| (y, x))).filter(|&(y, x)| on(y, x)).collect();
    let mut runs: Vec<(usize, usize, usize)> = Vec::new();
    for y in 0..th {
        let mut x = 0;
        while x < tw {
            if on(y, x) {
                let x0 = x;
                while x < tw && on(y, x) {
                    x += 1;
                }
                runs.push((y, x0, x));
            } else {
                x += 1;
            }
        }
    }
    let n = offsets.len();
    let prepared: Vec<Prepared> = templates
        .iter()
        .map(|t| {
            let vals: Vec<f32> = offsets.iter().map(|&(y, x)| t.px.px[y * tw + x] as f32).collect();
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

    let iw = img.w;
    let fimg: Vec<f32> = img.px.iter().map(|&p| p as f32).collect();
    // Per-row prefix sums of I and I^2 (row pitch iw + 1); 255^2 * iw fits u32 for any screen.
    let pw = iw + 1;
    let mut p1 = vec![0u32; pw * img.h];
    let mut p2 = vec![0u32; pw * img.h];
    for (y, row) in img.px.chunks_exact(iw).enumerate() {
        let (mut a, mut b) = (0u32, 0u32);
        for (x, &p) in row.iter().enumerate() {
            let p = p as u32;
            a += p;
            b += p * p;
            p1[y * pw + x + 1] = a;
            p2[y * pw + x + 1] = b;
        }
    }

    let nf = n as f64;
    let mut s1 = vec![0u32; ow];
    let mut s2 = vec![0u32; ow];
    let mut nums = vec![vec![0f32; ow]; prepared.len()];
    for (y, out) in v.chunks_exact_mut(ow).enumerate() {
        s1.fill(0);
        s2.fill(0);
        for &(dy, x0, x1) in &runs {
            let r = (y + dy) * pw;
            add_run(&mut s1, &p1[r + x0..r + x0 + ow], &p1[r + x1..r + x1 + ow]);
            add_run(&mut s2, &p2[r + x0..r + x0 + ow], &p2[r + x1..r + x1 + ow]);
        }
        for num in &mut nums {
            num.fill(0.0);
        }
        for (k, &(dy, dx)) in offsets.iter().enumerate() {
            let start = (y + dy) * iw + dx;
            let row = &fimg[start..start + ow];
            for (num, p) in nums.iter_mut().zip(&prepared) {
                let tv = p.t[k];
                for (acc, &i) in num.iter_mut().zip(row) {
                    *acc += tv * i;
                }
            }
        }
        for (x, o) in out.iter_mut().enumerate() {
            let (s, sq) = (s1[x] as i64, s2[x] as i64);
            let var_n = n as i64 * sq - s * s;
            if var_n <= 0 {
                continue;
            }
            let win_norm = (var_n as f64 / nf).sqrt();
            let mut best = f32::NEG_INFINITY;
            for (num, p) in nums.iter().zip(&prepared) {
                if p.norm == 0.0 {
                    best = best.max(0.0);
                    continue;
                }
                let sc = (num[x] as f64 / (p.norm as f64 * win_norm)) as f32;
                best = best.max(if sc.is_finite() { sc } else { 0.0 });
            }
            *o = best;
        }
    }
    ScoreMap { w: ow, h: oh, v }
}

/// `acc[x] += hi[x] - lo[x]`: one mask run's sum from row prefix sums.
fn add_run(acc: &mut [u32], lo: &[u32], hi: &[u32]) {
    for ((a, &l), &h) in acc.iter_mut().zip(lo).zip(hi) {
        *a += h - l;
    }
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

    /// Binary ring of radius band `[r0, r1]` in an `n x n` mask.
    fn ring(n: usize, r0: i32, r1: i32) -> Gray {
        let c = (n / 2) as f64;
        let mut mask = Gray { w: n, h: n, px: vec![0; n * n] };
        for y in 0..n {
            for x in 0..n {
                let d = (((x as f64 - c).powi(2) + (y as f64 - c).powi(2)).sqrt()) as i32;
                if (r0..=r1).contains(&d) {
                    mask.px[y * n + x] = 255;
                }
            }
        }
        mask
    }

    /// Brute-force f64 masked Pearson correlation, best over templates.
    fn reference(img: &Gray, ts: &[Template]) -> Vec<f64> {
        let (tw, th) = (ts[0].px.w, ts[0].px.h);
        let (ow, oh) = (img.w - tw + 1, img.h - th + 1);
        let idx: Vec<(usize, usize)> = (0..th)
            .flat_map(|y| (0..tw).map(move |x| (x, y)))
            .filter(|&(x, y)| ts[0].mask.px[y * tw + x] > 0)
            .collect();
        let n = idx.len() as f64;
        let mut out = vec![0f64; ow * oh];
        for oy in 0..oh {
            for ox in 0..ow {
                let iv: Vec<f64> = idx.iter().map(|&(x, y)| img.px[(oy + y) * img.w + ox + x] as f64).collect();
                let im = iv.iter().sum::<f64>() / n;
                let ivar: f64 = iv.iter().map(|v| (v - im).powi(2)).sum();
                if ivar <= 0.0 {
                    continue;
                }
                let mut best = f64::NEG_INFINITY;
                for t in ts {
                    let tv: Vec<f64> = idx.iter().map(|&(x, y)| t.px.px[y * tw + x] as f64).collect();
                    let tm = tv.iter().sum::<f64>() / n;
                    let tvar: f64 = tv.iter().map(|v| (v - tm).powi(2)).sum();
                    let cov: f64 = iv.iter().zip(&tv).map(|(a, b)| (a - im) * (b - tm)).sum();
                    best = best.max(if tvar > 0.0 { cov / (ivar * tvar).sqrt() } else { 0.0 });
                }
                out[oy * ow + ox] = best;
            }
        }
        out
    }

    #[test]
    fn vectorized_matches_reference() {
        let img = rand_gray(120, 80, 12);
        let mask = ring(11, 3, 5);
        let ts = [
            Template { px: rand_gray(11, 11, 13), mask: mask.clone() },
            Template { px: img.crop(40, 30, 51, 41), mask },
        ];
        let m = match_masked(&img, &ts);
        let r = reference(&img, &ts);
        assert_eq!((m.w, m.h), (110, 70));
        for (i, (&a, &b)) in m.v.iter().zip(&r).enumerate() {
            assert!((a as f64 - b).abs() < 1e-4, "at {i}: {a} vs {b}");
        }
        assert!((m.v[30 * 110 + 40] - 1.0).abs() < 1e-4);
    }

    #[test]
    #[ignore = "bench: cargo test -p vision --release -- --ignored --nocapture"]
    fn bench_detection_cpu() {
        let img = rand_gray(860, 360, 9);
        let mask = ring(35, 14, 17);
        let ts = [
            Template { px: rand_gray(35, 35, 10), mask: mask.clone() },
            Template { px: rand_gray(35, 35, 11), mask },
        ];
        match_masked(&img, &ts);
        let t0 = std::time::Instant::now();
        for _ in 0..10 {
            match_masked(&img, &ts);
        }
        println!("BENCH match_masked 860x360, two 35x35 rings, single-threaded: {:?} per call", t0.elapsed() / 10);
    }
}
