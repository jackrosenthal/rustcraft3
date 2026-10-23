//! Loads a map, runs its script, and lists the resulting units.
use std::path::Path;
fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let a: Vec<String> = std::env::args().collect();
    let data = war3::data::GameData::open(Path::new(&a[1]), false)?;
    let ar = std::sync::Arc::new(w3_mpq::Archive::open(&a[2])?);
    data.chain.mount(ar, None, war3::data::PRIORITY_MAP);
    let mut g = w3_game::Game::load(data.chain.clone(), 1)?;
    g.config()?;
    for p in &g.world.players[..4] { println!("player {} controller {} slot {} race {} start {}", p.id, p.controller, p.slot_state, p.race, p.start_location); }
    let mut first = true;
    g.start(|_, c| if c == 0 && first { first = false; w3_game::SlotFill::Local } else { w3_game::SlotFill::Empty })?;
    for p in &g.world.players[..4] { println!("player {} controller {} slot {} race {} start {}", p.id, p.controller, p.slot_state, p.race, p.start_location); }
    for &h in &g.world.unit_order {
        let u = g.world.unit(h).unwrap();
        println!("{:#x} {} owner {} at ({:.0},{:.0})", h, w3_game::objects::id_str(u.type_id), u.owner, u.x, u.y);
    }
    println!("requests: {:?}", g.world.requests);
    Ok(())
}
