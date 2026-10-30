//! `recunpack <file> <out>`: a recorded-game container's record (a replay's, a saved
//! game's, or a cache's), uncompressed, to `out`, its header printed.
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let file = std::fs::read(&a[1]).unwrap();
    let (h, data) = w3_formats::container::unpack(&file).unwrap();
    println!("product {} version {} build {} flags {:#x} length {} ms, record {} bytes", String::from_utf8_lossy(&h.product), h.version, h.build, h.flags, h.length_ms, data.len());
    std::fs::write(&a[2], &data).unwrap();
}
