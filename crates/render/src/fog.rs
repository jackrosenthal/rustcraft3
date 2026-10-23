//! The fog-of-war texture shared by the terrain and model shaders: one byte per fog cell,
//! 255 visible, 0 masked, and the fogged level between.

pub struct FogTexture {
    texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub sampler: wgpu::Sampler,
    pub size: (u32, u32),
}

impl FogTexture {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, w: u32, h: u32) -> FogTexture {
        let (w, h) = (w.max(1), h.max(1));
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("fog of war"),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        let sampler = crate::model::make_sampler(device, false, false);
        let fog = FogTexture { texture, view, sampler, size: (w, h) };
        fog.write(queue, &vec![255; (w * h) as usize]);
        fog
    }

    /// Replaces the contents; `data` has one byte per cell, rows from the south edge.
    pub fn write(&self, queue: &wgpu::Queue, data: &[u8]) {
        let (w, h) = self.size;
        if data.len() < (w * h) as usize {
            return;
        }
        queue.write_texture(
            self.texture.as_image_copy(),
            &data[..(w * h) as usize],
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(w), rows_per_image: Some(h) },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
    }
}

pub fn layout_entries(first: u32) -> [wgpu::BindGroupLayoutEntry; 2] {
    [
        wgpu::BindGroupLayoutEntry {
            binding: first,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        },
        wgpu::BindGroupLayoutEntry {
            binding: first + 1,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            count: None,
        },
    ]
}
