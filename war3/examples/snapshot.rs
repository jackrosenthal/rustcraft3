//! Renders a map offscreen from the default game camera and writes a PNG.
//!
//! `snapshot <datadir> <map> <out.png> [width height] [x y] [distance]`

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
    let mut scene = Scene::load(&device, &queue, format, &data, &mut assets, Path::new(&a[2]))?;
    if a.len() > 7 {
        scene.camera.target.x = a[6].parse()?;
        scene.camera.target.y = a[7].parse()?;
    }
    if a.len() > 8 {
        scene.camera.distance = a[8].parse()?;
    }
    scene.update(0.5);
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
    scene.render(&device, &queue, &mut enc, &cv, &dv, w as f32 / h as f32);
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
