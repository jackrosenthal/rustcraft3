//! Starts a map as the original's census of it was taken (two players of random race,
//! a seed in hex) and compares the tags this game gives with the original's (a file of
//! type codes, one per line after comments), printing where they first part.
//!
//! tags <game dir> <map> <seed> <reference>
use std::path::Path;
fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().collect();
    let data = war3::data::GameData::open(Path::new(&a[1]), false)?;
    let map = std::sync::Arc::new(w3_mpq::Archive::open(&a[2])?);
    data.chain.mount(map, None, war3::data::PRIORITY_MAP);
    let seed = u32::from_str_radix(&a[3], 16)?;
    let reference: Vec<String> = std::fs::read_to_string(&a[4])?.lines().filter(|l| !l.starts_with('#')).map(|l| l.to_string()).collect();
    let lobby: Vec<w3_game::LobbySlot> = (0..2u8).map(|p| w3_game::LobbySlot { player: p, fill: if p == 0 { w3_game::SlotFill::Remote } else { w3_game::SlotFill::Local }, name: Some(format!("P{p}")), race: None, team: p as i32, color: p as i32, handicap: 100.0, difficulty: 1 }).collect();
    w3_game::tags::RECORD.store(true, std::sync::atomic::Ordering::Relaxed);
    let mut g = w3_game::Game::load_with_keys(data.chain.clone(), data.version as i32, None)?;
    g.config()?;
    g.world.seed = seed;
    g.start_lobby(&lobby, war3::advanced::DEFAULT_FLAGS)?;
    g.tick(0.1);
    let ours: Vec<String> = g.world.tags.log.clone().unwrap_or_default().iter().map(|c| String::from_utf8_lossy(c).to_string()).collect();
    println!("ours {} tags, the original's {}", ours.len(), reference.len());
    match ours.iter().zip(&reference).position(|(a, b)| a != b) {
        Some(k) => {
            println!("first difference at tag {k}:");
            let from = k.saturating_sub(8);
            println!("  ours     {}", ours[from..(k + 12).min(ours.len())].join(" "));
            println!("  original {}", reference[from..(k + 12).min(reference.len())].join(" "));
        }
        None => println!("the same as far as both go"),
    }
    Ok(())
}
