//! Counts a model's live particles over time in its first sequence: pcount <model> <archives...>
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let data = a[2..].iter().find_map(|ar| w3_mpq::Archive::open(ar).and_then(|x| x.read(&a[1])).ok()).expect("not found");
    let m = w3_formats::mdx::Model::parse(&data).unwrap();
    let s = &m.sequences[0];
    let mut p = w3_render::particles::Particles::default();
    let mut quads = Vec::new();
    for i in 0..60 {
        let now = i as f32 * 33.0;
        let time = w3_render::anim::Time { frame: s.start + (now as u32 % (s.end - s.start)), seq_start: s.start, seq_end: s.end, global: now as u32 };
        p.update(&m, &[], glam::Mat4::IDENTITY, &time, now);
        quads.clear();
        p.quads(&m, glam::Mat4::IDENTITY, glam::Quat::IDENTITY, &mut quads);
        if i % 10 == 9 {
            let by: Vec<(u32, u32, usize)> = (0..m.particle_emitters2.len()).map(|e| { let pe = &m.particle_emitters2[e]; (pe.texture_id, pe.filter_mode, quads.iter().filter(|q| q.texture_id == pe.texture_id && q.filter == pe.filter_mode).count()) }).collect();
            println!("t {now} quads {} by emitter (texture, filter, n) {by:?}", quads.len());
            let zs: Vec<i32> = quads.iter().filter(|q| q.filter == 2).map(|q| q.corners[0].z as i32).collect();
            println!("  smoke z {zs:?}");
        }
    }
}
