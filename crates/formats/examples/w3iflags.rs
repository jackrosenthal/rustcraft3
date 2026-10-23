//! `w3iflags <map>`: prints a map's w3i flags.
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let ar = w3_mpq::Archive::open(&a[1]).unwrap();
    let i = w3_formats::map::MapInfo::parse(&ar.read("war3map.w3i").unwrap()).unwrap();
    println!("flags {:#x}", i.flags);
}
