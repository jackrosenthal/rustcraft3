//! `mapcks <game dir> <map>`: the map's checksums as a LAN game's description carries
//! them (its xoro and SHA-1 over the game's scripts), as the port computes them.
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let data = war3::data::GameData::open(std::path::Path::new(&a[1]), false).unwrap();
    let bytes = std::fs::read(&a[2]).unwrap();
    let archive = w3_mpq::Archive::open(&a[2]).unwrap();
    let (common, blizzard) = (data.chain.read("Scripts\\common.j").unwrap_or_default(), data.chain.read("Scripts\\blizzard.j").unwrap_or_default());
    let c = w3_net::map::Checksums::compute(&bytes, &common, &blizzard, |n| archive.read(n).ok());
    println!("xoro {:08x} sha1 {}", c.xoro, c.sha1.iter().map(|b| format!("{b:02x}")).collect::<String>());
}
