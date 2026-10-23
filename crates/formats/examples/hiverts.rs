//! `hiverts <model> <archives...>`: model-space x/y extent of each geoset's highest vertices.
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let data = a[2..].iter().find_map(|ar| w3_mpq::Archive::open(ar).and_then(|x| x.read(&a[1])).ok()).expect("not found");
    let m = w3_formats::mdx::Model::parse(&data).unwrap();
    let zmax = m.geosets.iter().flat_map(|g| g.vertices.iter().map(|v| v[2])).fold(f32::MIN, f32::max);
    let hi: Vec<[f32; 3]> = m.geosets.iter().flat_map(|g| g.vertices.iter().copied()).filter(|v| v[2] > zmax - 1.0).collect();
    let (mut x0, mut x1, mut y0, mut y1) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
    for v in &hi { x0 = x0.min(v[0]); x1 = x1.max(v[0]); y0 = y0.min(v[1]); y1 = y1.max(v[1]); }
    println!("zmax {zmax} high verts x {x0}..{x1} y {y0}..{y1} ({} verts)", hi.len());
}
