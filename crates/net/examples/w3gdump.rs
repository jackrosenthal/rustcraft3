//! Prints a replay's (.w3g) header and each turn's actions, decoded.
//!
//! w3gdump <replay> [turns]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().collect();
    let r = w3_formats::w3g::Replay::parse(&std::fs::read(&a[1])?)?;
    let limit: usize = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(usize::MAX);
    println!("version {} build {} length {} ms, {} players", r.version, r.build, r.length_ms, r.players.len() + 1);
    let mut t = 0u32;
    let mut turns = 0;
    for rec in &r.records {
        if let w3_formats::w3g::Record::Turn { ms, actions } = rec {
            t += *ms as u32;
            turns += 1;
            if turns > limit {
                break;
            }
            for (pid, bytes) in actions {
                match w3_net::action::Action::read_all(bytes) {
                    Ok(list) => {
                        for act in list {
                            println!("{t:7} p{pid} {act:?}");
                        }
                    }
                    Err(e) => println!("{t:7} p{pid} ({e}) {bytes:02x?}"),
                }
            }
        }
    }
    Ok(())
}
