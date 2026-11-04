//! `evaluate [<screens> <labels>] [--height <px>]`: accuracy report against labeled screenshots.
//! Defaults to the repo fixtures; `--height` downscales every screen first (synthetic resolution).

use std::path::PathBuf;

use altar::evaluate::{evaluate_scaled, fixtures_dir};
use anyhow::{Context, bail};
use vision::MIN_CLIENT_HEIGHT;

const USAGE: &str = "usage: evaluate [<screens> <labels>] [--height <px>]";

fn main() -> anyhow::Result<()> {
    let mut paths: Vec<PathBuf> = Vec::new();
    let mut height: Option<u32> = None;
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
    match height {
        Some(h) => println!("scale: each screen resized to height {h} px (factor {h} / screen height)"),
        None => println!("scale: native screens"),
    }
    let report = evaluate_scaled(&screens, &labels, height)?;
    println!("{report}");
    Ok(())
}
