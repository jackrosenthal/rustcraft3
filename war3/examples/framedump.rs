//! `framedump <data dir> <template>`: prints a glue frame's tree: index, kind, name,
//! definition name, children, and rectangle.
use std::path::Path;
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let data = war3::data::GameData::open(Path::new(&a[1]), false).unwrap();
    let db = w3_ui::FrameDb::load(&data.chain);
    let mut tree = w3_ui::FrameTree::new(4.0 / 3.0);
    tree.create(&db, &a[2], 0);
    tree.bind_points();
    tree.layout();
    for (i, f) in tree.frames.iter().enumerate() {
        println!("{i} {:?} {:?} def={:?} children={:?} rect={:?}", f.def.kind, f.name, f.def.name, f.children, f.rect);
    }
}
