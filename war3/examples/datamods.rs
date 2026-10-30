//! `datamods <game dir> <map>`: the keepalive's data modules for a game of `map`, as
//! the port computes them from the archives with the map mounted.
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let data = war3::data::GameData::open(std::path::Path::new(&a[1]), false).unwrap();
    data.chain.mount(std::sync::Arc::new(w3_mpq::Archive::open(&a[2]).unwrap()), None, war3::data::PRIORITY_MAP);
    if a.get(3).is_some_and(|s| s == "custom") {
        data.select_data_set(war3::data::DataSet::Custom);
    }
    for (k, v) in war3::net::data_modules(&data.chain) {
        println!("{k:2} {v:08x}");
    }
}
