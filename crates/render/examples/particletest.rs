//! `particletest <model> <archives...>`: runs a model's particle emitters for two seconds
//! of its first sequence and reports how many particles and quads there are.
use glam::{Mat4, Quat};
use w3_render::anim::{Skeleton, Time};
use w3_render::particles::Particles;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let data = a[2..].iter().find_map(|ar| w3_mpq::Archive::open(ar).and_then(|x| x.read(&a[1])).ok()).expect("not found");
    let m = w3_formats::mdx::Model::parse(&data).unwrap();
    let seq = &m.sequences[0];
    let mut skel = Skeleton::new(&m);
    let mut p = Particles::default();
    for step in 0..60 {
        let now = step as f32 * 33.0;
        let time = Time { frame: seq.start + (now as u32) % (seq.end - seq.start).max(1), seq_start: seq.start, seq_end: seq.end, global: now as u32 };
        skel.update(&m, &time, Quat::IDENTITY, None);
        p.update(&m, &skel.world, Mat4::IDENTITY, &time, now);
    }
    let mut quads = Vec::new();
    p.quads(&m, Mat4::IDENTITY, Quat::IDENTITY, &mut quads);
    println!("quads {} nodes {}", quads.len(), skel.world.len());
    if let Some(q) = quads.first() { println!("first {:?}", q); }
}
