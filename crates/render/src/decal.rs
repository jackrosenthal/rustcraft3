//! Flat textured quads on the ground: selection circles, shadows, and splats.

use std::collections::HashMap;
use std::sync::Arc;

use bytemuck::{Pod, Zeroable};

use crate::model::{make_sampler, texture_bind_group, texture_bind_group_layout};
use crate::texture::Texture;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vertex {
    pos: [f32; 3],
    uv: [f32; 2],
    color: [f32; 4],
}

pub struct Decal {
    pub texture: Arc<Texture>,
    pub center: [f32; 3],
    pub size: [f32; 2],
    pub color: [f32; 4],
    pub additive: bool,
}

pub struct DecalRenderer {
    alpha: wgpu::RenderPipeline,
    add: wgpu::RenderPipeline,
    globals_bg: wgpu::BindGroup,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    binds: HashMap<usize, (Arc<Texture>, wgpu::BindGroup)>,
    vbuf: wgpu::Buffer,
    cap: usize,
}

impl DecalRenderer {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat, globals_buf: &wgpu::Buffer) -> Self {
        let shader = device.create_shader_module(wgpu::include_wgsl!("decal.wgsl"));
        let glayout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("decal globals"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                count: None,
            }],
        });
        let globals_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("decal globals"),
            layout: &glayout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: globals_buf.as_entire_binding() }],
        });
        let layout = texture_bind_group_layout(device);
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("decal"),
            bind_group_layouts: &[Some(&glayout), Some(&layout)],
            immediate_size: 0,
        });
        let attrs = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2, 2 => Float32x4];
        let make = |blend: wgpu::BlendState| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("decal"),
                layout: Some(&pl),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<Vertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &attrs,
                    })],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_main"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState { format, blend: Some(blend), write_mask: wgpu::ColorWrites::ALL })],
                }),
                primitive: Default::default(),
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: crate::gpu::DEPTH_FORMAT,
                    depth_write_enabled: Some(false),
                    depth_compare: Some(wgpu::CompareFunction::LessEqual),
                    stencil: Default::default(),
                    bias: wgpu::DepthBiasState { constant: -4, slope_scale: -1.0, clamp: 0.0 },
                }),
                multisample: Default::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        let alpha = make(wgpu::BlendState::ALPHA_BLENDING);
        let add = make(wgpu::BlendState {
            color: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::SrcAlpha, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add },
            alpha: wgpu::BlendComponent::OVER,
        });
        let cap = 1024;
        DecalRenderer {
            alpha,
            add,
            globals_bg,
            layout,
            sampler: make_sampler(device, false, false),
            binds: HashMap::new(),
            vbuf: Self::buffer(device, cap),
            cap,
        }
    }

    fn buffer(device: &wgpu::Device, cap: usize) -> wgpu::Buffer {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("decals"),
            size: (cap * std::mem::size_of::<Vertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    /// Draws decals; `height` gives the ground height so quads drape over terrain.
    pub fn render(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, pass: &mut wgpu::RenderPass, decals: &[Decal], height: &dyn Fn(f32, f32) -> f32) {
        if decals.is_empty() {
            return;
        }
        const N: usize = 4;
        let mut verts = Vec::with_capacity(decals.len() * N * N * 6);
        for d in decals {
            let [cx, cy, z0] = d.center;
            let (hw, hh) = (d.size[0] / 2.0, d.size[1] / 2.0);
            let at = |i: usize, j: usize| {
                let (u, v) = (i as f32 / N as f32, j as f32 / N as f32);
                let (x, y) = (cx - hw + u * d.size[0], cy + hh - v * d.size[1]);
                Vertex { pos: [x, y, height(x, y).max(z0 - 1000.0) + 2.0], uv: [u, v], color: d.color }
            };
            for j in 0..N {
                for i in 0..N {
                    verts.extend([at(i, j), at(i, j + 1), at(i + 1, j), at(i + 1, j), at(i, j + 1), at(i + 1, j + 1)]);
                }
            }
        }
        if verts.len() > self.cap {
            self.cap = verts.len().next_power_of_two();
            self.vbuf = Self::buffer(device, self.cap);
        }
        queue.write_buffer(&self.vbuf, 0, bytemuck::cast_slice(&verts));
        pass.set_bind_group(0, &self.globals_bg, &[]);
        pass.set_vertex_buffer(0, self.vbuf.slice(..));
        let per = (N * N * 6) as u32;
        for (k, d) in decals.iter().enumerate() {
            let key = Arc::as_ptr(&d.texture) as usize;
            if !self.binds.contains_key(&key) {
                let bg = texture_bind_group(device, &self.layout, &d.texture.view, &self.sampler);
                self.binds.insert(key, (d.texture.clone(), bg));
            }
            pass.set_pipeline(if d.additive { &self.add } else { &self.alpha });
            pass.set_bind_group(1, &self.binds[&key].1, &[]);
            pass.draw(k as u32 * per..(k as u32 + 1) * per, 0..1);
        }
    }
}
