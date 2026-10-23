//! `missing_natives <datadir>`: common.j natives with no implementation.
use std::path::Path;
fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().collect();
    let data = war3::data::GameData::open(Path::new(&a[1]), false)?;
    let src = data.chain.read_string("Scripts\\common.j")?;
    let table = w3_game::natives::table();
    let mut missing = Vec::new();
    for line in src.lines() {
        let l = line.trim_start();
        let l = l.strip_prefix("constant ").unwrap_or(l);
        if let Some(rest) = l.strip_prefix("native ") {
            let name = rest.split_whitespace().next().unwrap_or("");
            if !table.contains_key(name) {
                missing.push(name.to_string());
            }
        }
    }
    println!("{} missing: {}", missing.len(), missing.join(" "));
    Ok(())
}
