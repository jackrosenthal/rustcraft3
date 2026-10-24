//! Renders the glue screens offscreen and writes PNGs.
//!
//! `gluesnap <datadir> <width> <height> <commands...>` with commands
//! `wait <s>`, `click <px> <py>`, `type <text>`, and `shot <out.png>`.

use std::path::Path;

use war3::data::GameData;
use war3::glue::Glue;
use war3::scene::Assets;

fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    let mut a: Vec<String> = std::env::args().collect();
    let (w, h): (u32, u32) = (a[2].parse()?, a[3].parse()?);
    // W3CLASSIC=1 opens Reign of Chaos.
    let data = GameData::open(Path::new(&a[1]), std::env::var("W3CLASSIC").is_ok_and(|v| v == "1"))?;
    let mut desc = wgpu::InstanceDescriptor::new_without_display_handle_from_env();
    desc.backends = wgpu::Backends::PRIMARY.with_env();
    let instance = wgpu::Instance::new(desc);
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))?;
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default()))?;
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let mut assets = Assets::new(&device, &queue, &data);
    let mut glue = Glue::new(&device, &queue, format, &data, &mut assets);
    // W3SAVES: where saved games are kept, for testing.
    if let Ok(d) = std::env::var("W3SAVES") {
        glue.saves_dir = d.into();
    }
    let mut i = 4;
    while i < a.len() {
        match a[i].as_str() {
            "wait" => {
                let secs: f32 = a[i + 1].parse()?;
                let mut t = 0.0;
                // Render every step so particles and other per-frame effects advance.
                while t < secs {
                    glue.update(0.02, &mut assets);
                    shot(&device, &queue, &mut glue, &mut assets, format, w, h, "")?;
                    if let Some(war3::glue::GlueAction::PlayNet(map, lobby, role, speed, flags)) = glue.action.take() {
                        data.setup_flags.set(flags);
                        return net_game(&device, &queue, format, &data, &mut assets, &map, &lobby, role.borrow_mut().take(), speed, w, h);
                    }
                    t += 0.02;
                }
                i += 2;
            }
            // sleep <s>: real time passes, the glue updating (for network screens).
            "sleep" => {
                let end = std::time::Instant::now() + std::time::Duration::from_secs_f32(a[i + 1].parse()?);
                while std::time::Instant::now() < end {
                    glue.update(0.02, &mut assets);
                    if let Some(war3::glue::GlueAction::PlayNet(map, lobby, role, speed, flags)) = glue.action.take() {
                        data.setup_flags.set(flags);
                        return net_game(&device, &queue, format, &data, &mut assets, &map, &lobby, role.borrow_mut().take(), speed, w, h);
                    }
                    std::thread::sleep(std::time::Duration::from_millis(20));
                }
                i += 2;
            }
            // rects <text>: prints the frames whose names contain the text, with their
            // rectangles, widths, and text.
            "rects" => {
                for (k, f) in glue.tree.frames.iter().enumerate() {
                    if f.def.name.contains(a[i + 1].as_str()) {
                        eprintln!("{k} {} {:?} width {:?} text {:?}", f.def.name, f.rect, f.width, f.text);
                    }
                }
                i += 2;
            }
            // hoverframe <frame>: moves the pointer to the middle of a named frame.
            "hoverframe" => {
                if let Some(r) = glue.tree.get(&a[i + 1]).and_then(|f| glue.tree.frames[f].rect) {
                    let (ux, uy) = ((r.x0 + r.x1) / 2.0, (r.y0 + r.y1) / 2.0);
                    glue.pointer_move(ux / glue.tree.width * w as f32, (1.0 - uy / w3_ui::UI_HEIGHT) * h as f32, (w, h));
                }
                i += 2;
            }
            // press <frame>: clicks the middle of a named frame.
            "press" => {
                let rect = glue.tree.get(&a[i + 1]).and_then(|f| glue.tree.frames[f].rect);
                match rect {
                    Some(r) => {
                        let (ux, uy) = ((r.x0 + r.x1) / 2.0, (r.y0 + r.y1) / 2.0);
                        let (px, py) = (ux / glue.tree.width * w as f32, (1.0 - uy / w3_ui::UI_HEIGHT) * h as f32);
                        a.splice(i..i + 2, ["click".to_string(), px.to_string(), py.to_string()]);
                        continue;
                    }
                    None => {
                        eprintln!("no frame {}", a[i + 1]);
                        i += 2;
                    }
                }
            }
            "click" => {
                glue.pointer_move(a[i + 1].parse()?, a[i + 2].parse()?, (w, h));
                glue.pointer_down(&mut assets);
                let clicked = glue.pointer_up(&mut assets);
                eprintln!("clicked {clicked:?}");
                match glue.action.take() {
                    Some(war3::glue::GlueAction::Play(..)) => glue.show_loading(&mut assets),
                    Some(war3::glue::GlueAction::PlayNet(map, lobby, role, speed, flags)) => {
                        data.setup_flags.set(flags);
                        return net_game(&device, &queue, format, &data, &mut assets, &map, &lobby, role.borrow_mut().take(), speed, w, h);
                    }
                    Some(war3::glue::GlueAction::PlayNetSave(map, save, role, speed, me)) => {
                        return net_game_saved(&device, &queue, format, &data, &mut assets, &map, &save, role.borrow_mut().take(), speed, me, w, h);
                    }
                    Some(war3::glue::GlueAction::Campaign(map)) => {
                        eprintln!("campaign mission {map}");
                        glue.skirmish.summary = war3::skirmish::summarize_from(&data.chain, &map);
                        glue.show_loading(&mut assets);
                        glue.loading_done(&mut assets);
                    }
                    _ => {}
                }
                i += 3;
            }
            // type <text>: types characters; "\n" in the text is Enter.
            "type" => {
                for c in a[i + 1].replace("\\n", "\r").chars() {
                    glue.key(c, &mut assets);
                }
                i += 2;
            }
            // cinematic <model>: plays a model cinematic (PlayModelCinematic).
            "cinematic" => {
                glue.play_cinematic(a[i + 1].clone(), None, None, &mut assets);
                i += 2;
            }
            "move" => {
                glue.pointer_move(a[i + 1].parse()?, a[i + 2].parse()?, (w, h));
                i += 3;
            }
            "sprites" => {
                for (i, m, x, y) in glue.tree.sprites() {
                    eprintln!("sprite {} {m} at {x},{y} rect {:?}", glue.tree.frames[i].name, glue.tree.frames[i].rect);
                }
                if let Some(i) = glue.tree.get("WarCraftIIILogo") {
                    let f = &glue.tree.frames[i];
                    eprintln!("logo visible {} model {:?} rect {:?} points {:?} w {:?} h {:?}", glue.tree.is_visible(i), f.model, f.rect, f.points.len(), f.width, f.height);
                }
                i += 1;
            }
            // score <map> <seconds>: plays the map that long, then shows its score screen
            // with player 1 the winner.
            "score" => {
                let mut scene = war3::scene::Scene::load(&device, &queue, format, &data, &mut assets, Path::new(&a[i + 1]), w as f32 / h as f32, None, None, None)?;
                let secs: f32 = a[i + 2].parse()?;
                let mut t = 0.0;
                while t < secs {
                    scene.update(0.05);
                    t += 0.05;
                }
                let g = scene.game.as_mut().unwrap();
                g.world.player_results.insert(0, 0);
                let d = war3::scorescreen::ScoreData::from_world(&g.world);
                glue.show_score(d, None, &mut assets);
                i += 3;
            }
            "shot" => {
                shot(&device, &queue, &mut glue, &mut assets, format, w, h, &a[i + 1])?;
                i += 2;
            }
            other => anyhow::bail!("unknown command {other}"),
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn shot(device: &wgpu::Device, queue: &wgpu::Queue, glue: &mut Glue, assets: &mut Assets, format: wgpu::TextureFormat, w: u32, h: u32, out: &str) -> anyhow::Result<()> {
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
    glue.render(device, queue, &mut enc, &cv, &dv, (w, h), assets);
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
    if !out.is_empty() {
        image::save_buffer(&out, &px, w, h, image::ExtendedColorType::Rgba8)?;
    }
    Ok(())
}

/// A network game the glue started: plays it in real time for W3NETPLAY seconds (20
/// without), then prints the players and a digest of the game to compare.
#[allow(clippy::too_many_arguments)]
#[allow(clippy::too_many_arguments)]
fn net_game(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat, data: &GameData, assets: &mut Assets, map: &Path, lobby: &[w3_game::LobbySlot], role: Option<war3::net::Role>, speed: u8, w: u32, h: u32) -> anyhow::Result<()> {
    let scene = war3::scene::Scene::load(device, queue, format, data, assets, map, w as f32 / h as f32, Some(lobby), None, None)?;
    play_net(scene, role, speed)
}

/// A loaded network game, as the application starts one.
#[allow(clippy::too_many_arguments)]
fn net_game_saved(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat, data: &GameData, assets: &mut Assets, map: &Path, save: &[u8], role: Option<war3::net::Role>, speed: u8, me: u8, w: u32, h: u32) -> anyhow::Result<()> {
    let save = w3_game::save::SaveGame::parse(save)?;
    let mut scene = war3::scene::Scene::load(device, queue, format, data, assets, map, w as f32 / h as f32, None, Some(save), None)?;
    if let Some(g) = scene.game.as_mut() {
        g.world.local_player = me;
        println!("loaded at {:.1}s", g.world.time);
    }
    play_net(scene, role, speed)
}

fn play_net(mut scene: war3::scene::Scene, role: Option<war3::net::Role>, speed: u8) -> anyhow::Result<()> {
    if let Some(r) = role {
        scene.start_net(r, speed);
    }
    // W3NETFRAMES: the frames to play before comparing (400 without).
    let frames: u32 = std::env::var("W3NETFRAMES").ok().and_then(|s| s.parse().ok()).unwrap_or(400);
    if let Some(s) = scene.net.as_mut() {
        s.limit = Some(frames);
    }
    let secs: f32 = std::env::var("W3NETPLAY").ok().and_then(|s| s.parse().ok()).unwrap_or(60.0);
    let start = std::time::Instant::now();
    let mut last = start;
    while start.elapsed().as_secs_f32() < secs && scene.net.as_ref().is_some_and(|s| s.played < frames) {
        let now = std::time::Instant::now();
        scene.update((now - last).as_secs_f32().min(0.1));
        last = now;
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let g = scene.game.as_ref().unwrap();
    let wd = &g.world;
    let mut digest = 0u64;
    for &hd in &wd.unit_order {
        if let Some(u) = wd.unit(hd) {
            let line = format!("{hd} {:.3} {:.3} {:.3}", u.x, u.y, u.life);
            digest = line.bytes().fold(digest.rotate_left(5), |d, b| d.wrapping_mul(31).wrapping_add(b as u64));
        }
    }
    let played = scene.net.as_ref().map_or(0, |s| s.played);
    println!("netgame local {} played {played} units {} digest {digest:016x}", wd.local_player, wd.unit_order.len());
    for p in 0..2 {
        println!("p{p} {} gold {:?}", wd.players[p].name, wd.players[p].state.get(&1));
    }
    Ok(())
}
