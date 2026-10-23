//! Parses terrain, info, doodads, and units of every map given.
use w3_formats::map::*;
fn main() {
    let (mut ok, mut bad) = (0, 0);
    for path in std::env::args().skip(1) {
        let Ok(ar) = w3_mpq::Archive::open(&path) else { continue };
        let mut errs = Vec::new();
        let mut check = |name: &str, f: &dyn Fn(&[u8]) -> Result<(), w3_formats::Error>| {
            if let Ok(d) = ar.read(name) {
                if let Err(e) = f(&d) { errs.push(format!("{name}: {e}")); }
            }
        };
        check("war3map.w3e", &|d| Terrain::parse(d).map(|_| ()));
        check("war3map.w3i", &|d| MapInfo::parse(d).map(|_| ()));
        check("war3map.doo", &|d| Doodads::parse(d).map(|_| ()));
        check("war3mapUnits.doo", &|d| PlacedUnit::parse_all(d).map(|_| ()));
        if errs.is_empty() { ok += 1 } else { bad += 1; println!("FAIL {path}: {errs:?}"); }
    }
    println!("ok={ok} failed={bad}");
}
