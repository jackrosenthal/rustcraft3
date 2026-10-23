//! `w3iload <map>`: prints a map's loading screen settings.
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let ar = w3_mpq::Archive::open(&a[1]).unwrap();
    let i = w3_formats::map::MapInfo::parse(&ar.read("war3map.w3i").unwrap()).unwrap();
    println!("screen {} model {:?} title {:?} subtitle {:?} text {:?}", i.loading_screen, i.loading_model, i.loading_title, i.loading_subtitle, i.loading_text);
    println!("prologue {} {:?} {:?} {:?}", i.prologue_screen, i.prologue_path, i.prologue_title, i.prologue_text);
}
