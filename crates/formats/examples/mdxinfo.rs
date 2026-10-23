//! Prints sequences, lights, and node tracks of a model in an archive chain.
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let mut data = None;
    for ar in &a[2..] { if let Ok(d) = w3_mpq::Archive::open(ar).and_then(|x| x.read(&a[1])) { data = Some(d); break; } }
    let m = w3_formats::mdx::Model::parse(&data.expect("not found")).unwrap();
    for s in &m.sequences { println!("seq {:?} {}..{}", s.name, s.start, s.end); }
    println!("globals {:?}", m.global_sequences);
    for l in &m.lights {
        println!("light {:?} kind {} att {:?} color {:?} int {} amb {:?} ambint {} flags {:#x}", l.node.name, l.kind, l.attenuation, l.color, l.intensity, l.ambient_color, l.ambient_intensity, l.node.flags);
        for (t, tr) in &l.tracks.f32s { println!("  {} {:?} {:?}", String::from_utf8_lossy(t), tr.interp, tr.keys.iter().map(|k| (k.frame, k.value)).collect::<Vec<_>>()); }
        for (t, tr) in &l.tracks.vec3s { println!("  {} {:?} {:?}", String::from_utf8_lossy(t), tr.interp, tr.keys.iter().map(|k| (k.frame, k.value)).collect::<Vec<_>>()); }
        for (t, tr) in &l.node.tracks.quats { println!("  node {} {:?}", String::from_utf8_lossy(t), tr.keys.iter().map(|k| (k.frame, k.value)).collect::<Vec<_>>()); }
    }
}
