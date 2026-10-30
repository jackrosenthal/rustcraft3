//! `slkhash <slk>...`: each SLK file's hash as the original's data tables take it
//! (`w3_game::datahash`), and, given UnitData, UnitAbilities, UnitWeapons, and
//! UnitBalance in that order, the unit tables' module.
fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let files: Vec<Vec<u8>> = a.iter().map(|p| std::fs::read(p).unwrap()).collect();
    for (p, f) in a.iter().zip(&files) {
        let t = w3_game::datahash::Slk::parse(f);
        println!("{p}: {} rows, {} columns, hash {:08x}", t.rows, t.cols, t.hash());
    }
    if files.len() == 4 {
        let v = w3_game::datahash::unit_tables(&files[0], &files[1], &files[2], &files[3]);
        println!("unit tables {v:08x}, module {:08x}", w3_game::datahash::slot(v));
    }
}
