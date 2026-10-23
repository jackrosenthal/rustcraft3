//! Scripted play test: loads a map, runs commands, steps the simulation, and writes a PNG.
//!
//! `playtest <datadir> <map> <out.png> <width> <height> <commands...>`
//! Commands: `select x0 y0 x1 y1` (screen box), `click x y` (right click), `wait secs`,
//! `camera x y`.

use std::path::Path;

use war3::data::GameData;
use war3::scene::{Assets, Scene};

fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    let a: Vec<String> = std::env::args().collect();
    let (w, h): (u32, u32) = if a.len() > 5 { (a[4].parse()?, a[5].parse()?) } else { (1920, 1080) };
    let data = GameData::open(Path::new(&a[1]), false)?;
    let mut desc = wgpu::InstanceDescriptor::new_without_display_handle_from_env();
    desc.backends = wgpu::Backends::PRIMARY.with_env();
    let instance = wgpu::Instance::new(desc);
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))?;
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default()))?;
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let mut assets = Assets::new(&device, &queue, &data);
    let mut scene = Scene::load(&device, &queue, format, &data, &mut assets, Path::new(&a[2]), w as f32 / h as f32)?;
    scene.sync_units(&device, &queue, &mut assets);
    scene.sync_destructables(&device, &queue, &mut assets);
    let mut i = 6;
    let num = |s: &str| s.parse::<f32>().unwrap();
    while i < a.len() {
        match a[i].as_str() {
            "select" => {
                scene.select((num(&a[i + 1]), num(&a[i + 2])), (num(&a[i + 3]), num(&a[i + 4])), (w, h), false);
                eprintln!("selected {:?}", scene.selection);
                i += 5;
            }
            "lclick" => {
                let consumed = scene.left_click(num(&a[i + 1]), num(&a[i + 2]), (w, h));
                eprintln!("left click consumed: {consumed}");
                i += 3;
            }
            "key" => {
                scene.key(a[i + 1].chars().next().unwrap());
                i += 2;
            }
            "click" => {
                scene.command(num(&a[i + 1]), num(&a[i + 2]), (w, h));
                i += 3;
            }
            "camera" => {
                scene.camera.target.x = num(&a[i + 1]);
                scene.camera.target.y = num(&a[i + 2]);
                i += 3;
            }
            "wait" => {
                let mut t = num(&a[i + 1]);
                while t > 0.0 {
                    scene.update(0.02);
                    scene.sync_units(&device, &queue, &mut assets);
    scene.sync_destructables(&device, &queue, &mut assets);
                    t -= 0.02;
                }
                i += 2;
            }
            "card" => {
                if let Some(hud) = &scene.hud {
                    for b in &hud.card {
                        eprintln!("card {:?} ({},{}) hotkey {:?} {}", b.action, b.x, b.y, b.hotkey, b.icon);
                    }
                }
                i += 1;
            }
            "dump" => {
                if let Some(g) = &scene.game {
                    for &h in &scene.selection {
                        if let Some(u) = g.world.unit(h) {
                            eprintln!("{h:#x} at ({:.0},{:.0}) {:?} gold {} lumber {} anim {} path {:?}", u.x, u.y, u.action, u.carry_gold, u.carry_lumber, u.anim, u.path.len());
                        }
                    }
                }
                i += 1;
            }
            "pick" => {
                let (x, y) = (num(&a[i + 1]), num(&a[i + 2]));
                if let Some(g) = &scene.game {
                    let wd = &g.world;
                    eprintln!("{} destructables", wd.destructable_order.len());
                    if let Some((gx, gy)) = scene.ground_at(x, y, (w, h)) {
                        let mut best = (f32::MAX, 0u32);
                        for &d in &wd.destructable_order {
                            if let Some(w3_game::types::Obj::Destructable(dd)) = wd.handles.get(d) {
                                let dist = (dd.x - gx).hypot(dd.y - gy);
                                if dist < best.0 { best = (dist, d); }
                            }
                        }
                        if let Some(w3_game::types::Obj::Destructable(dd)) = wd.handles.get(best.1) {
                            let o = wd.objects.destructables.get(dd.type_id).unwrap();
                            eprintln!("nearest dest {:.0} away at ({:.0},{:.0},{:.0}) life {} vis {} sel {} occH {}", best.0, dd.x, dd.y, dd.z, dd.life, dd.visible, o.str("selSize"), o.str("occH"));
                        }
                    }
                }
                eprintln!("unit {:?} dest {:?} ground {:?}", scene.pick_unit(x, y, (w, h)), scene.pick_destructable(x, y, (w, h)), scene.ground_at(x, y, (w, h)));
                i += 3;
            }
            "players" => {
                if let Some(g) = &scene.game {
                    let w = &g.world;
                    for p in 0..2u8 {
                        let n = w.unit_order.iter().filter(|&&h| w.unit(h).is_some_and(|u| u.owner == p && !u.dead)).count();
                        let mut types: std::collections::BTreeMap<String, i32> = Default::default();
                        for &h in &w.unit_order {
                            if let Some(u) = w.unit(h) { if u.owner == p && !u.dead { *types.entry(w3_game::objects::id_str(u.type_id)).or_default() += 1; } }
                        }
                        eprintln!("p{p} gold {} lumber {} food {}/{} units {n} {:?}", w.players[p as usize].get_state(1), w.players[p as usize].get_state(2), w.players[p as usize].get_state(5), w.players[p as usize].get_state(4), types);
                    }
                }
                i += 1;
            }
            // spawn <type> <player> <dx> <dy>: creates a unit near player 0's start.
            "spawn" => {
                if let Some(g) = scene.game.as_mut() {
                    let (sx, sy) = g.world.start_of(0);
                    let id = w3_game::objects::id_from_str(&a[i + 1]);
                    let hd = w3_game::natives::units::create_unit(&mut g.world, num(&a[i + 2]) as u8, id, sx + num(&a[i + 3]), sy + num(&a[i + 4]), 270.0);
                    eprintln!("spawned {} = {hd}", a[i + 1]);
                }
                i += 5;
            }
            // hero <handle> <xp> <ability>: grants experience and learns an ability.
            "hero" => {
                if let Some(g) = scene.game.as_mut() {
                    let h = num(&a[i + 1]) as u32;
                    g.world.add_hero_xp(h, num(&a[i + 2]) as i32);
                    let ok = g.world.learn_skill(h, w3_game::objects::id_from_str(&a[i + 3]));
                    let u = g.world.unit(h).unwrap();
                    eprintln!("hero level {} xp {} str {} agi {} int {} life {}/{} mana {}/{} learned {ok} skill points {}", u.level, u.hero_xp, u.str_, u.agi, u.int, u.life, u.max_life, u.mana, u.max_mana, u.skill_points);
                }
                i += 4;
            }
            // order <unit> <order> <target>: issues an order at a unit.
            "order" => {
                if let Some(g) = scene.game.as_mut() {
                    let id = w3_game::natives::basic::order_id(&a[i + 2]);
                    g.world.order_unit(num(&a[i + 1]) as u32, id, 0.0, 0.0, num(&a[i + 3]) as u32);
                }
                i += 4;
            }
            "unit" => {
                if let Some(g) = scene.game.as_ref() {
                    if let Some(u) = g.world.unit(num(&a[i + 1]) as u32) {
                        eprintln!("unit {} life {}/{} mana {} action {:?} buffs {:?}", w3_game::objects::id_str(u.type_id), u.life, u.max_life, u.mana, u.action, u.buffs.iter().map(|b| (w3_game::objects::id_str(b.id), b.remaining, b.stun)).collect::<Vec<_>>());
                    }
                }
                i += 2;
            }
            "mine" => {
                if let Some(g) = &scene.game {
                    let w = &g.world;
                    let m = num(&a[i + 1]) as u32;
                    if let Some(u) = w.unit(m) { eprintln!("mine at {:.0},{:.0} inside {} gold {}", u.x, u.y, u.inside, u.resource); }
                    for &h in &w.unit_order {
                        if let Some(u) = w.unit(h) { if u.owner == 1 && matches!(u.action, w3_game::types::Action::Harvest{target} if target == m) || u.owner == 1 && u.carry_gold > 0 { eprintln!("  t={:.1} {h} at {:.0},{:.0} {:?} inside {} path {}", w.time, u.x, u.y, u.action, u.inside, u.path.len()); } }
                    }
                }
                i += 2;
            }
            "structs" => {
                if let Some(g) = &scene.game {
                    let w = &g.world;
                    for (h, o) in w.handles.iter() {
                        if let w3_game::types::Obj::Unit(u) = o {
                            if u.owner == 1 && u.is_building { eprintln!("  {h} {} at {:.0},{:.0} constructing {:?} dead {} in order {}", w3_game::objects::id_str(u.type_id), u.x, u.y, u.constructing, u.dead, w.unit_order.contains(&h)); }
                        }
                    }
                }
                i += 1;
            }
            "aidump" => {
                if let Some(g) = &scene.game {
                    let w = &g.world;
                    for &h in &w.unit_order {
                        if let Some(u) = w.unit(h) { if u.owner == 1 && !u.is_building { eprintln!("  {} {:?} gold {} lumber {} inside {} lvl {} abil {:?}", w3_game::objects::id_str(u.type_id), u.action, u.carry_gold, u.carry_lumber, u.inside, u.level, u.abilities.iter().map(|(a, l)| (w3_game::objects::id_str(*a), *l)).collect::<Vec<_>>()); } }
                    }
                    if let Some(a) = w.ai.get(&1) { eprintln!("  harvest {:?}", a.harvest); }
                }
                i += 1;
            }
            "state" => {
                if let Some(g) = &scene.game {
                    let p = &g.world.players[g.world.local_player as usize];
                    eprintln!("gold {} lumber {}", p.get_state(1), p.get_state(2));
                }
                i += 1;
            }
            other => panic!("unknown command {other}"),
        }
    }
    scene.update(0.02);
    scene.sync_units(&device, &queue, &mut assets);
    scene.sync_destructables(&device, &queue, &mut assets);
    let size = wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 };
    let color = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let depth = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: w3_render::gpu::DEPTH_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let cv = color.create_view(&Default::default());
    let dv = depth.create_view(&Default::default());
    let mut enc = device.create_command_encoder(&Default::default());
    scene.render(&device, &queue, &mut enc, &cv, &dv, (w, h), &mut assets);
    let stride = (w * 4).div_ceil(256) * 256;
    let buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: (stride * h) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    enc.copy_texture_to_buffer(
        color.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buf,
            layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(stride), rows_per_image: Some(h) },
        },
        size,
    );
    queue.submit([enc.finish()]);
    buf.slice(..).map_async(wgpu::MapMode::Read, |_| {});
    device.poll(wgpu::PollType::wait_indefinitely())?;
    let data = buf.slice(..).get_mapped_range().map_err(|e| anyhow::anyhow!("{e:?}"))?;
    let mut px = Vec::with_capacity((w * h * 4) as usize);
    for row in 0..h {
        let s = (row * stride) as usize;
        px.extend_from_slice(&data[s..s + (w * 4) as usize]);
    }
    for p in px.chunks_mut(4) {
        p[3] = 255;
    }
    image::save_buffer(&a[3], &px, w, h, image::ExtendedColorType::Rgba8)?;
    Ok(())
}
