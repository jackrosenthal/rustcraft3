//! Renders the glue screens offscreen and writes PNGs.
//!
//! `gluesnap <datadir> <width> <height> <commands...>` with commands
//! `wait <s>`, `click <px> <py>`, `key <c>`, and `shot <out.png>`.

use std::path::Path;

use war3::data::GameData;
use war3::glue::Glue;
use war3::scene::Assets;

fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    let a: Vec<String> = std::env::args().collect();
    let (w, h): (u32, u32) = (a[2].parse()?, a[3].parse()?);
    let data = GameData::open(Path::new(&a[1]), false)?;
    let mut desc = wgpu::InstanceDescriptor::new_without_display_handle_from_env();
    desc.backends = wgpu::Backends::PRIMARY.with_env();
    let instance = wgpu::Instance::new(desc);
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))?;
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default()))?;
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let mut assets = Assets::new(&device, &queue, &data);
    let mut glue = Glue::new(&device, &queue, format, &data, &mut assets);
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
                    t += 0.02;
                }
                i += 2;
            }
            "click" => {
                glue.pointer_move(a[i + 1].parse()?, a[i + 2].parse()?, (w, h));
                glue.pointer_down(&mut assets);
                let clicked = glue.pointer_up(&mut assets);
                eprintln!("clicked {clicked:?}");
                match glue.action.take() {
                    Some(war3::glue::GlueAction::Play(..)) => glue.show_loading(&mut assets),
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
