//! `evaluate [<screens> <labels>] [--height <px>] [--write <dir>] [--draft <file>]`: accuracy
//! report against labeled screenshots.
//! Defaults to the repo fixtures; `--height` downscales every screen first (synthetic resolution);
//! `--write` saves those resized screens as PNG into <dir> (for `pov-app --replay`) and stops;
//! `--draft` writes what was read from <screens> as a labels file to check by eye (with the OCR
//! lines and an `unsure` flag per box, to be removed from the final labels) and stops.

use std::path::{Path, PathBuf};

use altar::evaluate::{evaluate_scaled, fixtures_dir};
use altar::read::Reader;
use anyhow::{Context, bail};
use serde_json::{Value, json};
use vision::{MIN_CLIENT_HEIGHT, Rgb};

const USAGE: &str = "usage: evaluate [<screens> <labels>] [--height <px>] [--write <dir>] [--draft <file>]";

fn main() -> anyhow::Result<()> {
    let mut paths: Vec<PathBuf> = Vec::new();
    let mut height: Option<u32> = None;
    let mut write: Option<PathBuf> = None;
    let mut draft: Option<PathBuf> = None;
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
            "--draft" => draft = Some(PathBuf::from(args.next().context(USAGE)?)),
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(());
            }
            _ => paths.push(PathBuf::from(arg)),
        }
    }
    if let Some(out) = draft {
        let [screens] = paths.as_slice() else { bail!("--draft takes exactly one <screens> folder") };
        let n = write_draft(screens, &out)?;
        println!("wrote a draft for {n} screens to {}", out.display());
        return Ok(());
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

/// The `screen_*.png` files of `screens`, sorted by name.
fn screen_paths(screens: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    for entry in std::fs::read_dir(screens).with_context(|| format!("listing {}", screens.display()))? {
        let path = entry?.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name.starts_with("screen_") && name.ends_with(".png") {
            paths.push(path);
        }
    }
    paths.sort();
    Ok(paths)
}

/// Reads every native `screen_*.png` of `screens` and writes the result in the labels format.
fn write_draft(screens: &Path, out: &Path) -> anyhow::Result<usize> {
    let reader = Reader::new()?;
    let paths = screen_paths(screens)?;
    let mut doc = serde_json::Map::new();
    for path in &paths {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
        let img = Rgb::load(path)?;
        let altars: Vec<Value> = reader
            .read_altars(&img)?
            .iter()
            .map(|a| {
                let option = |o: &altar::read::OptionRead| {
                    let target = o.target.map(|t| format!("{t:?}").to_lowercase());
                    let mods: Vec<&str> = o.matches.iter().map(|m| reader.cards[m.card].id.as_str()).collect();
                    json!({ "target": target, "mods": mods, "unsure": !o.complete(), "lines": o.lines })
                };
                json!({
                    "icon": [a.icon.x, a.icon.y],
                    "altar": format!("{:?}", a.altar).to_lowercase(),
                    "top": option(&a.options[0]),
                    "bottom": option(&a.options[1]),
                })
            })
            .collect();
        doc.insert(name, Value::Array(altars));
    }
    std::fs::write(out, serde_json::to_string_pretty(&Value::Object(doc))?).with_context(|| format!("writing {}", out.display()))?;
    Ok(paths.len())
}

/// Saves every `screen_*.png` of `screens`, resized to height `h`, into `dir`.
fn write_resized(screens: &Path, dir: &Path, h: u32) -> anyhow::Result<usize> {
    std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    let paths = screen_paths(screens)?;
    for path in &paths {
        let img = Rgb::load(path)?;
        let img = if h as usize == img.h { img } else { img.resize_area(f64::from(h) / img.h as f64) };
        img.save(&dir.join(path.file_name().context("screen without a file name")?))?;
    }
    Ok(paths.len())
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
