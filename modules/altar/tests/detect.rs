use altar::AltarKind;
use altar::detect::*;
use vision::Rgb;

fn frame(seed: u64) -> Rgb {
    let (w, h) = (1200usize, 700usize);
    let mut s = seed;
    let px = (0..w * h)
        .map(|_| {
            let mut c = [0u8; 3];
            for v in &mut c {
                s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                *v = 10 + ((s >> 33) % 51) as u8;
            }
            c
        })
        .collect();
    Rgb { w, h, px }
}

fn paste(frame: &mut Rgb, name: &str, x0: i32, y0: i32) {
    let icon = Rgb::load(&assets_dir().join(name)).unwrap();
    for y in 0..icon.h as i32 {
        for x in 0..icon.w as i32 {
            let (fx, fy) = (x0 + x, y0 + y);
            if fx >= 0 && fy >= 0 && (fx as usize) < frame.w && (fy as usize) < frame.h {
                frame.px[fy as usize * frame.w + fx as usize] = icon.px[y as usize * icon.w + x as usize];
            }
        }
    }
}

fn detector() -> Detector {
    Detector::new(&assets_dir()).unwrap()
}

#[test]
fn finds_a_pasted_icon() {
    let mut f = frame(1);
    paste(&mut f, "icon_eater.png", 400, 250);
    let icons = detector().find_icons(&f.to_gray());
    assert_eq!(icons.len(), 1, "{icons:?}");
    assert!((icons[0].x - 470).abs() <= 2 && (icons[0].y - 320).abs() <= 2, "{icons:?}");
    assert!(icons[0].score >= 0.66, "{icons:?}");
    assert_eq!(altar_type(&f, &icons[0]), AltarKind::Eater);
}

#[test]
fn tells_exarch_from_eater() {
    let mut f = frame(2);
    paste(&mut f, "icon_exarch.png", 400, 250);
    let icons = detector().find_icons(&f.to_gray());
    assert_eq!(icons.len(), 1, "{icons:?}");
    assert_eq!(altar_type(&f, &icons[0]), AltarKind::Exarch);
}

#[test]
fn finds_two_icons() {
    let mut f = frame(3);
    paste(&mut f, "icon_eater.png", 100, 100);
    paste(&mut f, "icon_exarch.png", 800, 400);
    let icons = detector().find_icons(&f.to_gray());
    assert_eq!(icons.len(), 2, "{icons:?}");
}

#[test]
fn noise_alone_finds_nothing() {
    let f = frame(4);
    assert!(detector().find_icons(&f.to_gray()).is_empty());
}

#[test]
fn icon_cut_off_at_edge_is_skipped() {
    let mut f = frame(5);
    paste(&mut f, "icon_eater.png", -60, 300);
    assert!(detector().find_icons(&f.to_gray()).is_empty());
}

#[test]
fn tiny_frame_does_not_panic() {
    let f = Rgb { w: 20, h: 20, px: vec![[0; 3]; 400] };
    assert!(detector().find_icons(&f.to_gray()).is_empty());
}

#[test]
#[ignore = "bench: cargo test -p altar --release --test detect -- --ignored --nocapture"]
fn bench_find_icons_screen_02() {
    let path = altar::evaluate::fixtures_dir().join("screens/screen_02.png");
    if !path.exists() {
        eprintln!("skipped: no local screenshots");
        return;
    }
    let img = Rgb::load(&path).unwrap();
    let det = detector();
    let mut times = Vec::new();
    let mut icons = Vec::new();
    for _ in 0..20 {
        let t0 = std::time::Instant::now();
        icons = det.find_icons(&img.to_gray());
        times.push(t0.elapsed());
    }
    times.sort();
    let mut gray_times: Vec<_> = (0..20)
        .map(|_| {
            let t0 = std::time::Instant::now();
            std::hint::black_box(img.to_gray());
            t0.elapsed()
        })
        .collect();
    gray_times.sort();
    println!("BENCH to_gray alone median: {:?}", gray_times[10]);
    println!("BENCH screen_02 {}x{} to_gray+find_icons median: {:?} (min {:?})", img.w, img.h, times[10], times[0]);
    println!("icons: {icons:?}");
}
