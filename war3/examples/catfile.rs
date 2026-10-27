//! Prints a file from the mounted archive chain (melee data set unless `custom` is given),
//! with a map mounted over it when one is given: catfile <game dir> <file> [map | custom]
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let data = war3::data::GameData::open(std::path::Path::new(&a[1]), false).unwrap();
    if let Some(m) = a.get(3).filter(|s| s.as_str() != "custom") {
        data.chain.mount(std::sync::Arc::new(w3_mpq::Archive::open(m).unwrap()), None, war3::data::PRIORITY_MAP);
    }
    if a.get(3).is_some_and(|s| s == "custom") { data.select_data_set(war3::data::DataSet::Custom); }
    use std::io::Write;
    std::io::stdout().write_all(&data.chain.read(&a[2]).expect("not found")).unwrap();
}
