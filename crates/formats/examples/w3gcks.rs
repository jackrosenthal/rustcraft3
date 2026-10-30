//! `w3gcks <replay>`: each turn's number, length, and the checksum after it, a line each.
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let r = w3_formats::w3g::Replay::parse(&std::fs::read(&a[1]).unwrap()).unwrap();
    let (mut turn, mut ms) = (0u32, 0u16);
    for rec in &r.records {
        match rec {
            w3_formats::w3g::Record::Turn { ms: m, .. } => {
                turn += 1;
                ms = *m;
            }
            w3_formats::w3g::Record::Checksum(b) if b.len() == 4 => println!("{turn} {ms} {:08x}", u32::from_le_bytes([b[0], b[1], b[2], b[3]])),
            _ => {}
        }
    }
}
