//! `w3gdiff <a> <b>`: where two replays' records part: their header fields, then their
//! turns (length and actions) and checksums in order.
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let x = w3_formats::w3g::Replay::parse(&std::fs::read(&a[1]).unwrap()).unwrap();
    let y = w3_formats::w3g::Replay::parse(&std::fs::read(&a[2]).unwrap()).unwrap();
    let field = |n: &str, p: String, q: String| {
        if p != q {
            println!("{n}: {p} | {q}");
        }
    };
    field("product", format!("{:?}", x.product), format!("{:?}", y.product));
    field("version", format!("{} {}", x.version, x.build), format!("{} {}", y.version, y.build));
    field("flags", format!("{:#x}", x.flags), format!("{:#x}", y.flags));
    field("host", format!("{:?}", x.host), format!("{:?}", y.host));
    field("game", x.game_name.clone(), y.game_name.clone());
    field("stat", format!("{:?}", w3_net_stat(&x.stat)), format!("{:?}", w3_net_stat(&y.stat)));
    field("players", format!("{} {:?}", x.player_count, x.players), format!("{} {:?}", y.player_count, y.players));
    field("type", format!("{:#x} {:#x}", x.game_type, x.language), format!("{:#x} {:#x}", y.game_type, y.language));
    field("slots", format!("{:?}", x.slots), format!("{:?}", y.slots));
    field("seed", format!("{:08x} {} {}", x.seed, x.select_mode, x.start_spots), format!("{:08x} {} {}", y.seed, y.select_mode, y.start_spots));
    let (mut turn, mut same) = (0, 0);
    for (p, q) in x.records.iter().zip(&y.records) {
        if let w3_formats::w3g::Record::Turn { .. } = p {
            turn += 1;
        }
        if p != q {
            println!("records part after {same} alike, at turn {turn}: {:?} | {:?}", p, q);
            return;
        }
        same += 1;
    }
    println!("records alike for {same} ({} and {} in all)", x.records.len(), y.records.len());
}

fn w3_net_stat(b: &[u8]) -> Vec<u8> {
    b.to_vec()
}
