fn main() {
    let a: Vec<String> = std::env::args().collect();
    let ar = w3_mpq::Archive::open(&a[1]).unwrap();
    let i = w3_formats::map::MapInfo::parse(&ar.read("war3map.w3i").unwrap()).unwrap();
    let t = w3_formats::map::Terrain::parse(&ar.read("war3map.w3e").unwrap()).unwrap();
    println!("camera {:?} complements {:?} playable {}x{} terrain {}x{} offset {:?}", i.camera_bounds, i.camera_complements, i.playable_width, i.playable_height, t.width, t.height, t.offset);
}
