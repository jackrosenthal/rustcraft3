//! Draws interface draw lists: textured quads and text.

use std::collections::HashMap;
use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use w3_ui::{Blend, Draw, Justify};

use crate::model::{make_sampler, texture_bind_group, texture_bind_group_layout};
use crate::texture::{Texture, TextureCache};

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vertex {
    pos: [f32; 2],
    uv: [f32; 2],
    color: [f32; 4],
    /// 0: image, 1: text (atlas alpha), 2: alpha-keyed image
    mode: u32,
    _pad: u32,
}

const ATLAS: u32 = 2048;

struct Glyph {
    uv: [f32; 4],
    w: f32,
    h: f32,
    xmin: f32,
    ymin: f32,
    advance: f32,
}

struct Atlas {
    texture: wgpu::Texture,
    bind: wgpu::BindGroup,
    x: u32,
    y: u32,
    row_h: u32,
    glyphs: HashMap<(usize, u32, char), Glyph>,
}

struct Batch {
    bind: BindKey,
    add: bool,
    first: u32,
    count: u32,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum BindKey {
    Texture(usize),
    Atlas,
}

pub struct UiRenderer {
    alpha: wgpu::RenderPipeline,
    add: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    wrap_sampler: wgpu::Sampler,
    binds: HashMap<usize, (Arc<Texture>, wgpu::BindGroup)>,
    atlas: Atlas,
    fonts: Vec<(String, fontdue::Font)>,
    vbuf: wgpu::Buffer,
    vcap: usize,
}

impl UiRenderer {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::include_wgsl!("ui.wgsl"));
        let layout = texture_bind_group_layout(device);
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ui"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let attrs = wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x4, 3 => Uint32];
        let make = |blend: wgpu::BlendState| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("ui"),
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
                depth_stencil: None,
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
        let sampler = make_sampler(device, false, false);
        let wrap_sampler = make_sampler(device, true, true);
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("glyphs"),
            size: wgpu::Extent3d { width: ATLAS, height: ATLAS, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        let bind = texture_bind_group(device, &layout, &view, &sampler);
        let vcap = 4096;
        let vbuf = Self::vbuf(device, vcap);
        UiRenderer {
            alpha,
            add,
            layout,
            sampler,
            wrap_sampler,
            binds: HashMap::new(),
            atlas: Atlas { texture, bind, x: 0, y: 0, row_h: 0, glyphs: HashMap::new() },
            fonts: Vec::new(),
            vbuf,
            vcap,
        }
    }

    fn vbuf(device: &wgpu::Device, cap: usize) -> wgpu::Buffer {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ui vertices"),
            size: (cap * std::mem::size_of::<Vertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    fn font(&mut self, textures: &TextureCache, path: &str) -> Option<usize> {
        if let Some(i) = self.fonts.iter().position(|(p, _)| p.eq_ignore_ascii_case(path)) {
            return Some(i);
        }
        let bytes = textures.chain.read(path).ok()?;
        let f = fontdue::Font::from_bytes(bytes, fontdue::FontSettings::default()).ok()?;
        self.fonts.push((path.to_string(), f));
        Some(self.fonts.len() - 1)
    }

    fn glyph(&mut self, queue: &wgpu::Queue, font: usize, px: u32, c: char) -> Option<&Glyph> {
        let key = (font, px, c);
        if !self.atlas.glyphs.contains_key(&key) {
            let (m, bitmap) = self.fonts[font].1.rasterize(c, px as f32);
            let (w, h) = (m.width as u32, m.height as u32);
            let a = &mut self.atlas;
            if a.x + w + 1 > ATLAS {
                a.x = 0;
                a.y += a.row_h + 1;
                a.row_h = 0;
            }
            if a.y + h + 1 > ATLAS {
                // Out of space: start over.
                a.glyphs.clear();
                a.x = 0;
                a.y = 0;
                a.row_h = 0;
            }
            if w > 0 && h > 0 {
                queue.write_texture(
                    wgpu::TexelCopyTextureInfo { texture: &a.texture, mip_level: 0, origin: wgpu::Origin3d { x: a.x, y: a.y, z: 0 }, aspect: wgpu::TextureAspect::All },
                    &bitmap,
                    wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(w), rows_per_image: Some(h) },
                    wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                );
            }
            let s = ATLAS as f32;
            a.glyphs.insert(key, Glyph {
                uv: [a.x as f32 / s, a.y as f32 / s, (a.x + w) as f32 / s, (a.y + h) as f32 / s],
                w: w as f32,
                h: h as f32,
                xmin: m.xmin as f32,
                ymin: m.ymin as f32,
                advance: m.advance_width,
            });
            a.x += w + 1;
            a.row_h = a.row_h.max(h);
        }
        self.atlas.glyphs.get(&key)
    }

    /// Draws a list in interface coordinates.  `ui_width` is the interface width (0.8 at
    /// 4:3); the height is always 0.6.
    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        textures: &mut TextureCache,
        pass: &mut wgpu::RenderPass,
        draws: &[Draw],
        ui_width: f32,
        screen: (u32, u32),
    ) {
        let (sw, sh) = (screen.0 as f32, screen.1 as f32);
        // Interface units to normalized device coordinates.
        let nx = |x: f32| x / ui_width * 2.0 - 1.0;
        let ny = |y: f32| y / w3_ui::UI_HEIGHT * 2.0 - 1.0;
        let px_per_unit = sh / w3_ui::UI_HEIGHT;
        let mut verts: Vec<Vertex> = Vec::new();
        let mut batches: Vec<Batch> = Vec::new();
        let push_quad = |verts: &mut Vec<Vertex>, batches: &mut Vec<Batch>, bind: BindKey, add: bool, r: [f32; 4], uv: [[f32; 2]; 4], color: [f32; 4], mode: u32| {
            let [x0, y0, x1, y1] = r;
            let v = |x: f32, y: f32, uv: [f32; 2]| Vertex { pos: [x, y], uv, color, mode, _pad: 0 };
            // uv corners: top-left, top-right, bottom-left, bottom-right
            let q = [v(x0, y1, uv[0]), v(x1, y1, uv[1]), v(x0, y0, uv[2]), v(x1, y0, uv[3])];
            let first = verts.len() as u32;
            verts.extend([q[0], q[2], q[1], q[1], q[2], q[3]]);
            match batches.last_mut() {
                Some(b) if b.bind == bind && b.add == add => b.count += 6,
                _ => batches.push(Batch { bind, add, first, count: 6 }),
            }
        };
        for d in draws {
            match d {
                Draw::Image { texture, rect, uv, color, blend, rotated } => {
                    // An empty texture name draws a solid color.
                    let t = if texture.is_empty() { textures.white.clone() } else {
                        let Some(t) = textures.get(device, queue, texture) else { continue };
                        t
                    };
                    let key = Arc::as_ptr(&t) as usize;
                    let tiled = uv.iter().any(|v| *v > 1.0 || *v < 0.0);
                    if !self.binds.contains_key(&key) {
                        let s = if tiled { &self.wrap_sampler } else { &self.sampler };
                        let bg = texture_bind_group(device, &self.layout, &t.view, s);
                        self.binds.insert(key, (t.clone(), bg));
                    }
                    let [l, tp, r, b] = *uv;
                    let corners = if *rotated {
                        // Screen x follows v; screen y follows u.
                        [[r, tp], [r, b], [l, tp], [l, b]]
                    } else {
                        [[l, tp], [r, tp], [l, b], [r, b]]
                    };
                    let mode = if *blend == Blend::AlphaKey { 2 } else { 0 };
                    push_quad(&mut verts, &mut batches, BindKey::Texture(key), *blend == Blend::Add, [nx(rect.x0), ny(rect.y0), nx(rect.x1), ny(rect.y1)], corners, *color, mode);
                }
                Draw::Text { font, size, rect, text, color, justify_h, justify_v, shadow } => {
                    let Some(fi) = self.font(textures, font) else { continue };
                    let px = (size * px_per_unit).round().max(1.0) as u32;
                    // Measure.
                    let lines: Vec<&str> = text.split('\n').collect();
                    let line_h = px as f32 * 1.2;
                    let mut widths = Vec::new();
                    for line in &lines {
                        let mut w = 0.0;
                        for c in strip_colors(line).chars() {
                            if let Some(g) = self.glyph(queue, fi, px, c) {
                                w += g.advance;
                            }
                        }
                        widths.push(w);
                    }
                    let to_px_x = |x: f32| x * sw / ui_width;
                    let to_px_y = |y: f32| sh - y * px_per_unit;
                    let (rx0, rx1) = (to_px_x(rect.x0), to_px_x(rect.x1));
                    let (ry_top, ry_bottom) = (to_px_y(rect.y1), to_px_y(rect.y0));
                    let total_h = line_h * lines.len() as f32;
                    let y = match justify_v {
                        Justify::Start => ry_top,
                        Justify::End => ry_bottom - total_h,
                        Justify::Middle => (ry_top + ry_bottom - total_h) / 2.0,
                    };
                    let passes: Vec<([f32; 4], f32, f32, bool)> = match shadow {
                        Some((c, dx, dy)) => vec![(*c, dx * px_per_unit, -dy * px_per_unit, true), (*color, 0.0, 0.0, false)],
                        None => vec![(*color, 0.0, 0.0, false)],
                    };
                    let y_start = y;
                    for (base_color, ox, oy, is_shadow) in passes {
                    let mut y = y_start + oy;
                    for (li, line) in lines.iter().enumerate() {
                        let mut x = ox + match justify_h {
                            Justify::Start => rx0,
                            Justify::End => rx1 - widths[li],
                            Justify::Middle => (rx0 + rx1 - widths[li]) / 2.0,
                        };
                        let baseline = y + px as f32;
                        let mut col = base_color;
                        let mut chars = line.chars().peekable();
                        while let Some(c) = chars.next() {
                            // |cAARRGGBB ... |r color codes.
                            if c == '|' {
                                match chars.peek() {
                                    Some('c') | Some('C') => {
                                        chars.next();
                                        let hex: String = chars.by_ref().take(8).collect();
                                        if let (Ok(v), false) = (u32::from_str_radix(&hex, 16), is_shadow) {
                                            col = [((v >> 16) & 0xff) as f32 / 255.0, ((v >> 8) & 0xff) as f32 / 255.0, (v & 0xff) as f32 / 255.0, (v >> 24) as f32 / 255.0];
                                        }
                                        continue;
                                    }
                                    Some('r') | Some('R') => {
                                        chars.next();
                                        col = base_color;
                                        continue;
                                    }
                                    _ => {}
                                }
                            }
                            let Some(g) = self.glyph(queue, fi, px, c) else { continue };
                            let (gx0, gy1) = (x + g.xmin, baseline - g.ymin);
                            let (gx1, gy0) = (gx0 + g.w, gy1 - g.h);
                            let uv = g.uv;
                            let adv = g.advance;
                            if g.w > 0.0 {
                                let r = [gx0 / sw * 2.0 - 1.0, 1.0 - gy1 / sh * 2.0, gx1 / sw * 2.0 - 1.0, 1.0 - gy0 / sh * 2.0];
                                push_quad(&mut verts, &mut batches, BindKey::Atlas, false, r, [[uv[0], uv[1]], [uv[2], uv[1]], [uv[0], uv[3]], [uv[2], uv[3]]], col, 1);
                            }
                            x += adv;
                        }
                        y += line_h;
                    }
                    }
                }
            }
        }
        if verts.is_empty() {
            return;
        }
        if verts.len() > self.vcap {
            self.vcap = verts.len().next_power_of_two();
            self.vbuf = Self::vbuf(device, self.vcap);
        }
        queue.write_buffer(&self.vbuf, 0, bytemuck::cast_slice(&verts));
        pass.set_vertex_buffer(0, self.vbuf.slice(..));
        for b in &batches {
            pass.set_pipeline(if b.add { &self.add } else { &self.alpha });
            match b.bind {
                BindKey::Atlas => pass.set_bind_group(0, &self.atlas.bind, &[]),
                BindKey::Texture(k) => pass.set_bind_group(0, &self.binds[&k].1, &[]),
            }
            pass.draw(b.first..b.first + b.count, 0..1);
        }
    }
}

fn strip_colors(s: &str) -> String {
    let mut out = String::new();
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c == '|' {
            match it.peek() {
                Some('c') | Some('C') => {
                    it.next();
                    for _ in 0..8 {
                        it.next();
                    }
                    continue;
                }
                Some('r') | Some('R') => {
                    it.next();
                    continue;
                }
                _ => {}
            }
        }
        out.push(c);
    }
    out
}
