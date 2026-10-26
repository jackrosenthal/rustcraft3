//! `doodads <map> [x y radius]`: counts of a map's doodad types, near a point if given.
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let ar = w3_mpq::Archive::open(&a[1]).unwrap();
    let d = w3_formats::map::Doodads::parse(&ar.read("war3map.doo").unwrap()).unwrap();
    let near = (a.len() > 4).then(|| (a[2].parse::<f32>().unwrap(), a[3].parse::<f32>().unwrap(), a[4].parse::<f32>().unwrap()));
    let mut counts = std::collections::BTreeMap::new();
    for p in &d.doodads {
        if let Some((x, y, r)) = near {
            if (p.position[0] - x).hypot(p.position[1] - y) > r { continue; }
        }
        *counts.entry(String::from_utf8_lossy(&p.id).to_string()).or_insert(0) += 1;
        if std::env::var("WHERE").is_ok_and(|w| w == "all" || w.as_bytes() == p.id) { println!("{} at {:?} scale {:?} var {} flags {}", String::from_utf8_lossy(&p.id), p.position, p.scale, p.variation, p.flags); }
    }
    for (k, v) in counts { println!("{k} {v}"); }
    println!("special {}", d.special.len());
}
