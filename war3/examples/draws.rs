//! Prints each of the game's random draws, and where it came from, while it starts a map
//! for two players of random race from a seed (hex) and plays its first turn; then the
//! main generator's state, to compare with the original's (read under Wine: 0x0d5a5d0a,
//! 0x8064a444 for Booty Bay from 35000000).
//!
//! draws <game dir> <map> <seed>
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
    w3_game::rng::TRACE.store(true, std::sync::atomic::Ordering::Relaxed);
    g.start_lobby(&lobby, war3::advanced::DEFAULT_FLAGS)?;
    eprintln!("TURN");
    g.tick(0.1);
    w3_game::rng::TRACE.store(false, std::sync::atomic::Ordering::Relaxed);
    eprintln!("main after the first turn {:?}", g.world.rng);
    eprintln!("races {:?}", (0..12).map(|p| g.world.players[p].race).collect::<Vec<_>>());
    Ok(())
}
