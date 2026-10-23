//! `ramps <archive> <map path inside it>`: lists where a map's ramp models start.
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let ar = w3_mpq::Archive::open(&a[1]).expect("open");
    let map = w3_mpq::Archive::from_bytes(&a[2], ar.read(&a[2]).expect("map")).expect("map archive");
    let t = w3_formats::map::Terrain::parse(&map.read("war3map.w3e").expect("w3e")).expect("parse");
    let l = w3_formats::map::ramp_letters(&t);
    let at = |x: u32, y: u32| l[(y * t.width + x) as usize];
    for y in 0..t.height - 1 {
        for x in 0..t.width - 1 {
            let s = [at(x, y + 1), at(x + 1, y + 1), at(x + 1, y), at(x, y)];
            if s.iter().filter(|&&c| c == b'R').count() == 1 && !s.contains(&b'S') {
                let hit = w3_formats::map::RAMP_PATTERNS.iter().any(|p| p.0 == &s);
                println!("{x} {y} {} {} {} {hit}", t.offset[0] + x as f32 * 128.0, t.offset[1] + y as f32 * 128.0, String::from_utf8_lossy(&s));
            }
        }
    }
}
