//! `units <map> [id]`: counts of a map's placed unit types, or where those of one type are.
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let ar = w3_mpq::Archive::open(&a[1]).unwrap();
    let units = w3_formats::map::PlacedUnit::parse_all(&ar.read("war3mapUnits.doo").unwrap()).unwrap();
    let mut counts = std::collections::BTreeMap::new();
    for u in &units {
        let id = String::from_utf8_lossy(&u.id).to_string();
        if a.get(2) == Some(&id) {
            println!("{id} at {:?} player {}", u.position, u.player);
        }
        *counts.entry(id).or_insert(0) += 1;
    }
    if a.len() < 3 {
        for (k, v) in counts { println!("{k} {v}"); }
    }
}
