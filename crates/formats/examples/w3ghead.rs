//! `w3ghead <replay>`: a replay's record fields and its records other than turns and
//! checksums.
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let r = w3_formats::w3g::Replay::parse(&std::fs::read(&a[1]).unwrap()).unwrap();
    println!("product {} version {} build {} flags {:#x} length {}", String::from_utf8_lossy(&r.product), r.version, r.build, r.flags, r.length_ms);
    println!("host {:?} game {:?} players {} type {:#x} language {:#x} select {:#x} spots {} seed {:08x}", r.host, r.game_name, r.player_count, r.game_type, r.language, r.select_mode, r.start_spots, r.seed);
    for p in &r.players {
        println!("player {:?}", p);
    }
    for s in &r.slots {
        println!("slot {:?}", s);
    }
    let mut turns = 0;
    for rec in &r.records {
        match rec {
            w3_formats::w3g::Record::Turn { .. } => turns += 1,
            w3_formats::w3g::Record::Checksum(_) => {}
            other => println!("after turn {turns}: {:?}", other),
        }
    }
}
