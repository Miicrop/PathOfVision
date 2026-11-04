use altar::read::*;
use vision::Scale;

fn line(text: &str, x0: f32, x1: f32, y0: f32, y1: f32) -> Line {
    Line { text: text.to_string(), x0, x1, y0, y1 }
}

#[test]
fn merge_rows_joins_fragments_of_one_row() {
    let merged = merge_rows(vec![line("SHOCK", 420.0, 500.0, 12.0, 31.0), line("HITS ALWAYS", 100.0, 300.0, 10.0, 30.0)], Scale::REFERENCE);
    assert_eq!(merged.len(), 1);
    assert_eq!(merged[0].text, "HITS ALWAYS SHOCK");
    assert_eq!((merged[0].x0, merged[0].x1, merged[0].y0, merged[0].y1), (100.0, 500.0, 10.0, 31.0));
}

#[test]
fn merge_rows_keeps_far_fragments_apart() {
    let merged = merge_rows(vec![line("A", 100.0, 300.0, 10.0, 30.0), line("B", 521.0, 600.0, 10.0, 30.0)], Scale::REFERENCE);
    assert_eq!(merged.len(), 2);
    let at_limit = merge_rows(vec![line("A", 100.0, 300.0, 10.0, 30.0), line("B", 520.0, 600.0, 10.0, 30.0)], Scale::REFERENCE);
    assert_eq!(at_limit.len(), 1);
}

#[test]
fn box_lines_takes_the_contiguous_block_next_to_the_icon() {
    let lines = vec![
        line("far", 300.0, 500.0, 280.0, 300.0),
        line("second", 300.0, 500.0, 435.0, 455.0),
        line("first", 300.0, 500.0, 460.0, 480.0),
        line("third", 300.0, 500.0, 410.0, 430.0),
    ];
    let block = box_lines(lines, 400.0, 500.0, true, Scale::REFERENCE);
    let texts: Vec<&str> = block.iter().map(|l| l.text.as_str()).collect();
    assert_eq!(texts, ["third", "second", "first"]);
}

#[test]
fn box_lines_first_gap_may_be_three_line_gaps() {
    // 66 px from the icon edge is allowed for the first line, 67 is not.
    let ok = box_lines(vec![line("a", 300.0, 500.0, 534.0, 566.0)], 400.0, 500.0, false, Scale::REFERENCE);
    assert_eq!(ok.len(), 1);
    let far = box_lines(vec![line("a", 300.0, 500.0, 567.0, 590.0)], 400.0, 500.0, false, Scale::REFERENCE);
    assert!(far.is_empty());
}

#[test]
fn box_lines_ignores_lines_off_the_center_column() {
    let lines = vec![line("off", 600.0, 900.0, 440.0, 458.0), line("on", 300.0, 500.0, 460.0, 480.0)];
    let block = box_lines(lines, 400.0, 500.0, true, Scale::REFERENCE);
    assert_eq!(block.len(), 1);
    assert_eq!(block[0].text, "on");
}

#[test]
fn preprocess_binarizes_and_doubles() {
    let crop = vision::Rgb { w: 2, h: 1, px: vec![[0, 0, 111], [110, 110, 110]] };
    let g = preprocess(&crop, Scale::REFERENCE);
    assert_eq!((g.w, g.h), (4, 2));
    // Bright text becomes black, the rest white; the outer columns keep their value.
    assert_eq!(g.px[0], 0);
    assert_eq!(g.px[3], 255);
    assert_eq!(g.px[4], 0);
    assert_eq!(g.px[7], 255);
}

#[test]
fn preprocess_empty_crop() {
    let g = preprocess(&vision::Rgb { w: 0, h: 0, px: vec![] }, Scale::REFERENCE);
    assert_eq!((g.w, g.h), (0, 0));
}

#[cfg(windows)]
#[test]
fn empty_region_reads_no_lines() {
    use altar::detect::{Icon, assets_dir};
    let (w, h) = (1200usize, 700usize);
    let mut f = vision::Rgb { w, h, px: vec![[20, 20, 20]; w * h] };
    let icon_img = vision::Rgb::load(&assets_dir().join("icon_eater.png")).unwrap();
    // Paste the icon so its center is at (470, 40): the top region is empty after clamping.
    let (x0, y0) = (470 - icon_img.w as i32 / 2, 40 - icon_img.h as i32 / 2);
    for y in 0..icon_img.h as i32 {
        for x in 0..icon_img.w as i32 {
            let (fx, fy) = (x0 + x, y0 + y);
            if fx >= 0 && fy >= 0 && (fx as usize) < w && (fy as usize) < h {
                f.px[fy as usize * w + fx as usize] = icon_img.px[y as usize * icon_img.w + x as usize];
            }
        }
    }
    let reader = Reader::new().unwrap();
    let read = reader.read_altar(&f, &Icon { x: 470, y: 40, score: 1.0 }, Scale::REFERENCE).unwrap();
    assert!(read.is_none(), "no box text, so no altar");
    assert!(reader.read_altars(&f).unwrap().is_empty());
}

#[test]
fn merge_rows_scales_the_fragment_gap() {
    // 220 reference px is 165 px at 1080.
    let near = merge_rows(vec![line("A", 100.0, 300.0, 10.0, 30.0), line("B", 465.0, 600.0, 10.0, 30.0)], Scale(0.75));
    assert_eq!(near.len(), 1);
    let far = merge_rows(vec![line("A", 100.0, 300.0, 10.0, 30.0), line("B", 466.0, 600.0, 10.0, 30.0)], Scale(0.75));
    assert_eq!(far.len(), 2);
}

#[test]
fn preprocess_upscales_more_at_smaller_scale() {
    let crop = vision::Rgb { w: 3, h: 3, px: vec![[200, 200, 200]; 9] };
    let g = preprocess(&crop, Scale(0.75));
    assert_eq!((g.w, g.h), (8, 8));
}

#[cfg(windows)]
#[test]
fn reads_downscaled_screen_02() {
    let path = altar::evaluate::fixtures_dir().join("screens/screen_02.png");
    if !path.exists() {
        eprintln!("skipped: no local screenshots");
        return;
    }
    let img = vision::Rgb::load(&path).unwrap();
    let small = img.resize_area(1080.0 / img.h as f64);
    let reader = Reader::new().unwrap();
    let altars = reader.read_altars(&small).unwrap();
    // The first labeled altar of screen_02 is at (956, 1134) in 1440 px.
    let (ex, ey) = (956.0 * 0.75, 1134.0 * 0.75);
    let altar = altars
        .iter()
        .find(|a| (f64::from(a.icon.x) - ex).abs() <= 30.0 && (f64::from(a.icon.y) - ey).abs() <= 30.0)
        .unwrap_or_else(|| panic!("altar not found: {:?}", altars.iter().map(|a| a.icon).collect::<Vec<_>>()));
    let bottom = &altar.options[1];
    assert_eq!(bottom.target, Some(altar::Target::Player));
    let mut got: Vec<&str> = bottom.matches.iter().map(|m| reader.cards[m.card].id.as_str()).collect();
    got.sort();
    let mut want = vec![
        "TangledAltarDownsidePlayerLightningMonsterAura",
        "TangledAltarDownsidePlayerNonDamagingAilmentsReflectedToSelf",
        "TangledAltarDownsidePlayerTaintedPower",
        "TangledAltarUpsidePlayerIncreasedRarity",
        "TangledAltarUpsidePlayerIncreasedRarity",
    ];
    want.sort();
    assert_eq!(got, want);
}

#[cfg(windows)]
#[test]
fn tiny_frames_are_skipped() {
    let reader = Reader::new().unwrap();
    let t0 = std::time::Instant::now();
    for (w, h) in [(3440, 10), (3440, 0), (0, 1440)] {
        let f = vision::Rgb { w, h, px: vec![[30; 3]; w * h] };
        assert!(reader.read_altars(&f).unwrap().is_empty(), "{w}x{h}");
    }
    assert!(t0.elapsed() < std::time::Duration::from_millis(500), "{:?}", t0.elapsed());
}
