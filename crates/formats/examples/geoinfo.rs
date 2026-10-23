//! `geoinfo <model> <geoset> <archives...>`: a geoset's vertices with their UVs and bone groups.
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let data = a[3..].iter().find_map(|ar| w3_mpq::Archive::open(ar).and_then(|x| x.read(&a[1])).ok()).expect("not found");
    let m = w3_formats::mdx::Model::parse(&data).unwrap();
    let g = &m.geosets[a[2].parse::<usize>().unwrap()];
    println!("groups {:?} matrices {:?}", g.group_sizes, g.matrix_indices);
    for (i, v) in g.vertices.iter().enumerate() {
        let uv = g.uvs.first().and_then(|u| u.get(i)).copied().unwrap_or([0.0; 2]);
        println!("{i} {:?} uv {:?} group {}", v, uv, g.vertex_groups.get(i).copied().unwrap_or(0));
    }
}
