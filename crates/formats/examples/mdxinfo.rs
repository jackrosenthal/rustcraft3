//! Prints sequences, lights, and node tracks of a model in an archive chain.
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let mut data = None;
    for ar in &a[2..] { if let Ok(d) = w3_mpq::Archive::open(ar).and_then(|x| x.read(&a[1])) { data = Some(d); break; } }
    let m = w3_formats::mdx::Model::parse(&data.expect("not found")).unwrap();
    for s in &m.sequences { println!("seq {:?} {}..{} extent {:?}", s.name, s.start, s.end, s.extent); }
    println!("model extent {:?}", m.extent);
    for l in &m.lights { println!("light {:?} type {} atten {:?}..{:?} color {:?} intensity {} amb {:?} {}", l.node.name, l.kind, l.attenuation, "", l.color, l.intensity, l.ambient_color, l.ambient_intensity); }
    for t in &m.textures { println!("texture {:?} repl {}", t.path, t.replaceable_id); }
    for (i, mat) in m.materials.iter().enumerate() { for l in &mat.layers { println!("material {i} filter {} shading {:#x} tex {} alpha {} anim {:?} tracks {:?}", l.filter_mode, l.shading, l.texture_id, l.alpha, l.texture_anim_id, l.tracks.f32s.iter().map(|(t, tr)| (String::from_utf8_lossy(t).to_string(), tr.keys.iter().take(8).map(|k| (k.frame, k.value)).collect::<Vec<_>>())).collect::<Vec<_>>()); } }
    for p in &m.particle_emitters2 { println!("pe2 tracks {:?} node tracks {:?} squirt {} lat {} w {} l {} grav {}", p.tracks.f32s.iter().map(|(t, tr)| (String::from_utf8_lossy(t).to_string(), tr.keys.iter().take(6).map(|k| (k.frame, k.value)).collect::<Vec<_>>())).collect::<Vec<_>>(), p.node.tracks.vec3s.iter().map(|(t, _)| String::from_utf8_lossy(t).to_string()).collect::<Vec<_>>(), p.squirt, p.latitude, p.width, p.length, p.gravity); println!("pe2 {:?} colors {:?} alpha {:?} scale {:?} filter {} flags {:#x} rows {} cols {} life {} speed {} rate {} head {:?} tail {:?} hot {} time {}", p.node.name, p.segment_color, p.segment_alpha, p.segment_scaling, p.filter_mode, p.node.flags, p.rows, p.columns, p.lifespan, p.speed, p.emission_rate, p.head_intervals, p.tail_intervals, p.head_or_tail, p.time); }
    for p in &m.particle_emitters2 { for (t, tr) in &p.node.tracks.vec3s { println!("pe2 node {:?} pivot {:?} {} {:?}", p.node.name, m.pivots.get(p.node.object_id as usize), String::from_utf8_lossy(t), tr.keys.iter().take(10).map(|k| (k.frame, k.value)).collect::<Vec<_>>()); } }
    for ga in &m.geoset_anims { println!("geoset anim {} alpha {} flags {} color {:?} {:?}", ga.geoset_id, ga.alpha, ga.flags, ga.color, ga.tracks.f32s.iter().map(|(t, tr)| (String::from_utf8_lossy(t).to_string(), format!("{:?}", tr.interp), tr.keys.iter().take(8).map(|k| (k.frame, k.value)).collect::<Vec<_>>())).collect::<Vec<_>>()); }
    for ga in &m.geoset_anims { for (t, tr) in &ga.tracks.vec3s { println!("geoset anim {} {} {:?}", ga.geoset_id, String::from_utf8_lossy(t), tr.keys.iter().take(16).map(|k| (k.frame, k.value)).collect::<Vec<_>>()); } }
    println!("globals {:?}", m.global_sequences);
    for a in &m.attachments { println!("attachment {:?} path {:?} id {} tracks {:?}", a.node.name, a.path, a.attachment_id, a.tracks.f32s.iter().map(|(t, tr)| (String::from_utf8_lossy(t).to_string(), tr.keys.iter().take(8).map(|k| (k.frame, k.value)).collect::<Vec<_>>())).collect::<Vec<_>>()); }
    for e in &m.event_objects { println!("event {:?} {:?}", e.node.name, e.frames); }
    for c in &m.cameras { println!("camera {:?}", c); }
    for b in &m.bones { println!("bone {:?} parent {:?} pivot {:?} flags {:#x} id {}", b.node.name, b.node.parent, m.pivots.get(b.node.object_id as usize), b.node.flags, b.node.object_id); for (t, tr) in &b.node.tracks.vec3s { println!("  {} {:?}", String::from_utf8_lossy(t), tr.keys.iter().take(6).map(|k| (k.frame, k.value)).collect::<Vec<_>>()); } }
    for g in &m.geosets { println!("geoset extent {:?} verts {} material {} groups {:?} matrices {:?}", g.extent, g.vertices.len(), g.material_id, g.group_sizes, g.matrix_indices); if let Some(uv) = g.uvs.first() { let (mut a, mut b) = ([9f32; 2], [-9f32; 2]); for p in uv { a[0] = a[0].min(p[0]); a[1] = a[1].min(p[1]); b[0] = b[0].max(p[0]); b[1] = b[1].max(p[1]); } println!("  uv {:?}..{:?} sets {}", a, b, g.uvs.len()); } }
    // MDXVERTS=<geoset>: that geoset's vertices with their group.
    if let Some(gi) = std::env::var("MDXVERTS").ok().and_then(|v| v.parse::<usize>().ok()) {
        if let Some(g) = m.geosets.get(gi) {
            for (v, grp) in g.vertices.iter().zip(&g.vertex_groups) { println!("v {:?} group {}", v, grp); }
        }
    }
    for l in &m.lights {
        println!("light {:?} kind {} att {:?} color {:?} int {} amb {:?} ambint {} flags {:#x}", l.node.name, l.kind, l.attenuation, l.color, l.intensity, l.ambient_color, l.ambient_intensity, l.node.flags);
        for (t, tr) in &l.tracks.f32s { println!("  {} {:?} {:?}", String::from_utf8_lossy(t), tr.interp, tr.keys.iter().map(|k| (k.frame, k.value)).collect::<Vec<_>>()); }
        for (t, tr) in &l.tracks.vec3s { println!("  {} {:?} {:?}", String::from_utf8_lossy(t), tr.interp, tr.keys.iter().map(|k| (k.frame, k.value)).collect::<Vec<_>>()); }
        for (t, tr) in &l.node.tracks.quats { println!("  node {} {:?}", String::from_utf8_lossy(t), tr.keys.iter().map(|k| (k.frame, k.value)).collect::<Vec<_>>()); }
    }
}
