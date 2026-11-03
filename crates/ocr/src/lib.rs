//! Windows.Media.Ocr wrapper. Lines carry the union of their word rects as box.

use anyhow::{Result, anyhow, bail};
use vision::Gray;
use windows::Globalization::Language;
use windows::Graphics::Imaging::{BitmapPixelFormat, SoftwareBitmap};
use windows::Media::Ocr::OcrEngine;
use windows::Storage::Streams::DataWriter;
use windows::core::HSTRING;

#[derive(Debug, Clone, PartialEq)]
pub struct OcrLine {
    pub text: String,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

pub struct Ocr {
    engine: OcrEngine,
}

impl Ocr {
    pub fn new(language: &str) -> Result<Ocr> {
        let not_installed = || anyhow!("Windows OCR language {language} is not installed");
        let lang = Language::CreateLanguage(&HSTRING::from(language)).map_err(|_| not_installed())?;
        let engine = OcrEngine::TryCreateFromLanguage(&lang).map_err(|_| not_installed())?;
        Ok(Ocr { engine })
    }

    pub fn read(&self, img: &Gray) -> Result<Vec<OcrLine>> {
        if img.w == 0 || img.h == 0 {
            return Ok(Vec::new());
        }
        if img.px.len() != img.w * img.h {
            bail!("Gray buffer size {} does not match {}x{}", img.px.len(), img.w, img.h);
        }
        let mut bgra = Vec::with_capacity(img.px.len() * 4);
        for &v in &img.px {
            bgra.extend_from_slice(&[v, v, v, 255]);
        }
        let writer = DataWriter::new()?;
        writer.WriteBytes(&bgra)?;
        let buffer = writer.DetachBuffer()?;
        let bitmap = SoftwareBitmap::CreateCopyFromBuffer(
            &buffer,
            BitmapPixelFormat::Bgra8,
            img.w as i32,
            img.h as i32,
        )?;
        let result = self.engine.RecognizeAsync(&bitmap)?.get()?;

        let mut out = Vec::new();
        for line in result.Lines()? {
            let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
            for word in line.Words()? {
                let r = word.BoundingRect()?;
                x0 = x0.min(r.X);
                y0 = y0.min(r.Y);
                x1 = x1.max(r.X + r.Width);
                y1 = y1.max(r.Y + r.Height);
            }
            if x0 > x1 {
                continue; // line without words
            }
            out.push(OcrLine {
                text: line.Text()?.to_string(),
                x: x0 as i32,
                y: y0 as i32,
                w: (x1 - x0) as i32,
                h: (y1 - y0) as i32,
            });
        }
        Ok(out)
    }
}
