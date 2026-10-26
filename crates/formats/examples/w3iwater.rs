//! `w3iwater <map>`: a map's w3i flags, fog, light environment, and water tint.
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let ar = w3_mpq::Archive::open(&a[1]).unwrap();
    let i = w3_formats::map::MapInfo::parse(&ar.read("war3map.w3i").unwrap()).unwrap();
    println!("flags {:#x} fog {} {} {} {} {:?} light {} water {:?}", i.flags, i.fog_style, i.fog_start, i.fog_end, i.fog_density, i.fog_color, i.light_environment, i.water_color);
}
