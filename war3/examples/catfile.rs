//! Prints a file from the mounted archive chain (melee data set unless `custom` is given).
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let data = war3::data::GameData::open(std::path::Path::new(&a[1]), false).unwrap();
    if a.get(3).is_some_and(|s| s == "custom") { data.select_data_set(war3::data::DataSet::Custom); }
    use std::io::Write;
    std::io::stdout().write_all(&data.chain.read(&a[2]).expect("not found")).unwrap();
}
