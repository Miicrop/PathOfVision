//! `evaluate [<screens> <labels>] [--height <px>] [--write <dir>]`: accuracy report against labeled screenshots.
//! Defaults to the repo fixtures; `--height` downscales every screen first (synthetic resolution);
//! `--write` saves those resized screens as PNG into <dir> (for `pov-app --replay`) and stops.

use std::path::{Path, PathBuf};

use altar::evaluate::{evaluate_scaled, fixtures_dir};
use anyhow::{Context, bail};
use vision::{MIN_CLIENT_HEIGHT, Rgb};

const USAGE: &str = "usage: evaluate [<screens> <labels>] [--height <px>] [--write <dir>]";

fn main() -> anyhow::Result<()> {
    let mut paths: Vec<PathBuf> = Vec::new();
    let mut height: Option<u32> = None;
    let mut write: Option<PathBuf> = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--height" => {
                let v = args.next().context(USAGE)?;
                let h: u32 = v.parse().with_context(|| format!("--height {v}: not a pixel count"))?;
                if (h as usize) < MIN_CLIENT_HEIGHT {
                    bail!("--height {h}: below the minimum client height of {MIN_CLIENT_HEIGHT} px");
                }
                height = Some(h);
            }
            "--write" => write = Some(PathBuf::from(args.next().context(USAGE)?)),
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(());
            }
            _ => paths.push(PathBuf::from(arg)),
        }
    }
    let (screens, labels) = match paths.len() {
        0 => (fixtures_dir().join("screens"), fixtures_dir().join("labels.json")),
        2 => (paths[0].clone(), paths[1].clone()),
        _ => bail!(USAGE),
    };
    if let Some(dir) = write {
        let h = height.context("--write needs --height")?;
        let n = write_resized(&screens, &dir, h)?;
        println!("wrote {n} screens resized to height {h} px to {}", dir.display());
        return Ok(());
    }
    match height {
        Some(h) => println!("scale: each screen resized to height {h} px (factor {h} / screen height)"),
        None => println!("scale: native screens"),
    }
    let report = evaluate_scaled(&screens, &labels, height)?;
    println!("{report}");
    Ok(())
}

/// Saves every `screen_*.png` of `screens`, resized to height `h`, into `dir`.
fn write_resized(screens: &Path, dir: &Path, h: u32) -> anyhow::Result<usize> {
    std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    let mut n = 0;
    for entry in std::fs::read_dir(screens).with_context(|| format!("listing {}", screens.display()))? {
        let path = entry?.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
        if !(name.starts_with("screen_") && name.ends_with(".png")) {
            continue;
        }
        let img = Rgb::load(&path)?;
        let img = if h as usize == img.h { img } else { img.resize_area(f64::from(h) / img.h as f64) };
        img.save(&dir.join(&name))?;
        n += 1;
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_resized_saves_scaled_pngs() {
        let src = tempfile::tempdir().unwrap();
        let out = tempfile::tempdir().unwrap();
        Rgb { w: 32, h: 18, px: vec![[10, 20, 30]; 32 * 18] }.save(&src.path().join("screen_01.png")).unwrap();
        std::fs::write(src.path().join("notes.txt"), "x").unwrap();
        assert_eq!(write_resized(src.path(), &out.path().join("sub"), 9).unwrap(), 1);
        let back = Rgb::load(&out.path().join("sub/screen_01.png")).unwrap();
        assert_eq!((back.w, back.h), (16, 9));
    }
}
