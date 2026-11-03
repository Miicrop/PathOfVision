use altar::evaluate::{evaluate, fixtures_dir};

fn main() -> anyhow::Result<()> {
    let report = evaluate(&fixtures_dir().join("screens"), &fixtures_dir().join("labels.json"))?;
    println!("{report}");
    Ok(())
}
