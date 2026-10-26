//! `corners <map> <x> <y> <r>`: the terrain corners near a point, one letter each: W
//! water, B boundary, E map edge, combined as their first letters or "." for none.
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let ar = w3_mpq::Archive::open(&a[1]).unwrap();
    let t = w3_formats::map::Terrain::parse(&ar.read("war3map.w3e").unwrap()).unwrap();
    let (x, y, r): (f32, f32, f32) = (a[2].parse().unwrap(), a[3].parse().unwrap(), a[4].parse().unwrap());
    let (cx, cy, n) = (((x - t.offset[0]) / 128.0).round() as i32, ((y - t.offset[1]) / 128.0).round() as i32, (r / 128.0) as i32);
    for dy in (-n..=n).rev() {
        let mut row = String::new();
        for dx in -n..=n {
            let (px, py) = (cx + dx, cy + dy);
            if px < 0 || py < 0 || px as u32 >= t.width || py as u32 >= t.height {
                row.push(' ');
                continue;
            }
            let c = t.corner(px as u32, py as u32);
            row.push(match (c.boundary, c.map_edge, c.water) {
                (true, _, true) => 'b',
                (true, _, false) => 'B',
                (false, true, _) => 'E',
                (false, false, true) => 'w',
                _ => '.',
            });
        }
        println!("{row}");
    }
}
