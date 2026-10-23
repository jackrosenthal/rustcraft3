//! `tiles <map>`: a map's tileset and ground tile ids.
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let ar = w3_mpq::Archive::open(&a[1]).unwrap();
    let t = w3_formats::map::Terrain::parse(&ar.read("war3map.w3e").unwrap()).unwrap();
    println!("tileset {} ground {:?}", t.tileset as char, t.ground_tilesets.iter().map(|g| String::from_utf8_lossy(g).to_string()).collect::<Vec<_>>());
    let blight = t.corners.iter().filter(|c| c.blight).count();
    println!("blight corners {blight} of {}", t.corners.len());
    let mut counts = [0usize; 16];
    for c in &t.corners { counts[c.ground_texture as usize] += 1; }
    println!("ground texture use {counts:?}");
}
