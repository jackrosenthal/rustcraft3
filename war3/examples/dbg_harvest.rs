use std::path::Path;
fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().collect();
    let data = war3::data::GameData::open(Path::new(&a[1]), false)?;
    let ar = std::sync::Arc::new(w3_mpq::Archive::open(&a[2])?);
    data.chain.mount(ar, None, war3::data::PRIORITY_MAP);
    let mut g = w3_game::Game::load(data.chain.clone(), 1)?;
    g.config()?;
    g.start(|id, _| if id == 0 { w3_game::SlotFill::Local } else { w3_game::SlotFill::Empty })?;
    let w = &g.world;
    let pea = *w.unit_order.iter().find(|&&h| w.unit(h).is_some_and(|u| &u.type_id == b"hpea")).unwrap();
    println!("peasant abilities {:?}", w.unit(pea).unwrap().abilities.iter().map(|(i, l)| (w3_game::objects::id_str(*i), *l)).collect::<Vec<_>>());
    println!("Ahar code {:?}", w.objects.abilities.get(*b"Ahar").map(|o| o.str("code")));
    println!("Ahar ability: {:?}", w.ability_with_code(pea, "Ahar"));
    let t = w.destructable_order[0];
    if let Some(w3_game::types::Obj::Destructable(d)) = w.handles.get(t) {
        let o = w.objects.destructables.get(d.type_id).unwrap();
        println!("dest {} life {} targType {:?}", w3_game::objects::id_str(d.type_id), d.life, o.str("targType"));
    }
    println!("can_harvest {}", w.can_harvest(pea, t));
    Ok(())
}
