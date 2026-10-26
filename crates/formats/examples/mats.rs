//! `mats <path in archive> <archives...>`: a model's textures, materials, and which
//! geosets use each material.
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let data = a[2..].iter().find_map(|ar| w3_mpq::Archive::open(ar).and_then(|x| x.read(&a[1])).ok()).expect("not found");
    let m = w3_formats::mdx::Model::parse(&data).unwrap();
    for (i, s) in m.sequences.iter().enumerate() {
        println!("sequence {i} {:?} rarity {} looping {}", s.name, s.rarity, !s.non_looping);
    }
    for (i, t) in m.textures.iter().enumerate() {
        println!("texture {i} replaceable {} {}", t.replaceable_id, t.path);
    }
    for (i, mat) in m.materials.iter().enumerate() {
        let geosets: Vec<usize> = m.geosets.iter().enumerate().filter(|(_, g)| g.material_id as usize == i).map(|(k, _)| k).collect();
        println!("material {i} flags {:#x} geosets {:?}", mat.flags, geosets);
        for l in &mat.layers {
            println!("  layer filter {} shading {:#x} texture {} alpha {} tracks {:?}", l.filter_mode, l.shading, l.texture_id, l.alpha, l.tracks.f32s.iter().chain([].iter()).map(|(t, _)| String::from_utf8_lossy(t).to_string()).chain(l.tracks.u32s.iter().map(|(t, _)| String::from_utf8_lossy(t).to_string())).collect::<Vec<_>>());
        }
    }
}
