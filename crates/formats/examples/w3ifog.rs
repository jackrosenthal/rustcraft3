//! `w3ifog <map>`: a map's fog settings.
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let ar = w3_mpq::Archive::open(&a[1]).unwrap();
    let i = w3_formats::map::MapInfo::parse(&ar.read("war3map.w3i").unwrap()).unwrap();
    println!("style {} start {} end {} density {} color {:?} light env {} weather {:?}", i.fog_style, i.fog_start, i.fog_end, i.fog_density, i.fog_color, i.light_environment, i.weather);
}
