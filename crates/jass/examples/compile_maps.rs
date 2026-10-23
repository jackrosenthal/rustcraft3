//! `compile_maps <common.j> <Blizzard.j> <map>...`: compiles each map's script against the
//! given common.j and Blizzard.j and reports errors.
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let common = std::fs::read_to_string(&a[1]).unwrap();
    let blizz = std::fs::read_to_string(&a[2]).unwrap();
    let (mut ok, mut bad) = (0, 0);
    for map in &a[3..] {
        let ar = match w3_mpq::Archive::open(map) {
            Ok(a) => a,
            Err(e) => { println!("SKIP {map}: {e}"); continue; }
        };
        let Ok(script) = ar.read("war3map.j").or_else(|_| ar.read("Scripts\\war3map.j")) else {
            println!("NOSCRIPT {map}");
            continue;
        };
        let script = String::from_utf8_lossy(&script);
        match w3_jass::compile([("common.j", &*common), ("Blizzard.j", &*blizz), ("war3map.j", &*script)]) {
            Ok(_) => ok += 1,
            Err(e) => { bad += 1; println!("FAIL {map}: {e}"); }
        }
    }
    println!("ok={ok} failed={bad}");
}
