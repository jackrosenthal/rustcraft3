//! GPU textures loaded from the data archives.

use std::collections::HashMap;
use std::sync::Arc;

use w3_mpq::Chain;

pub struct Texture {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub width: u32,
    pub height: u32,
}

/// Normalizes a texture path and lists the names to try, in order.  Assets often refer
/// to `.tga` files that only exist as `.blp`, or omit the extension.
pub fn candidates(path: &str) -> Vec<String> {
    let p = path.trim().replace('/', "\\");
    let lower = p.to_ascii_lowercase();
    let mut out = vec![p.clone()];
    if let Some(dot) = lower.rfind('.').filter(|&d| d > lower.rfind('\\').unwrap_or(0)) {
        if &lower[dot..] != ".blp" {
            out.push(format!("{}.blp", &p[..dot]));
        }
    } else {
        out.insert(0, format!("{p}.blp"));
    }
    out
}

pub fn upload(device: &wgpu::Device, queue: &wgpu::Queue, label: &str, mips: &[w3_formats::blp::Image]) -> Texture {
    let (w, h) = (mips[0].width, mips[0].height);
    // Keep only a consistent mip chain.
    let n = mips
        .iter()
        .enumerate()
        .take_while(|(i, m)| m.width == (w >> i).max(1) && m.height == (h >> i).max(1))
        .count()
        .max(1);
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        mip_level_count: n as u32,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    for (level, m) in mips.iter().take(n).enumerate() {
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: level as u32,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &m.rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(m.width * 4),
                rows_per_image: Some(m.height),
            },
            wgpu::Extent3d { width: m.width, height: m.height, depth_or_array_layers: 1 },
        );
    }
    let view = texture.create_view(&Default::default());
    Texture { texture, view, width: w, height: h }
}

/// A single-channel texture, returned as a view.
pub fn upload_r8(device: &wgpu::Device, queue: &wgpu::Queue, label: &str, w: u32, h: u32, data: &[u8]) -> wgpu::TextureView {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::R8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        texture.as_image_copy(),
        data,
        wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(w), rows_per_image: Some(h) },
        wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
    );
    texture.create_view(&Default::default())
}

pub struct TextureCache {
    pub chain: Arc<Chain>,
    cache: HashMap<String, Option<Arc<Texture>>>,
    pub white: Arc<Texture>,
}

impl TextureCache {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, chain: Arc<Chain>) -> Self {
        let white = Arc::new(upload(
            device,
            queue,
            "white",
            &[w3_formats::blp::Image { width: 1, height: 1, rgba: vec![255; 4] }],
        ));
        TextureCache { chain, cache: HashMap::new(), white }
    }

    pub fn get(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, path: &str) -> Option<Arc<Texture>> {
        let key = path.to_ascii_lowercase().replace('/', "\\");
        if let Some(t) = self.cache.get(&key) {
            return t.clone();
        }
        let tex = candidates(path).into_iter().find_map(|name| {
            let data = self.chain.read(&name).ok()?;
            match w3_formats::blp::decode_texture(&name, &data) {
                Ok(mips) => Some(Arc::new(upload(device, queue, &name, &mips))),
                Err(e) => {
                    log::warn!("texture {name}: {e}");
                    None
                }
            }
        });
        if tex.is_none() && !path.is_empty() {
            log::debug!("missing texture {path}");
        }
        self.cache.insert(key, tex.clone());
        tex
    }

    /// Creates or updates a texture made at run time (such as the minimap fog), which
    /// interface draws then name like an archive file.
    pub fn set_dynamic(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, name: &str, w: u32, h: u32, rgba: &[u8]) {
        let key = name.to_ascii_lowercase();
        if let Some(Some(t)) = self.cache.get(&key) {
            if t.width == w && t.height == h {
                queue.write_texture(
                    t.texture.as_image_copy(),
                    rgba,
                    wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(w * 4), rows_per_image: Some(h) },
                    wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                );
                return;
            }
        }
        let t = upload(device, queue, name, &[w3_formats::blp::Image { width: w, height: h, rgba: rgba.to_vec() }]);
        self.cache.insert(key, Some(Arc::new(t)));
    }

    pub fn get_or_white(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, path: &str) -> Arc<Texture> {
        self.get(device, queue, path).unwrap_or_else(|| self.white.clone())
    }
}
