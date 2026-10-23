//! `cams <model> <archives...>`: a model's cameras.
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let data = a[2..].iter().find_map(|ar| w3_mpq::Archive::open(ar).and_then(|x| x.read(&a[1])).ok()).expect("not found");
    let m = w3_formats::mdx::Model::parse(&data).unwrap();
    for c in &m.cameras {
        println!("{:?} pos {:?} target {:?} fov {} tracks {:?}", c.name, c.position, c.target, c.fov, c.tracks.f32s.iter().map(|(k, _)| String::from_utf8_lossy(k).to_string()).collect::<Vec<_>>());
    }
    for s in &m.sequences { println!("seq {:?}", s.name); }
}
