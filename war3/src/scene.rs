//! A loaded map: terrain, doodads, and destructables, ready to draw.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Context;
use glam::{Mat4, Quat, Vec3};
use w3_formats::map::{Doodads, MapInfo, Terrain, fourcc_str};
use w3_formats::slk::Table;
use w3_render::camera::Camera;
use w3_render::model::{ModelGpu, ModelInstance, ModelRenderer, default_globals};
use w3_render::terrain::TerrainRenderer;
use w3_render::texture::TextureCache;

use crate::data::{DataSet, GameData, PRIORITY_MAP};

pub struct Assets {
    pub textures: TextureCache,
    models: HashMap<String, Option<Arc<ModelGpu>>>,
}

impl Assets {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, data: &GameData) -> Self {
        Assets { textures: TextureCache::new(device, queue, data.chain.clone()), models: HashMap::new() }
    }

    /// Loads a model by path.  Model references often name `.mdl` files, which ship
    /// only as `.mdx`.
    pub fn model(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, path: &str) -> Option<Arc<ModelGpu>> {
        let key = path.to_ascii_lowercase();
        if let Some(m) = self.models.get(&key) {
            return m.clone();
        }
        let mut names = vec![path.to_string()];
        let lower = path.to_ascii_lowercase();
        if lower.ends_with(".mdl") {
            names.insert(0, format!("{}.mdx", &path[..path.len() - 4]));
        } else if !lower.ends_with(".mdx") {
            names.insert(0, format!("{path}.mdx"));
        }
        let m = names.iter().find_map(|n| {
            let bytes = self.textures.chain.read(n).ok()?;
            match w3_formats::mdx::Model::parse(&bytes) {
                Ok(mdx) => Some(Arc::new(ModelGpu::new(device, queue, &mut self.textures, n, mdx))),
                Err(e) => {
                    log::warn!("{n}: {e}");
                    None
                }
            }
        });
        if m.is_none() {
            log::debug!("missing model {path}");
        }
        self.models.insert(key, m.clone());
        m
    }
}

pub struct Scene {
    pub game: Option<w3_game::Game>,
    /// Render instances for game units, by handle.
    pub units: HashMap<u32, ModelInstance>,
    pub info: MapInfo,
    pub terrain: TerrainRenderer,
    pub models: ModelRenderer,
    pub instances: Vec<ModelInstance>,
    pub camera: Camera,
    pub time: f32,
    /// Direction from the light toward the world (UI\MiscData.txt [Light] Direction).
    pub light: Vec3,
}

fn num(t: &Table, row: &str, col: &str) -> f64 {
    match t.get(row, col) {
        Some(w3_formats::slk::Value::Num(n)) => *n,
        Some(w3_formats::slk::Value::Str(s)) => s.parse().unwrap_or(0.0),
        None => 0.0,
    }
}

fn text(t: &Table, row: &str, col: &str) -> String {
    t.get(row, col).map(|v| v.as_str()).unwrap_or_default()
}

impl Scene {
    pub fn load(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        color_format: wgpu::TextureFormat,
        data: &GameData,
        assets: &mut Assets,
        map_path: &std::path::Path,
    ) -> anyhow::Result<Scene> {
        let archive = Arc::new(
            w3_mpq::Archive::open(map_path).with_context(|| format!("opening map {}", map_path.display()))?,
        );
        data.chain.unmount(PRIORITY_MAP);
        data.chain.mount(archive, None, PRIORITY_MAP);
        let chain = &data.chain;
        let info = MapInfo::parse(&chain.read("war3map.w3i")?).context("war3map.w3i")?;
        // w3i flag 0x4 marks a melee map; others use the custom rule set.
        data.select_data_set(if info.flags & 0x4 != 0 { DataSet::Melee } else { DataSet::Custom });
        let terrain = Terrain::parse(&chain.read("war3map.w3e")?).context("war3map.w3e")?;

        let models = ModelRenderer::new(device, queue, &mut assets.textures, color_format);
        let terrain_slk = data.slk("TerrainArt\\Terrain.slk");
        let water_slk = data.slk("TerrainArt\\Water.slk");
        let cliff_slk = data.slk("TerrainArt\\CliffTypes.slk");
        let chain2 = data.chain.clone();
        let mut load_mdx = |path: &str| chain2.read(path).ok().and_then(|b| w3_formats::mdx::Model::parse(&b).ok());
        let terrain_r = TerrainRenderer::new(
            device,
            queue,
            &mut assets.textures,
            color_format,
            &models.globals_buf,
            &terrain,
            &terrain_slk,
            &water_slk,
            &cliff_slk,
            &data.misc,
            &mut load_mdx,
        );

        let mut instances = Vec::new();
        if let Ok(bytes) = chain.read("war3map.doo") {
            let doodads = Doodads::parse(&bytes).context("war3map.doo")?;
            let doodad_slk = data.slk("Doodads\\Doodads.slk");
            let dest_slk = data.slk("Units\\DestructableData.slk");
            for d in &doodads.doodads {
                let id = fourcc_str(d.id);
                let (table, is_dest) = if dest_slk.rows.contains_key(&id) {
                    (&dest_slk, true)
                } else {
                    (&doodad_slk, false)
                };
                let file = text(table, &id, "file");
                if file.is_empty() {
                    continue;
                }
                let nvar = num(table, &id, "numVar") as u32;
                let model = if nvar > 1 || is_dest {
                    assets
                        .model(device, queue, &format!("{file}{}", d.variation))
                        .or_else(|| assets.model(device, queue, &file))
                } else {
                    assets.model(device, queue, &file)
                };
                let Some(model) = model else { continue };
                let mut inst = ModelInstance::new(model);
                inst.transform = Mat4::from_scale_rotation_translation(
                    Vec3::from_array(d.scale),
                    Quat::from_rotation_z(d.angle),
                    Vec3::from_array(d.position),
                );
                if is_dest {
                    let tex_id = num(table, &id, "texID") as u32;
                    let tex_file = text(table, &id, "texFile");
                    if tex_id != 0 && !tex_file.is_empty() && tex_file != "_" {
                        let t = assets.textures.get_or_white(device, queue, &format!("{tex_file}.blp"));
                        inst.replaceable.insert(tex_id, t);
                    }
                }
                inst.play("Stand");
                instances.push(inst);
            }
        }

        // Run the map script: config, then main with the local player in the first
        // user slot.  Other slots stay empty until computer players are implemented.
        let game = match w3_game::Game::load(data.chain.clone(), data.version as i32) {
            Ok(mut g) => {
                if let Err(e) = g.config() {
                    log::error!("config: {e:#}");
                }
                let mut first = true;
                let res = g.start(|_, controller| {
                    if controller == w3_game::types::map_control::USER && first {
                        first = false;
                        w3_game::SlotFill::Local
                    } else {
                        w3_game::SlotFill::Empty
                    }
                });
                if let Err(e) = res {
                    log::error!("main: {e:#}");
                }
                Some(g)
            }
            Err(e) => {
                log::error!("map script: {e:#}");
                None
            }
        };

        let mut camera = Camera::default();
        let start = info
            .players
            .first()
            .map(|p| p.start)
            .unwrap_or([terrain.offset[0] + terrain.width as f32 * 64.0, terrain.offset[1] + terrain.height as f32 * 64.0]);
        camera.target = Vec3::new(start[0], start[1], terrain_r.height_at(start[0], start[1]));
        let dir: Vec<f32> = data.misc.get_list("Light", "Direction").iter().filter_map(|v| v.trim().parse().ok()).collect();
        let light = if dir.len() == 3 { Vec3::new(dir[0], dir[1], dir[2]) } else { Vec3::new(0.3, 0.3, -0.25) };
        let mut scene = Scene { game, units: HashMap::new(), info, terrain: terrain_r, models, instances, camera, time: 0.0, light };
        scene.apply_requests();
        Ok(scene)
    }

    /// Handles presentation requests from the game.
    fn apply_requests(&mut self) {
        let Some(g) = self.game.as_mut() else { return };
        let local = 1u32 << g.world.local_player;
        for r in std::mem::take(&mut g.world.requests) {
            match r {
                w3_game::world::Request::SetCameraPosition { players, x, y } if players & local != 0 => {
                    self.camera.target.x = x;
                    self.camera.target.y = y;
                }
                w3_game::world::Request::PanCamera { players, x, y, .. } if players & local != 0 => {
                    self.camera.target.x = x;
                    self.camera.target.y = y;
                }
                w3_game::world::Request::DisplayText { text, .. } => log::info!("[text] {text}"),
                _ => {}
            }
        }
    }

    /// Creates, updates, and removes render instances to match the game's units.
    pub fn sync_units(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, assets: &mut Assets) {
        let Some(g) = self.game.as_ref() else { return };
        let w = &g.world;
        self.units.retain(|h, _| w.unit(*h).is_some());
        for &h in &w.unit_order {
            let Some(u) = w.unit(h) else { continue };
            if !self.units.contains_key(&h) {
                let Some(o) = w.objects.units.get(u.type_id) else { continue };
                let file = o.str("file");
                let Some(model) = assets.model(device, queue, &file) else { continue };
                let mut inst = ModelInstance::new(model);
                inst.play("Stand");
                self.units.insert(h, inst);
            }
            let inst = self.units.get_mut(&h).unwrap();
            let z = self.terrain.height_at(u.x, u.y) + u.fly_height;
            inst.transform = Mat4::from_scale_rotation_translation(
                Vec3::splat(u.scale),
                Quat::from_rotation_z(u.facing.to_radians()),
                Vec3::new(u.x, u.y, z),
            );
            inst.team = w.players.get(u.owner as usize).map_or(0, |p| p.color.clamp(0, 15) as u8);
            inst.team = u.color.clamp(0, 15) as u8;
            inst.visible = !u.hidden;
        }
    }

    pub fn update(&mut self, dt: f32) {
        self.time += dt;
        if let Some(g) = self.game.as_mut() {
            g.tick(dt);
        }
        self.apply_requests();
        for inst in &mut self.instances {
            inst.advance(dt * 1000.0);
        }
        for inst in self.units.values_mut() {
            inst.advance(dt * 1000.0);
        }
        self.camera.target.z = self.terrain.height_at(self.camera.target.x, self.camera.target.y);
    }

    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        color: &wgpu::TextureView,
        depth: &wgpu::TextureView,
        aspect: f32,
    ) {
        let view_proj = self.camera.view_proj(aspect);
        let eye = self.camera.eye();
        let rot = self.camera.billboard_rotation();
        let mut globals = default_globals(view_proj, eye);
        let light = self.light;
        globals.light_dir = [light.x, light.y, light.z, 0.0];
        for inst in &mut self.instances {
            // Cull instances well outside the view.
            let p = inst.transform.w_axis.truncate();
            if p.distance(self.camera.target) > self.camera.far * 1.5 {
                continue;
            }
            self.models.submit(inst, eye, rot);
        }
        for inst in self.units.values_mut() {
            self.models.submit(inst, eye, rot);
        }
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("scene"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: color,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth,
                depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                stencil_ops: None,
            }),
            ..Default::default()
        });
        queue.write_buffer(&self.models.globals_buf, 0, bytemuck::bytes_of(&globals));
        self.terrain.render_ground(&mut pass);
        self.models.render(device, queue, &mut pass, &globals);
        self.terrain.render_water(&mut pass, self.time);
    }
}
