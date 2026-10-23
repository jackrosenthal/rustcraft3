//! Prints the HUD frame tree with resolved rectangles.
use std::path::Path;
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let data = war3::data::GameData::open(Path::new(&a[1]), false).unwrap();
    let mut db = w3_ui::FrameDb::load(&data.chain);
    let hud = war3::hud::Hud::new(&mut db, 16.0 / 9.0, 1);
    for (i, f) in hud.tree.frames.iter().enumerate() {
        println!("{i} {} {:?} {:?} text={:?} rect={:?}", f.def.block, f.def.kind, f.name, f.text, f.rect);
    }
}
