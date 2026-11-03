use ocr::Ocr;
use vision::Gray;

fn load(path: &str) -> Gray {
    let img = image::open(path).unwrap().to_luma8();
    Gray { w: img.width() as usize, h: img.height() as usize, px: img.into_raw() }
}

#[test]
fn reads_a_rendered_line() {
    let g = load(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/header.png"));
    let ocr = Ocr::new("en-US").unwrap();
    let lines = ocr.read(&g).unwrap();
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert_eq!(lines[0].text, "PLAYER GAINS:");
    let l = &lines[0];
    assert!(l.x >= 0 && l.y >= 0 && l.w > 0 && l.h > 0);
    assert!(l.x + l.w <= g.w as i32 && l.y + l.h <= g.h as i32);
}

#[test]
fn unknown_language_is_an_error() {
    assert!(Ocr::new("xx-XX").is_err());
}

#[test]
fn blank_image_reads_nothing() {
    let g = Gray { w: 200, h: 50, px: vec![255; 200 * 50] };
    let ocr = Ocr::new("en-US").unwrap();
    assert!(ocr.read(&g).unwrap().is_empty());
}

#[test]
fn ocr_is_send_sync() {
    fn f<T: Send + Sync>() {}
    f::<Ocr>();
}
