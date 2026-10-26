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
    // `tiles <map> x y`: the corners around a point, texture and b for blight.
    if a.len() > 3 {
        let (x, y): (f32, f32) = (a[2].parse().unwrap(), a[3].parse().unwrap());
        let cx = ((x - t.offset[0]) / 128.0).round() as i32;
        let cy = ((y - t.offset[1]) / 128.0).round() as i32;
        for j in (cy - 4..=cy + 4).rev() {
            let row: Vec<String> = (cx - 6..=cx + 6)
                .map(|i| {
                    let c = &t.corners[(j as u32 * t.width + i as u32) as usize];
                    format!("{}{}", c.ground_texture, if c.blight { "b" } else { " " })
                })
                .collect();
            println!("{}", row.join(" "));
        }
    }
}
