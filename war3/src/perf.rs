//! A performance overlay after the likes of MangoHud and Steam's: frame rate, frame time
//! with its lows and a graph, where each frame's time goes, the GPU and display, the
//! process's and system's load, and the game's own figures.  F7 shows and hides it.
//!
//! It draws itself, apart from the game's interface: its own pipeline and an 8x8 bitmap
//! font (`font8x8`), so it reads the same over the menus and the game, and stays out of
//! screen captures.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use font8x8::UnicodeFonts;

/// The frames kept for the lows.
const HISTORY: Duration = Duration::from_secs(10);
/// The frames the figures are taken over (frame rate, averages).
const WINDOW: Duration = Duration::from_secs(1);
/// How often the figures shown are remade, as MangoHud's half second.
const REFRESH: Duration = Duration::from_millis(500);
/// How often the process and system are read.
const SAMPLE: Duration = Duration::from_secs(1);
/// The graph's frames, and its height in milliseconds.
const GRAPH: usize = 200;
const GRAPH_MS: f32 = 50.0;

/// The atlas: ASCII 32 to 126 in a 16 by 6 grid of 8x8 cells, and a solid cell (127) for
/// boxes and bars.
const COLS: u32 = 16;
const ROWS: u32 = 6;
const SOLID: char = '\u{7f}';

const WHITE: [f32; 4] = [0.92, 0.92, 0.92, 1.0];
const DIM: [f32; 4] = [0.6, 0.6, 0.62, 1.0];
const LABEL: [f32; 4] = [0.55, 0.75, 1.0, 1.0];
const GOOD: [f32; 4] = [0.4, 0.95, 0.4, 1.0];
const FAIR: [f32; 4] = [1.0, 0.85, 0.25, 1.0];
const BAD: [f32; 4] = [1.0, 0.35, 0.3, 1.0];
const PANEL: [f32; 4] = [0.0, 0.0, 0.0, 0.6];

/// Where a drawn frame's time went, in seconds.
#[derive(Clone, Copy, Default)]
pub struct Phases {
    /// Stepping the game and the menus.
    pub update: f32,
    /// Waiting for the surface's next frame (the GPU's or the display's pace).
    pub acquire: f32,
    /// Recording and submitting the frame's drawing.
    pub render: f32,
    /// Handing the frame to the display.
    pub present: f32,
}

#[derive(Clone, Copy)]
struct Frame {
    at: Instant,
    /// Since the frame before, in milliseconds.
    ms: f32,
    phases: Phases,
}

/// The figures shown, remade each `REFRESH`.
#[derive(Default)]
struct Summary {
    fps: f32,
    avg: f32,
    min: f32,
    max: f32,
    low1: f32,
    low01: f32,
    phases: Phases,
}

/// What the overlay tells of things outside it, gathered by the caller as it draws.
pub struct Info<'a> {
    pub adapter: &'a wgpu::AdapterInfo,
    pub present_mode: wgpu::PresentMode,
    pub size: (u32, u32),
    /// The display's refresh rate, in millihertz.
    pub refresh: Option<u32>,
    pub game: Option<GameInfo>,
}

pub struct GameInfo {
    /// Game seconds since the start.
    pub time: f32,
    /// 0 slow, 1 normal, 2 fast.
    pub speed: i32,
    pub units: usize,
    /// A network game: whether this computer hosts it, the turns waiting to be played,
    /// the turns played, and the players waited on.
    pub net: Option<(bool, usize, u32, usize)>,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    pos: [f32; 2],
    uv: [f32; 2],
    color: [f32; 4],
}

pub struct Perf {
    pub shown: bool,
    frames: VecDeque<Frame>,
    last: Option<Instant>,
    summary: Summary,
    summarized: Option<Instant>,
    sys: Sys,
    pipeline: wgpu::RenderPipeline,
    bind: wgpu::BindGroup,
    vbuf: wgpu::Buffer,
    vcap: usize,
}

const SHADER: &str = r#"
struct Out { @builtin(position) pos: vec4<f32>, @location(0) uv: vec2<f32>, @location(1) color: vec4<f32> };
@vertex fn vs(@location(0) pos: vec2<f32>, @location(1) uv: vec2<f32>, @location(2) color: vec4<f32>) -> Out {
    return Out(vec4<f32>(pos, 0.0, 1.0), uv, color);
}
@group(0) @binding(0) var atlas: texture_2d<f32>;
@group(0) @binding(1) var nearest: sampler;
@fragment fn fs(o: Out) -> @location(0) vec4<f32> {
    return vec4<f32>(o.color.rgb, o.color.a * textureSample(atlas, nearest, o.uv).r);
}
"#;

impl Perf {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Perf {
        let (w, h) = (COLS * 8, ROWS * 8);
        let mut pixels = vec![0u8; (w * h) as usize];
        for k in 0..COLS * ROWS {
            let c = char::from_u32(32 + k).unwrap_or(' ');
            let rows = if c == SOLID { [0xff; 8] } else { font8x8::BASIC_FONTS.get(c).unwrap_or([0; 8]) };
            let (cx, cy) = ((k % COLS) * 8, (k / COLS) * 8);
            for (y, row) in rows.iter().enumerate() {
                // Bit 0 is the leftmost pixel.
                for x in 0..8 {
                    if row & (1 << x) != 0 {
                        pixels[((cy + y as u32) * w + cx + x) as usize] = 0xff;
                    }
                }
            }
        }
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("perf font"),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo { texture: &texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            &pixels,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(w), rows_per_image: Some(h) },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        let view = texture.create_view(&Default::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor { label: Some("perf font"), ..Default::default() });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("perf"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None },
            ],
        });
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("perf"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) }, wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&sampler) }],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("perf"), source: wgpu::ShaderSource::Wgsl(SHADER.into()) });
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("perf"), bind_group_layouts: &[Some(&layout)], immediate_size: 0 });
        let attrs = wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x4];
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("perf"),
            layout: Some(&pl),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout { array_stride: std::mem::size_of::<Vertex>() as u64, step_mode: wgpu::VertexStepMode::Vertex, attributes: &attrs })],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState { format, blend: Some(wgpu::BlendState::ALPHA_BLENDING), write_mask: wgpu::ColorWrites::ALL })],
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        let vcap = 8192;
        Perf {
            shown: false,
            frames: VecDeque::new(),
            last: None,
            summary: Summary::default(),
            summarized: None,
            sys: Sys::default(),
            pipeline,
            bind,
            vbuf: Self::vbuf(device, vcap),
            vcap,
        }
    }

    fn vbuf(device: &wgpu::Device, cap: usize) -> wgpu::Buffer {
        device.create_buffer(&wgpu::BufferDescriptor { label: Some("perf vertices"), size: (cap * std::mem::size_of::<Vertex>()) as u64, usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false })
    }

    /// Notes a drawn frame begun at `start`, with where its time went.  (Kept while hidden
    /// too, so the overlay opens on figures.)
    pub fn frame(&mut self, start: Instant, phases: Phases) {
        let ms = self.last.map_or(0.0, |l| (start - l).as_secs_f32() * 1000.0);
        let first = self.last.is_none();
        self.last = Some(start);
        if first {
            return;
        }
        self.frames.push_back(Frame { at: start, ms, phases });
        while self.frames.front().is_some_and(|f| start - f.at > HISTORY) {
            self.frames.pop_front();
        }
    }

    fn summarize(&mut self, now: Instant) {
        if self.summarized.is_some_and(|t| now - t < REFRESH) {
            return;
        }
        self.summarized = Some(now);
        // (The second up to the newest frame: one long stalled shows as its own time.)
        let newest = self.frames.back().map_or(now, |f| f.at);
        let recent: Vec<&Frame> = self.frames.iter().filter(|f| newest - f.at <= WINDOW).collect();
        let mut s = Summary::default();
        if !recent.is_empty() {
            let n = recent.len() as f32;
            let total: f32 = recent.iter().map(|f| f.ms).sum();
            s.avg = total / n;
            s.fps = if total > 0.0 { n * 1000.0 / total } else { 0.0 };
            s.min = recent.iter().map(|f| f.ms).fold(f32::INFINITY, f32::min);
            s.max = recent.iter().map(|f| f.ms).fold(0.0, f32::max);
            let avg = |g: fn(&Phases) -> f32| recent.iter().map(|f| g(&f.phases)).sum::<f32>() / n * 1000.0;
            s.phases = Phases { update: avg(|p| p.update), acquire: avg(|p| p.acquire), render: avg(|p| p.render), present: avg(|p| p.present) };
        }
        // The lows: the frame rate of the slowest 1% and 0.1% of frames, as the frame time
        // at those percentiles.
        let mut all: Vec<f32> = self.frames.iter().map(|f| f.ms).collect();
        if !all.is_empty() {
            all.sort_by(|a, b| b.total_cmp(a));
            let at = |p: f32| all[((all.len() as f32 * p) as usize).min(all.len() - 1)];
            s.low1 = 1000.0 / at(0.01).max(0.001);
            s.low01 = 1000.0 / at(0.001).max(0.001);
        }
        self.summary = s;
    }

    /// Draws the overlay over the frame, if shown.
    pub fn draw(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, view: &wgpu::TextureView, info: &Info) {
        if !self.shown {
            return;
        }
        let now = Instant::now();
        self.summarize(now);
        self.sys.sample(now);
        let (w, h) = (info.size.0.max(1) as f32, info.size.1.max(1) as f32);
        // 8-pixel glyphs doubled at 1080p, tripled at 1620p and so on.
        let scale = (h / 540.0).floor().max(1.0);
        let mut q = Quads { verts: Vec::new(), scale, w, h, right: 0.0 };
        let (x0, mut y) = (12.0 * scale, 12.0 * scale);
        let top = y - 6.0 * scale;
        let line = 10.0 * scale;
        let col = x0 + 10.0 * 8.0 * scale;
        let s = &self.summary;
        let target = info.refresh.map_or(60.0, |r| r as f32 / 1000.0).min(60.0);
        let rate = |fps: f32| if fps >= target * 0.95 { GOOD } else if fps >= 30.0 { FAIR } else { BAD };

        // The panel goes under everything; its size is known once the rest is laid out.
        q.solid(0.0, 0.0, 0.0, 0.0, PANEL);
        let row = |q: &mut Quads, y: f32, label: &str, parts: &[(&str, [f32; 4])]| {
            q.text(x0, y, label, LABEL);
            let mut x = col;
            for (t, c) in parts {
                x = q.text(x, y, t, *c);
            }
        };
        row(&mut q, y, "FPS", &[(&format!("{:<6.0}", s.fps), rate(s.fps)), (&format!("{:.2} ms", s.avg), WHITE)]);
        y += line;
        row(&mut q, y, "Frametime", &[("min ", DIM), (&format!("{:<6.2}", s.min), WHITE), ("max ", DIM), (&format!("{:.2} ms", s.max), WHITE)]);
        y += line;
        row(&mut q, y, "Lows", &[("1% ", DIM), (&format!("{:<7.0}", s.low1), rate(s.low1)), ("0.1% ", DIM), (&format!("{:.0}", s.low01), rate(s.low01))]);
        y += line + 2.0 * scale;

        // The graph: a bar per frame, its height its time (to GRAPH_MS), with lines at 60
        // and 30 frames a second.
        let (gw, gh) = (GRAPH as f32 * scale, 40.0 * scale);
        q.solid(x0, y, x0 + gw, y + gh, [1.0, 1.0, 1.0, 0.06]);
        for ms in [1000.0 / 60.0, 1000.0 / 30.0] {
            let gy = y + gh - gh * ms / GRAPH_MS;
            q.solid(x0, gy, x0 + gw, gy + scale.max(1.0) * 0.5, [1.0, 1.0, 1.0, 0.25]);
        }
        let shown: Vec<f32> = self.frames.iter().rev().take(GRAPH).map(|f| f.ms).collect();
        for (k, ms) in shown.iter().enumerate() {
            let bx = x0 + gw - (k + 1) as f32 * scale;
            let bh = gh * (ms / GRAPH_MS).min(1.0);
            let c = if *ms <= 1000.0 / target * 1.05 { GOOD } else if *ms <= 1000.0 / 30.0 { FAIR } else { BAD };
            q.solid(bx, y + gh - bh, bx + scale, y + gh, c);
        }
        q.text(x0 + gw + 4.0 * scale, y, &format!("{GRAPH_MS:.0} ms"), DIM);
        q.text(x0 + gw + 4.0 * scale, y + gh - 8.0 * scale, "0", DIM);
        y += gh + 4.0 * scale;

        let p = &s.phases;
        row(&mut q, y, "CPU", &[("update ", DIM), (&format!("{:<6.2}", p.update), WHITE), ("draw ", DIM), (&format!("{:.2} ms", p.render), WHITE)]);
        y += line;
        row(&mut q, y, "Wait", &[("frame ", DIM), (&format!("{:<7.2}", p.acquire), WHITE), ("present ", DIM), (&format!("{:.2} ms", p.present), WHITE)]);
        y += line + 4.0 * scale;

        let a = info.adapter;
        row(&mut q, y, "GPU", &[(&a.name, WHITE), (&format!("  {:?}", a.backend), DIM)]);
        y += line;
        if !a.driver.is_empty() {
            row(&mut q, y, "Driver", &[(&a.driver, WHITE), (&format!("  {}", a.driver_info), DIM)]);
            y += line;
        }
        if let Some((busy, vram)) = self.sys.gpu {
            let mut parts = vec![(format!("{:<6}", format!("{busy}%")), WHITE)];
            if let Some((used, total)) = vram {
                parts.push((format!("VRAM {:.2}/{:.2} GB", gb(used), gb(total)), WHITE));
            }
            let parts: Vec<(&str, [f32; 4])> = parts.iter().map(|(t, c)| (t.as_str(), *c)).collect();
            row(&mut q, y, "GPU load", &parts);
            y += line;
        }
        let hz = info.refresh.map_or(String::new(), |r| format!(" @ {:.0} Hz", r as f32 / 1000.0));
        let vsync = match info.present_mode {
            wgpu::PresentMode::Fifo | wgpu::PresentMode::FifoRelaxed | wgpu::PresentMode::AutoVsync => "vsync on",
            _ => "vsync off",
        };
        row(&mut q, y, "Display", &[(&format!("{}x{}{hz}  ", info.size.0, info.size.1), WHITE), (vsync, DIM)]);
        y += line;
        if let Some(c) = self.sys.process {
            row(&mut q, y, "Process", &[("CPU ", DIM), (&format!("{:<6}", format!("{:.0}%", c.cpu)), WHITE), ("RAM ", DIM), (&format!("{:.0} MB", c.rss as f64 / 1048576.0), WHITE)]);
            y += line;
        }
        if let Some(c) = self.sys.system {
            row(&mut q, y, "System", &[("CPU ", DIM), (&format!("{:<6}", format!("{:.0}%", c.cpu)), WHITE), ("RAM ", DIM), (&format!("{:.1}/{:.1} GB", gb(c.used), gb(c.total)), WHITE)]);
            y += line;
        }
        if let Some(g) = &info.game {
            y += 4.0 * scale;
            let t = g.time.max(0.0) as u32;
            let speed = ["slow", "normal", "fast"].get(g.speed.clamp(0, 2) as usize).copied().unwrap_or("");
            row(&mut q, y, "Game", &[(&format!("{}:{:02}:{:02}  ", t / 3600, t / 60 % 60, t % 60), WHITE), (speed, DIM), (&format!("  {} units", g.units), WHITE)]);
            y += line;
            if let Some((host, queued, played, waiting)) = g.net {
                let waits = if waiting > 0 { BAD } else { WHITE };
                row(&mut q, y, "Net", &[(if host { "host  " } else { "player  " }, DIM), (&format!("turn {played}  queued {queued}  "), WHITE), (&format!("waiting on {waiting}"), waits)]);
                y += line;
            }
        }
        q.text(x0, y + 2.0 * scale, "F7 hides", DIM);
        y += line;
        // The panel, under all of it.
        let (right, bottom) = (q.right + 8.0 * scale, y + 4.0 * scale);
        q.set_solid(0, x0 - 6.0 * scale, top, right, bottom);

        let verts = q.verts;
        if verts.len() > self.vcap {
            self.vcap = verts.len().next_power_of_two();
            self.vbuf = Self::vbuf(device, self.vcap);
        }
        queue.write_buffer(&self.vbuf, 0, bytemuck::cast_slice(&verts));
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("perf"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment { view, depth_slice: None, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store } })],
            ..Default::default()
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind, &[]);
        pass.set_vertex_buffer(0, self.vbuf.slice(..(verts.len() * std::mem::size_of::<Vertex>()) as u64));
        pass.draw(0..verts.len() as u32, 0..1);
    }
}

fn gb(bytes: u64) -> f64 {
    bytes as f64 / (1u64 << 30) as f64
}

/// The overlay's quads in pixels (from the top left), made into the GPU's coordinates.
struct Quads {
    verts: Vec<Vertex>,
    scale: f32,
    w: f32,
    h: f32,
    /// The rightmost pixel any text reached.
    right: f32,
}

impl Quads {
    fn quad(&self, x0: f32, y0: f32, x1: f32, y1: f32, uv: [f32; 4], color: [f32; 4]) -> [Vertex; 6] {
        let (nx0, nx1) = (x0 / self.w * 2.0 - 1.0, x1 / self.w * 2.0 - 1.0);
        let (ny0, ny1) = (1.0 - y0 / self.h * 2.0, 1.0 - y1 / self.h * 2.0);
        let v = |x, y, u, v| Vertex { pos: [x, y], uv: [u, v], color };
        let (a, b, c, d) = (v(nx0, ny0, uv[0], uv[1]), v(nx1, ny0, uv[2], uv[1]), v(nx0, ny1, uv[0], uv[3]), v(nx1, ny1, uv[2], uv[3]));
        [a, c, b, b, c, d]
    }

    fn cell(c: char) -> [f32; 4] {
        let k = (c as u32).clamp(32, 127) - 32;
        let (cx, cy) = ((k % COLS) as f32, (k / COLS) as f32);
        // (Inset half a texel, against the neighbouring cells' edges.)
        let (cw, ch) = (1.0 / COLS as f32, 1.0 / ROWS as f32);
        let (ix, iy) = (0.5 / (COLS * 8) as f32, 0.5 / (ROWS * 8) as f32);
        [cx * cw + ix, cy * ch + iy, (cx + 1.0) * cw - ix, (cy + 1.0) * ch - iy]
    }

    fn solid(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, color: [f32; 4]) {
        let q = self.quad(x0, y0, x1, y1, Self::cell(SOLID), color);
        self.verts.extend(q);
    }

    /// Moves the solid quad made `k`th to a new place.
    fn set_solid(&mut self, k: usize, x0: f32, y0: f32, x1: f32, y1: f32) {
        let color = self.verts[k * 6].color;
        let q = self.quad(x0, y0, x1, y1, Self::cell(SOLID), color);
        self.verts[k * 6..k * 6 + 6].copy_from_slice(&q);
    }

    /// Writes a line from (x, y); returns where it ends.  Characters outside ASCII show as
    /// '?'.
    fn text(&mut self, mut x: f32, y: f32, s: &str, color: [f32; 4]) -> f32 {
        let g = 8.0 * self.scale;
        for c in s.chars() {
            let c = if (' '..='~').contains(&c) { c } else { '?' };
            if c != ' ' {
                // A shadow a pixel down and right, for reading over bright ground.
                let shadow = self.quad(x + self.scale, y + self.scale, x + g + self.scale, y + g + self.scale, Self::cell(c), [0.0, 0.0, 0.0, color[3] * 0.8]);
                let glyph = self.quad(x, y, x + g, y + g, Self::cell(c), color);
                self.verts.extend(shadow);
                self.verts.extend(glyph);
            }
            x += g;
        }
        self.right = self.right.max(x);
        x
    }
}

/// The process's and system's load, read from Linux's /proc and /sys (elsewhere none).
#[derive(Default)]
struct Sys {
    sampled: Option<Instant>,
    /// The last reading's CPU times: the process's, and the system's busy and total (in
    /// the kernel's ticks), and the CPUs.
    times: Option<(u64, u64, u64)>,
    cpus: usize,
    process: Option<Load>,
    system: Option<Load>,
    /// An AMD GPU's busy percent and VRAM (used, total), as amdgpu tells.
    gpu: Option<(u32, Option<(u64, u64)>)>,
}

#[derive(Clone, Copy)]
struct Load {
    /// Percent: of one CPU for the process (as top), of all of them for the system.
    cpu: f32,
    /// Bytes: the process's resident memory, or the system's in use.
    rss: u64,
    used: u64,
    total: u64,
}

impl Sys {
    fn sample(&mut self, now: Instant) {
        if self.sampled.is_some_and(|t| now - t < SAMPLE) {
            return;
        }
        self.sampled = Some(now);
        #[cfg(target_os = "linux")]
        self.sample_linux();
    }

    #[cfg(target_os = "linux")]
    fn sample_linux(&mut self) {
        let read = |p: &str| std::fs::read_to_string(p).ok();
        // /proc/stat's first line: the CPUs' ticks, idle 4th and iowait 5th.
        let stat = read("/proc/stat").unwrap_or_default();
        self.cpus = stat.lines().filter(|l| l.starts_with("cpu") && l.as_bytes().get(3).is_some_and(u8::is_ascii_digit)).count().max(1);
        let ticks: Vec<u64> = stat.lines().next().unwrap_or("").split_whitespace().skip(1).filter_map(|v| v.parse().ok()).collect();
        let total: u64 = ticks.iter().sum();
        let idle = ticks.get(3).copied().unwrap_or(0) + ticks.get(4).copied().unwrap_or(0);
        // /proc/self/stat: utime and stime, the 14th and 15th fields (after the name in
        // parentheses, which may hold spaces).
        let own = read("/proc/self/stat").and_then(|s| {
            let rest = s.rsplit_once(')')?.1.split_whitespace().collect::<Vec<_>>();
            Some(rest.get(11)?.parse::<u64>().ok()? + rest.get(12)?.parse::<u64>().ok()?)
        });
        let kb = |text: &str, key: &str| text.lines().find(|l| l.starts_with(key)).and_then(|l| l.split_whitespace().nth(1)?.parse::<u64>().ok()).map(|k| k * 1024);
        let rss = read("/proc/self/status").and_then(|s| kb(&s, "VmRSS:"));
        let mem = read("/proc/meminfo").unwrap_or_default();
        let (mem_total, mem_free) = (kb(&mem, "MemTotal:"), kb(&mem, "MemAvailable:"));
        if let (Some(own), Some((own0, busy0, total0))) = (own, self.times) {
            let dt = total.saturating_sub(total0).max(1) as f32;
            let busy = (total - idle).saturating_sub(busy0) as f32;
            self.process = Some(Load { cpu: own.saturating_sub(own0) as f32 / dt * self.cpus as f32 * 100.0, rss: rss.unwrap_or(0), used: 0, total: 0 });
            if let (Some(t), Some(f)) = (mem_total, mem_free) {
                self.system = Some(Load { cpu: busy / dt * 100.0, rss: 0, used: t.saturating_sub(f), total: t });
            }
        }
        self.times = own.map(|o| (o, total - idle, total));
        // amdgpu's figures, on the first card that has them.
        self.gpu = (0..4).find_map(|k| {
            let dir = format!("/sys/class/drm/card{k}/device");
            let busy = read(&format!("{dir}/gpu_busy_percent"))?.trim().parse().ok()?;
            let num = |f: &str| read(&format!("{dir}/{f}")).and_then(|s| s.trim().parse::<u64>().ok());
            Some((busy, num("mem_info_vram_used").zip(num("mem_info_vram_total"))))
        });
    }
}
