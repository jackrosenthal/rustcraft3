//! Ground tiles and water for a map's `war3map.w3e` terrain.

use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use glam::Vec3;
use w3_formats::map::Terrain;
use w3_formats::profile::Profile;
use w3_formats::slk::Table;

use crate::model::{make_sampler, texture_bind_group, texture_bind_group_layout};
use crate::texture::{Texture, TextureCache};

pub const TILE: f32 = 128.0;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vertex {
    pos: [f32; 3],
    normal: [f32; 3],
    uv: [f32; 2],
    color: [f32; 4],
}

struct Batch {
    texture: usize,
    first: u32,
    count: u32,
    blended: bool,
}

pub struct TerrainRenderer {
    opaque: wgpu::RenderPipeline,
    cliffs: wgpu::RenderPipeline,
    blended: wgpu::RenderPipeline,
    water_pipeline: wgpu::RenderPipeline,
    globals_bg: wgpu::BindGroup,
    globals_layout: wgpu::BindGroupLayout,
    globals_buf: wgpu::Buffer,
    extent_buf: wgpu::Buffer,
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    /// The ground tiles, in their own buffers so blight can change them.
    ground_vertices: wgpu::Buffer,
    ground_indices: wgpu::Buffer,
    ground: GroundSource,
    batches: Vec<Batch>,
    texture_bgs: Vec<wgpu::BindGroup>,
    cliff_batches: Vec<Batch>,
    cliff_bgs: Vec<wgpu::BindGroup>,
    shadow_bg: wgpu::BindGroup,
    shadow_layout: wgpu::BindGroupLayout,
    shadow_sampler: wgpu::Sampler,
    /// The map's baked shadows (war3map.shd), one byte per 32 by 32 cell.
    pub shadow_base: Vec<u8>,
    water_textures: Vec<wgpu::BindGroup>,
    water_range: (u32, u32),
    water_rate: f32,
    pub width: u32,
    pub height: u32,
    pub offset: [f32; 2],
    heights: Vec<f32>,
}

/// What the ground mesh is built from: each corner's ground texture and variation,
/// which tiles are cliffs, and which textures are extended (32 cells).
struct GroundSource {
    width: u32,
    height: u32,
    offset: [f32; 2],
    textures: Vec<u8>,
    variations: Vec<u8>,
    cliff_tiles: Vec<bool>,
    extended: Vec<bool>,
    blight_index: usize,
}

/// The ground tiles: per tile, the lowest texture index in full and every other texture
/// over it by which corners have it (blighted corners take the blight texture).
fn build_ground(src: &GroundSource, heights: &[f32], blight: &[bool]) -> (Vec<Vertex>, Vec<u32>, Vec<Batch>) {
    let (w, h) = (src.width, src.height);
    let idx = |x: u32, y: u32| (y.min(h - 1) * w + x.min(w - 1)) as usize;
    let hat = |x: u32, y: u32| heights[idx(x, y)];
    let corner_tex = |x: u32, y: u32| -> usize {
        let i = idx(x, y);
        if blight.get(i).copied().unwrap_or(false) { src.blight_index } else { src.textures[i] as usize }
    };
    let normal_at = |x: u32, y: u32| -> [f32; 3] {
        let l = hat(x.saturating_sub(1), y);
        let r = hat(x + 1, y);
        let d = hat(x, y.saturating_sub(1));
        let u = hat(x, y + 1);
        Vec3::new(l - r, d - u, 2.0 * TILE).normalize().to_array()
    };
    let n = src.extended.len();
    let mut verts: Vec<Vertex> = Vec::new();
    let mut per_tex_base: Vec<Vec<u32>> = vec![Vec::new(); n];
    let mut per_tex_overlay: Vec<Vec<u32>> = vec![Vec::new(); n];
    for y in 0..h - 1 {
        for x in 0..w - 1 {
            if src.cliff_tiles[(y * (w - 1) + x) as usize] {
                continue;
            }
            let bl = corner_tex(x, y);
            let br = corner_tex(x + 1, y);
            let tl = corner_tex(x, y + 1);
            let tr = corner_tex(x + 1, y + 1);
            let mut texs = vec![bl, br, tl, tr];
            texs.sort_unstable();
            texs.dedup();
            let corners = [(x, y + 1), (x + 1, y + 1), (x, y), (x + 1, y)];
            for (k, &t) in texs.iter().enumerate() {
                let ext = src.extended.get(t).copied().unwrap_or(false);
                let cell = if k == 0 {
                    full_variation(ext, src.variations[idx(x, y)])
                } else {
                    (br == t) as u32 | ((bl == t) as u32) << 1 | ((tr == t) as u32) << 2 | ((tl == t) as u32) << 3
                };
                let cols = if ext { 8.0 } else { 4.0 };
                let cx = (cell % 4 + (cell / 16) * 4) as f32;
                let cy = ((cell % 16) / 4) as f32;
                let (u0, v0) = (cx / cols, cy / 4.0);
                let (du, dv) = (1.0 / cols, 0.25);
                let base = verts.len() as u32;
                // top-left, top-right, bottom-left, bottom-right
                let uvs = [[u0, v0], [u0 + du, v0], [u0, v0 + dv], [u0 + du, v0 + dv]];
                for (c, &(cx, cy)) in corners.iter().enumerate() {
                    verts.push(Vertex {
                        pos: [src.offset[0] + cx as f32 * TILE, src.offset[1] + cy as f32 * TILE, hat(cx, cy)],
                        normal: normal_at(cx, cy),
                        uv: uvs[c],
                        color: [1.0; 4],
                    });
                }
                let quad = [base + 2, base + 3, base + 1, base + 2, base + 1, base];
                if k == 0 {
                    per_tex_base[t].extend(quad);
                } else {
                    per_tex_overlay[t].extend(quad);
                }
            }
        }
    }
    let mut indices: Vec<u32> = Vec::new();
    let mut batches = Vec::new();
    for (blended, lists) in [(false, &per_tex_base), (true, &per_tex_overlay)] {
        for (t, list) in lists.iter().enumerate() {
            if !list.is_empty() {
                batches.push(Batch { texture: t, first: indices.len() as u32, count: list.len() as u32, blended });
                indices.extend(list);
            }
        }
    }
    if verts.is_empty() {
        verts.push(Vertex::zeroed());
    }
    if indices.is_empty() {
        indices.push(0);
    }
    (verts, indices, batches)
}

/// Index of the tile cell in a ground texture for a full tile of the given variation.
fn full_variation(extended: bool, variation: u8) -> u32 {
    if extended {
        match variation {
            v if v < 16 => 16 + v as u32,
            16 => 15,
            _ => 0,
        }
    } else if variation == 0 {
        0
    } else {
        15
    }
}

impl TerrainRenderer {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        textures: &mut TextureCache,
        color_format: wgpu::TextureFormat,
        globals_buf: &wgpu::Buffer,
        terrain: &Terrain,
        terrain_slk: &Table,
        water_slk: &Table,
        cliff_slk: &Table,
        misc: &Profile,
        load_mdx: &mut dyn FnMut(&str) -> Option<w3_formats::mdx::Model>,
        shadows: Option<&[u8]>,
    ) -> Self {
        use wgpu::util::DeviceExt;
        let (w, h) = (terrain.width, terrain.height);
        let letters = w3_formats::map::ramp_letters(terrain);
        let letter = |x: u32, y: u32| letters[(y.min(h - 1) * w + x.min(w - 1)) as usize];
        // A middle ramp corner sits half a level above its stored layer.
        let heights: Vec<f32> = terrain.corners.iter().zip(&letters).map(|(c, &l)| c.height() + if l == b'R' { 64.0 } else { 0.0 }).collect();
        let hat = |x: u32, y: u32| heights[(y.min(h - 1) * w + x.min(w - 1)) as usize];

        // Ground textures in tileset order; blight is drawn above all of them.
        let mut ground: Vec<(Arc<Texture>, bool)> = terrain
            .ground_tilesets
            .iter()
            .map(|id| {
                let key = String::from_utf8_lossy(id).into_owned();
                let dir = terrain_slk.get(&key, "dir").map(|v| v.as_str()).unwrap_or_default();
                let file = terrain_slk.get(&key, "file").map(|v| v.as_str()).unwrap_or_default();
                let t = textures.get_or_white(device, queue, &format!("{dir}\\{file}.blp"));
                let ext = t.width > t.height;
                (t, ext)
            })
            .collect();
        let blight_index = ground.len();
        // The tileset's blight (WorldEditData [TileSets]: name, blight texture), else
        // [Blight] Texture.
        let blight_path = misc
            .get_list("TileSets", &(terrain.tileset as char).to_string())
            .get(1)
            .map(|s| s.trim().to_string())
            .or_else(|| misc.get_str("Blight", "Texture"))
            .unwrap_or_default();
        let blight = textures.get_or_white(device, queue, &format!("{blight_path}.blp"));
        let blight_ext = blight.width > blight.height;
        ground.push((blight, blight_ext));

        let mut verts: Vec<Vertex> = Vec::new();
        // A tile on a ramp (any corner H, L, R, or S) is ground, not cliff.
        let on_ramp = |x: u32, y: u32| [(x, y), (x + 1, y), (x, y + 1), (x + 1, y + 1)].iter().any(|&(cx, cy)| matches!(letter(cx, cy), b'H' | b'L' | b'R' | b'S'));
        let is_cliff = |x: u32, y: u32| {
            let l = [(x, y), (x + 1, y), (x, y + 1), (x + 1, y + 1)].map(|(cx, cy)| terrain.corner(cx, cy).layer_height);
            l.iter().any(|&v| v != l[0]) && !on_ramp(x, y)
        };
        // Ground height without cliff layers, which cliff models are draped over.
        let ground_only: Vec<f32> = terrain
            .corners
            .iter()
            .map(|c| (c.ground_height as f32 - 8192.0) / 4.0)
            .collect();
        let ground_at = |fx: f32, fy: f32| -> f32 {
            let fx = fx.clamp(0.0, (w - 1) as f32);
            let fy = fy.clamp(0.0, (h - 1) as f32);
            let (ix, iy) = (fx.floor() as u32, fy.floor() as u32);
            let (tx, ty) = (fx - ix as f32, fy - iy as f32);
            let at = |x: u32, y: u32| ground_only[(y.min(h - 1) * w + x.min(w - 1)) as usize];
            let a = at(ix, iy) * (1.0 - tx) + at(ix + 1, iy) * tx;
            let b = at(ix, iy + 1) * (1.0 - tx) + at(ix + 1, iy + 1) * tx;
            a * (1.0 - ty) + b * ty
        };
        let ground_src = GroundSource {
            width: w,
            height: h,
            offset: terrain.offset,
            textures: terrain.corners.iter().map(|c| c.ground_texture).collect(),
            variations: terrain.corners.iter().map(|c| c.ground_variation).collect(),
            cliff_tiles: (0..h - 1).flat_map(|y| (0..w - 1).map(move |x| (x, y))).map(|(x, y)| is_cliff(x, y)).collect(),
            extended: ground.iter().map(|g| g.1).collect(),
            blight_index,
        };
        let blight: Vec<bool> = terrain.corners.iter().map(|c| c.blight).collect();
        let (gverts, gindices, batches) = build_ground(&ground_src, &heights, &blight);
        let ground_vertices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("ground vertices"),
            contents: bytemuck::cast_slice(&gverts),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let ground_indices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("ground indices"),
            contents: bytemuck::cast_slice(&gindices),
            usage: wgpu::BufferUsages::INDEX,
        });
        let mut indices: Vec<u32> = Vec::new();

        // Cliffs: one model per tile whose corners sit on different cliff levels, named by
        // each corner's level above the lowest (top-left, top-right, bottom-right,
        // bottom-left), as game.dll builds them (FUN_6f744560).
        let cliff_types: Vec<(String, Arc<Texture>)> = terrain
            .cliff_tilesets
            .iter()
            .map(|id| {
                let key = String::from_utf8_lossy(id).into_owned();
                let dir = cliff_slk.get(&key, "cliffModelDir").map(|v| v.as_str()).unwrap_or_else(|| "Cliffs".into());
                let tdir = cliff_slk.get(&key, "texDir").map(|v| v.as_str()).unwrap_or_default();
                let tfile = cliff_slk.get(&key, "texFile").map(|v| v.as_str()).unwrap_or_default();
                // Some entries name the file with its source extension (Cliff0.tga).
                let tfile = tfile.rsplit_once('.').map_or(tfile.as_str(), |(stem, _)| stem).to_string();
                (dir, textures.get_or_white(device, queue, &format!("{tdir}\\{tfile}.blp")))
            })
            .collect();
        let mut cliff_models: std::collections::HashMap<String, Option<w3_formats::mdx::Model>> = Default::default();
        let mut per_cliff: Vec<Vec<u32>> = vec![Vec::new(); cliff_types.len().max(1)];
        for y in 0..h - 1 {
            for x in 0..w - 1 {
                if !is_cliff(x, y) {
                    continue;
                }
                let corners = [(x, y + 1), (x + 1, y + 1), (x + 1, y), (x, y)];
                let layers = corners.map(|(cx, cy)| terrain.corner(cx, cy).layer_height);
                let base = *layers.iter().min().unwrap();
                if layers.iter().any(|&l| l - base > 2) {
                    continue;
                }
                let letters: String = layers.iter().map(|&l| (b'A' + l - base) as char).collect();
                let bl = terrain.corner(x, y);
                let mut ct = bl.cliff_texture as usize;
                if ct == 15 {
                    ct = 0;
                }
                let Some((dir, _)) = cliff_types.get(ct) else { continue };
                let mut model = None;
                for var in [bl.cliff_variation as u32, 0] {
                    let path = format!("Doodads\\Terrain\\{dir}\\{dir}{letters}{var}.mdx");
                    let m = cliff_models.entry(path.clone()).or_insert_with(|| load_mdx(&path));
                    if m.is_some() {
                        model = Some(path);
                        break;
                    }
                }
                let Some(path) = model else { continue };
                let m = cliff_models[&path].as_ref().unwrap();
                let ox = terrain.offset[0] + x as f32 * TILE;
                let oy = terrain.offset[1] + y as f32 * TILE;
                let oz = (base as f32 - 2.0) * TILE;
                for g in &m.geosets {
                    let first = verts.len() as u32;
                    for (vi, p) in g.vertices.iter().enumerate() {
                        // Cliff models are turned a quarter: world x is model y, world y is
                        // minus model x, from the corner the letters start at.
                        let (wx, wy) = (ox + p[1], oy - p[0]);
                        let gz = ground_at((wx - terrain.offset[0]) / TILE, (wy - terrain.offset[1]) / TILE);
                        let n = g.normals.get(vi).copied().unwrap_or([0.0, 0.0, 1.0]);
                        verts.push(Vertex {
                            pos: [wx, wy, oz + p[2] + gz],
                            normal: [n[1], -n[0], n[2]],
                            uv: g.uvs.first().and_then(|u| u.get(vi)).copied().unwrap_or([0.0; 2]),
                            color: [1.0; 4],
                        });
                    }
                    per_cliff[ct].extend(g.faces.chunks_exact(3).flat_map(|t| [first + t[0] as u32, first + t[1] as u32, first + t[2] as u32]));
                }
            }
        }
        // Ramps (FUN_6f730e50): a tile whose corners match a ramp pattern takes the
        // cliff type's ramp model named by its letters with R as L, placed at the
        // pattern's offset from the tile.
        for y in 0..h - 1 {
            for x in 0..w - 1 {
                let l = [letter(x, y + 1), letter(x + 1, y + 1), letter(x + 1, y), letter(x, y)];
                let weight: u32 = l.iter().map(|&c| match c { b'R' => 1, b'S' => 2, _ => 0 }).sum();
                if weight != 1 {
                    continue;
                }
                let Some(&(_, dx, dy)) = w3_formats::map::RAMP_PATTERNS.iter().find(|p| p.0 == &l) else { continue };
                let mut ct = terrain.corner(x, y).cliff_texture as usize;
                if ct == 15 {
                    ct = 0;
                }
                let Some((_, _)) = cliff_types.get(ct) else { continue };
                let key = String::from_utf8_lossy(&terrain.cliff_tilesets[ct]).into_owned();
                let dir = cliff_slk.get(&key, "rampModelDir").map(|v| v.as_str()).unwrap_or_else(|| "CliffTrans".into());
                let name: String = l.iter().map(|&c| if c == b'R' { 'L' } else { c as char }).collect();
                let path = format!("Doodads\\Terrain\\{dir}\\{dir}{name}0.mdx");
                let m = cliff_models.entry(path.clone()).or_insert_with(|| load_mdx(&path));
                let Some(m) = m.as_ref() else { continue };
                let base = [(x, y), (x + 1, y), (x, y + 1), (x + 1, y + 1)].iter().map(|&(cx, cy)| terrain.corner(cx, cy).layer_height).min().unwrap_or(0);
                let ox = terrain.offset[0] + (x as i32 + dx) as f32 * TILE;
                let oy = terrain.offset[1] + (y as i32 + dy) as f32 * TILE;
                let oz = (base as f32 - 2.0) * TILE;
                for g in &m.geosets {
                    let first = verts.len() as u32;
                    for (vi, p) in g.vertices.iter().enumerate() {
                        // Cliff models are turned a quarter: world x is model y, world y is
                        // minus model x, from the corner the letters start at.
                        let (wx, wy) = (ox + p[1], oy - p[0]);
                        let gz = ground_at((wx - terrain.offset[0]) / TILE, (wy - terrain.offset[1]) / TILE);
                        let n = g.normals.get(vi).copied().unwrap_or([0.0, 0.0, 1.0]);
                        verts.push(Vertex {
                            pos: [wx, wy, oz + p[2] + gz],
                            normal: [n[1], -n[0], n[2]],
                            uv: g.uvs.first().and_then(|u| u.get(vi)).copied().unwrap_or([0.0; 2]),
                            color: [1.0; 4],
                        });
                    }
                    per_cliff[ct].extend(g.faces.chunks_exact(3).flat_map(|t| [first + t[0] as u32, first + t[1] as u32, first + t[2] as u32]));
                }
            }
        }
        let mut cliff_batches = Vec::new();
        for (t, list) in per_cliff.iter().enumerate() {
            if !list.is_empty() {
                cliff_batches.push(Batch { texture: t, first: indices.len() as u32, count: list.len() as u32, blended: false });
                indices.extend(list);
            }
        }

        // Water: one quad per tile that has any water corner.
        let tileset = terrain.tileset as char;
        let water_id = format!("{tileset}Sha");
        let water_offset = water_slk
            .get(&water_id, "height")
            .and_then(|v| match v {
                w3_formats::slk::Value::Num(n) => Some(*n as f32),
                _ => None,
            })
            .unwrap_or(0.0)
            * TILE;
        let num = |col: &str| match water_slk.get(&water_id, col) {
            Some(w3_formats::slk::Value::Num(n)) => *n as f32,
            _ => 0.0,
        };
        let shallow_min = [num("Smin_R"), num("Smin_G"), num("Smin_B"), num("Smin_A")].map(|v| v / 255.0);
        let shallow_max = [num("Smax_R"), num("Smax_G"), num("Smax_B"), num("Smax_A")].map(|v| v / 255.0);
        let deep_min = [num("Dmin_R"), num("Dmin_G"), num("Dmin_B"), num("Dmin_A")].map(|v| v / 255.0);
        let deep_max = [num("Dmax_R"), num("Dmax_G"), num("Dmax_B"), num("Dmax_A")].map(|v| v / 255.0);
        let min_depth = misc.get_str("Water", "MinDepth").and_then(|v| v.parse().ok()).unwrap_or(10.0);
        let deep_level = misc.get_str("Water", "DeepLevel").and_then(|v| v.parse().ok()).unwrap_or(64.0);
        let max_depth = misc.get_str("Water", "MaxDepth").and_then(|v| v.parse().ok()).unwrap_or(72.0);
        let water_color = |depth: f32| -> [f32; 4] {
            let lerp = |a: [f32; 4], b: [f32; 4], t: f32| [0, 1, 2, 3].map(|i| a[i] + (b[i] - a[i]) * t);
            if depth <= deep_level {
                let t = ((depth - min_depth) / (deep_level - min_depth)).clamp(0.0, 1.0);
                lerp(shallow_min, shallow_max, t)
            } else {
                let t = ((depth - deep_level) / (max_depth - deep_level)).clamp(0.0, 1.0);
                lerp(deep_min, deep_max, t)
            }
        };
        let water_first = indices.len() as u32;
        for y in 0..h - 1 {
            for x in 0..w - 1 {
                let corners = [(x, y + 1), (x + 1, y + 1), (x, y), (x + 1, y)];
                if !corners.iter().any(|&(cx, cy)| terrain.corner(cx, cy).water) {
                    continue;
                }
                let base = verts.len() as u32;
                let uvs = [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]];
                for (k, &(cx, cy)) in corners.iter().enumerate() {
                    let c = terrain.corner(cx, cy);
                    let wz = c.water_height() + water_offset;
                    let depth = wz - hat(cx, cy);
                    let mut color = water_color(depth);
                    if !c.water {
                        color[3] = 0.0;
                    }
                    verts.push(Vertex {
                        pos: [terrain.offset[0] + cx as f32 * TILE, terrain.offset[1] + cy as f32 * TILE, wz],
                        normal: [0.0, 0.0, 1.0],
                        uv: uvs[k],
                        color,
                    });
                }
                indices.extend([base + 2, base + 3, base + 1, base + 2, base + 1, base]);
            }
        }
        let water_range = (water_first, indices.len() as u32 - water_first);
        if verts.is_empty() {
            verts.push(Vertex::zeroed());
        }
        if indices.is_empty() {
            indices.push(0);
        }

        let vertices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("terrain vertices"),
            contents: bytemuck::cast_slice(&verts),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("terrain indices"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        let shader = device.create_shader_module(wgpu::include_wgsl!("terrain.wgsl"));
        let uniform = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let [fow0, fow1] = crate::fog::layout_entries(2);
        let fog = std::sync::Arc::new(crate::fog::FogTexture::new(device, queue, 1, 1));
        let globals_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("terrain globals"),
            entries: &[uniform(0), uniform(1), fow0, fow1],
        });
        let tex_layout = texture_bind_group_layout(device);
        // war3map.shd: one byte per 32x32 cell, four cells per tile edge, 0 or 255.
        let (sw, sh) = ((w - 1) * 4, (h - 1) * 4);
        let shadow_bytes: Vec<u8> = match shadows {
            Some(b) if b.len() >= (sw * sh) as usize => b[..(sw * sh) as usize].to_vec(),
            _ => vec![0; (sw * sh) as usize],
        };
        let shadow_tex = crate::texture::upload_r8(device, queue, "shadows", sw, sh, &shadow_bytes);
        let shadow_sampler = make_sampler(device, false, false);
        let shadow_bg = texture_bind_group(device, &tex_layout, &shadow_tex, &shadow_sampler);
        let shadow_layout = tex_layout.clone();
        let extent_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("terrain extent"),
            contents: bytemuck::cast_slice(&[terrain.offset[0], terrain.offset[1], (w - 1) as f32 * TILE, (h - 1) as f32 * TILE]),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let globals_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("terrain globals"),
            layout: &globals_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: globals_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: extent_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&fog.view) },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::Sampler(&fog.sampler) },
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("terrain"),
            bind_group_layouts: &[Some(&globals_layout), Some(&tex_layout), Some(&tex_layout)],
            immediate_size: 0,
        });
        let attrs = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2, 3 => Float32x4];
        let make = |entry: &str, blend: Option<wgpu::BlendState>, depth_write: bool, cull: Option<wgpu::Face>| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("terrain"),
                layout: Some(&layout),
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
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: color_format,
                        blend,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState { cull_mode: cull, ..Default::default() },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: crate::gpu::DEPTH_FORMAT,
                    depth_write_enabled: Some(depth_write),
                    depth_compare: Some(wgpu::CompareFunction::LessEqual),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        let opaque = make("fs_ground", None, true, Some(wgpu::Face::Back));
        // Cliff models wind their triangles the other way from the ground mesh.
        let cliffs = make("fs_ground", None, true, None);
        let blended = make("fs_ground", Some(wgpu::BlendState::ALPHA_BLENDING), false, Some(wgpu::Face::Back));
        let water_pipeline = make("fs_water", Some(wgpu::BlendState::ALPHA_BLENDING), false, None);

        let clamp = make_sampler(device, false, false);
        let wrap = make_sampler(device, true, true);
        let texture_bgs = ground.iter().map(|(t, _)| texture_bind_group(device, &tex_layout, &t.view, &clamp)).collect();
        let cliff_bgs = cliff_types.iter().map(|(_, t)| texture_bind_group(device, &tex_layout, &t.view, &wrap)).collect();
        let water_tex = water_slk
            .get(&water_id, "texFile")
            .map(|v| v.as_str())
            .unwrap_or_else(|| "ReplaceableTextures\\Water\\Water".into());
        let water_count = num("numTex").max(1.0) as usize;
        let water_textures = (0..water_count)
            .map(|i| {
                let t = textures.get_or_white(device, queue, &format!("{water_tex}{i:02}.blp"));
                texture_bind_group(device, &tex_layout, &t.view, &wrap)
            })
            .collect();
        TerrainRenderer {
            opaque,
            cliffs,
            blended,
            water_pipeline,
            globals_bg,
            globals_layout,
            globals_buf: globals_buf.clone(),
            extent_buf,
            vertices,
            indices: index_buf,
            ground_vertices,
            ground_indices,
            ground: ground_src,
            batches,
            texture_bgs,
            cliff_batches,
            cliff_bgs,
            shadow_bg,
            shadow_layout,
            shadow_sampler,
            shadow_base: shadow_bytes,
            water_textures,
            water_range,
            water_rate: num("texRate").max(1.0),
            width: w,
            height: h,
            offset: terrain.offset,
            heights,
        }
    }

    /// Rebuilds the ground with a new set of blighted corners.
    pub fn set_blight(&mut self, device: &wgpu::Device, blight: &[bool]) {
        use wgpu::util::DeviceExt;
        let (verts, indices, batches) = build_ground(&self.ground, &self.heights, blight);
        self.ground_vertices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("ground vertices"),
            contents: bytemuck::cast_slice(&verts),
            usage: wgpu::BufferUsages::VERTEX,
        });
        self.ground_indices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("ground indices"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        self.batches = batches;
    }

    /// Uses a fog-of-war texture (see `Globals::fow_extent`).
    pub fn set_fog(&mut self, device: &wgpu::Device, fog: &crate::fog::FogTexture) {
        self.globals_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("terrain globals"),
            layout: &self.globals_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: self.globals_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: self.extent_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&fog.view) },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::Sampler(&fog.sampler) },
            ],
        });
    }

    /// Ground height at a world position, interpolated across the tile.
    /// The shadow cells' size: four per tile edge.
    pub fn shadow_size(&self) -> (u32, u32) {
        ((self.width - 1) * 4, (self.height - 1) * 4)
    }

    /// Replaces the shadow mask (the baked shadows with shadow images stamped in).
    pub fn set_shadows(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, mask: &[u8]) {
        let (sw, sh) = self.shadow_size();
        if mask.len() < (sw * sh) as usize {
            return;
        }
        let tex = crate::texture::upload_r8(device, queue, "shadows", sw, sh, &mask[..(sw * sh) as usize]);
        self.shadow_bg = texture_bind_group(device, &self.shadow_layout, &tex, &self.shadow_sampler);
    }

    pub fn height_at(&self, x: f32, y: f32) -> f32 {
        let fx = ((x - self.offset[0]) / TILE).clamp(0.0, (self.width - 1) as f32);
        let fy = ((y - self.offset[1]) / TILE).clamp(0.0, (self.height - 1) as f32);
        let (ix, iy) = (fx.floor() as u32, fy.floor() as u32);
        let (tx, ty) = (fx - ix as f32, fy - iy as f32);
        let at = |x: u32, y: u32| self.heights[(y.min(self.height - 1) * self.width + x.min(self.width - 1)) as usize];
        let a = at(ix, iy) * (1.0 - tx) + at(ix + 1, iy) * tx;
        let b = at(ix, iy + 1) * (1.0 - tx) + at(ix + 1, iy + 1) * tx;
        a * (1.0 - ty) + b * ty
    }

    pub fn render_ground(&self, pass: &mut wgpu::RenderPass) {
        pass.set_bind_group(0, &self.globals_bg, &[]);
        pass.set_bind_group(2, &self.shadow_bg, &[]);
        pass.set_vertex_buffer(0, self.ground_vertices.slice(..));
        pass.set_index_buffer(self.ground_indices.slice(..), wgpu::IndexFormat::Uint32);
        for b in &self.batches {
            pass.set_pipeline(if b.blended { &self.blended } else { &self.opaque });
            pass.set_bind_group(1, &self.texture_bgs[b.texture], &[]);
            pass.draw_indexed(b.first..b.first + b.count, 0, 0..1);
        }
        pass.set_vertex_buffer(0, self.vertices.slice(..));
        pass.set_index_buffer(self.indices.slice(..), wgpu::IndexFormat::Uint32);
        pass.set_pipeline(&self.cliffs);
        for b in &self.cliff_batches {
            pass.set_bind_group(1, &self.cliff_bgs[b.texture], &[]);
            pass.draw_indexed(b.first..b.first + b.count, 0, 0..1);
        }
    }

    /// `seconds` drives the water texture animation.
    pub fn render_water(&self, pass: &mut wgpu::RenderPass, seconds: f32) {
        if self.water_range.1 == 0 || self.water_textures.is_empty() {
            return;
        }
        pass.set_bind_group(0, &self.globals_bg, &[]);
        pass.set_bind_group(2, &self.shadow_bg, &[]);
        pass.set_vertex_buffer(0, self.vertices.slice(..));
        pass.set_index_buffer(self.indices.slice(..), wgpu::IndexFormat::Uint32);
        pass.set_pipeline(&self.water_pipeline);
        let frame = (seconds * self.water_rate) as usize % self.water_textures.len();
        pass.set_bind_group(1, &self.water_textures[frame], &[]);
        let (first, count) = self.water_range;
        pass.draw_indexed(first..first + count, 0, 0..1);
    }
}
