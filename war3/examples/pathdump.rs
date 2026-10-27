//! Starts a map as the tags example does (two players of random race, a seed in hex)
//! and writes its pathing map's blockers, one byte per cell row by row: 1 for a
//! destructable's, 2 for another (a structure's), to compare with the original's.
//!
//! pathdump <game dir> <map> <seed> <out>
use std::path::Path;
fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().collect();
    let data = war3::data::GameData::open(Path::new(&a[1]), false)?;
    let map = std::sync::Arc::new(w3_mpq::Archive::open(&a[2])?);
    data.chain.mount(map, None, war3::data::PRIORITY_MAP);
    let seed = u32::from_str_radix(&a[3], 16)?;
    let lobby: Vec<w3_game::LobbySlot> = (0..2u8).map(|p| w3_game::LobbySlot { player: p, fill: if p == 0 { w3_game::SlotFill::Remote } else { w3_game::SlotFill::Local }, name: Some(format!("P{p}")), race: None, team: p as i32, color: p as i32, handicap: 100.0, difficulty: 1 }).collect();
    let mut g = w3_game::Game::load_with_keys(data.chain.clone(), data.version as i32, None)?;
    g.config()?;
    g.world.seed = seed;
    g.start_lobby(&lobby, war3::advanced::DEFAULT_FLAGS)?;
    g.tick(0.1);
    let p = g.world.pathing.as_ref().unwrap();
    let out: Vec<u8> = (0..p.blocked.len()).map(|i| if p.by_destructables.get(i).is_some_and(|&n| n > 0) { 1 } else if p.blocked[i] > 0 { 2 } else { 0 }).collect();
    std::fs::write(&a[4], out)?;
    println!("{} cells, {} by destructables, {} by others", p.blocked.len(), p.by_destructables.iter().filter(|&&n| n > 0).count(), p.blocked.iter().zip(&p.by_destructables).filter(|(b, d)| **b > 0 && **d == 0).count());
    Ok(())
}
