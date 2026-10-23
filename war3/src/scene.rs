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
    pub frames: w3_ui::FrameDb,
    models: HashMap<String, Option<Arc<ModelGpu>>>,
}

impl Assets {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, data: &GameData) -> Self {
        let mut frames = w3_ui::FrameDb::load(&data.chain);
        frames.version = if data.version == crate::data::Version::FrozenThrone { 1 } else { 0 };
        Assets { textures: TextureCache::new(device, queue, data.chain.clone()), frames, models: HashMap::new() }
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

fn team_colors(chain: &w3_mpq::Chain) -> [[f32; 3]; 16] {
    let mut out = [[1.0; 3]; 16];
    for (i, c) in out.iter_mut().enumerate() {
        let path = format!("ReplaceableTextures\\TeamColor\\TeamColor{i:02}.blp");
        if let Some(img) = chain.read(&path).ok().and_then(|b| w3_formats::blp::decode_texture(&path, &b).ok()).and_then(|v| v.into_iter().next()) {
            *c = [img.rgba[0] as f32 / 255.0, img.rgba[1] as f32 / 255.0, img.rgba[2] as f32 / 255.0];
        }
    }
    out
}

pub struct UnitView {
    pub inst: ModelInstance,
    serial: u32,
    requested: String,
    anim: String,
    rng: u32,
}

impl UnitView {
    fn pick(&mut self, name: &str) {
        let seqs = w3_render::anim::sequences_named(&self.inst.model.mdx, name);
        if seqs.is_empty() {
            // Drop qualifiers ("walk gold" -> "walk") before falling back to stand.
            match name.rsplit_once(' ') {
                Some((base, _)) => self.pick(base),
                None if name != "stand" => self.pick("stand"),
                None => {}
            }
            return;
        }
        self.rng = self.rng.wrapping_mul(1_103_515_245).wrapping_add(12345);
        let s = seqs[(self.rng >> 16) as usize % seqs.len()];
        self.inst.set_sequence(Some(s));
    }

    /// Follows the game's animation requests; returns to stand variants when a one-shot
    /// animation ends, and lets corpses decay.
    fn update(&mut self, anim: &str, serial: u32) {
        if serial != self.serial || anim != self.requested {
            self.serial = serial;
            self.requested = anim.to_string();
            self.anim = anim.to_string();
            self.pick(anim);
            return;
        }
        if self.inst.finished {
            let next = match self.anim.as_str() {
                "death" => Some("decay flesh"),
                "decay flesh" => Some("decay bone"),
                "decay bone" => None,
                _ => Some("stand"),
            };
            if let Some(n) = next {
                self.anim = n.to_string();
                self.pick(n);
            }
        } else if self.inst.looped && self.anim == "stand" {
            self.inst.looped = false;
            self.pick("stand");
        }
    }
}

pub struct Scene {
    pub game: Option<w3_game::Game>,
    /// Render instances for game units, by handle.
    pub units: HashMap<u32, UnitView>,
    pub dests: HashMap<u32, (ModelInstance, bool)>,
    pub selection: Vec<u32>,
    pub info: MapInfo,
    pub terrain: TerrainRenderer,
    pub models: ModelRenderer,
    pub instances: Vec<ModelInstance>,
    pub camera: Camera,
    pub time: f32,
    /// Direction from the light toward the world (UI\MiscData.txt [Light] Direction).
    pub light: Vec3,
    /// Terrain and unit day/night models.
    pub dnc: (Option<w3_formats::mdx::Model>, Option<w3_formats::mdx::Model>),
    dnc_paths: (String, String),
    pub hud: Option<crate::hud::Hud>,
    ui: w3_render::ui::UiRenderer,
    decals: w3_render::decal::DecalRenderer,
    pub drag: Option<((f32, f32), (f32, f32))>,
    /// A targeted order waiting for a world click.
    pub targeting: Option<&'static str>,
    /// A structure waiting for a placement click.
    pub placing: Option<w3_game::objects::Id>,
    commands: crate::card::Commands,
    /// Player colors, read from the solid ReplaceableTextures\TeamColor images.
    team_rgb: [[f32; 3]; 16],
    portrait_renderer: ModelRenderer,
    portrait: Option<(u32, UnitView)>,
    fog: Option<Arc<w3_render::fog::FogTexture>>,
    /// Sound output; absent when running without audio.
    pub audio: Option<w3_audio::Audio>,
    /// Sound and music requests waiting for [`Scene::update_audio`].
    pending_audio: Vec<w3_game::world::Request>,
    /// Script sound handles to playing voices.
    voices: HashMap<u32, u64>,
    /// Acknowledgments: the unit last clicked, the consecutive "What" count (game.dll
    /// DAT_6fd707f0), and the playing line.
    ack_unit: u32,
    ack_clicks: usize,
    ack_voice: Option<u64>,
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
        aspect: f32,
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
            chain.read("war3map.shd").ok().as_deref(),
        );

        let mut instances = Vec::new();
        if let Ok(bytes) = chain.read("war3map.doo") {
            let doodads = Doodads::parse(&bytes).context("war3map.doo")?;
            let doodad_slk = data.slk("Doodads\\Doodads.slk");
            let dest_slk = data.slk("Units\\DestructableData.slk");
            for d in &doodads.doodads {
                let id = fourcc_str(d.id);
                // Destructables are game objects; the scene draws them from the world.
                if dest_slk.rows.contains_key(&id) {
                    continue;
                }
                let (table, is_dest) = (&doodad_slk, false);
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
                        w3_game::SlotFill::Computer
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
        let hud = game.as_ref().map(|g| {
            let race = g.world.players[g.world.local_player as usize].race;
            crate::hud::Hud::new(&mut assets.frames, aspect, race)
        });
        let mut hud = hud;
        if let Some(h) = hud.as_mut() {
            let c = info.camera_complements;
            let (ox, oy) = (terrain.offset[0], terrain.offset[1]);
            let (w, hgt) = ((terrain.width - 1) as f32 * 128.0, (terrain.height - 1) as f32 * 128.0);
            h.minimap_area = Some(crate::hud::MinimapArea {
                x0: ox + c[0] as f32 * 128.0,
                x1: ox + w - c[1] as f32 * 128.0,
                y0: oy + c[2] as f32 * 128.0,
                y1: oy + hgt - c[3] as f32 * 128.0,
            });
        }
        let ui = w3_render::ui::UiRenderer::new(device, color_format);
        let portrait_renderer = ModelRenderer::new(device, queue, &mut assets.textures, color_format);
        let decals = w3_render::decal::DecalRenderer::new(device, color_format, &models.globals_buf);
        let mut scene = Scene { game, units: HashMap::new(), dests: HashMap::new(), selection: Vec::new(), info, terrain: terrain_r, models, instances, camera, time: 0.0, light, dnc: (None, None), dnc_paths: Default::default(), hud, ui, decals, drag: None, targeting: None, placing: None, commands: crate::card::Commands::load(&data.chain), team_rgb: team_colors(&data.chain), portrait_renderer, portrait: None, fog: None, audio: None, pending_audio: Vec::new(), voices: HashMap::new(), ack_unit: 0, ack_clicks: 0, ack_voice: None };
        if let Some(f) = scene.game.as_ref().and_then(|g| g.world.fog.as_ref()) {
            let tex = Arc::new(w3_render::fog::FogTexture::new(device, queue, f.width as u32, f.height as u32));
            scene.terrain.set_fog(device, &tex);
            scene.models.set_fog(device, tex.clone());
            scene.fog = Some(tex);
        }
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
                r @ (w3_game::world::Request::PlaySound(_)
                | w3_game::world::Request::UiSound { .. }
                | w3_game::world::Request::WorldSound { .. }
                | w3_game::world::Request::Ack { .. }
                | w3_game::world::Request::StopSound { .. }
                | w3_game::world::Request::PlayMusic(_)
                | w3_game::world::Request::StopMusic(_)) => self.pending_audio.push(r),
                _ => {}
            }
        }
    }

    /// Plays a unit's acknowledgment line (UnitAckSounds `<unitSound><kind>`), replacing
    /// the one playing.  Returns whether a line started.
    fn ack(&mut self, h: u32, kind: &str, variant: Option<usize>) -> bool {
        let Some(g) = &self.game else { return false };
        let Some(u) = g.world.unit(h) else { return false };
        let Some(sound) = g.world.objects.units.get(u.type_id).map(|o| o.str("unitSound")).filter(|s| !s.is_empty()) else { return false };
        let pos = [u.x, u.y, g.world.ground_z(u.x, u.y)];
        let Some(audio) = self.audio.as_mut() else { return false };
        let label = format!("{sound}{kind}");
        if let Some(v) = self.ack_voice.take() {
            audio.stop(v);
        }
        self.ack_voice = match variant {
            Some(i) => audio.play_label_variant(&label, i, Some(pos)),
            None => audio.play_label(&label, Some(pos)),
        };
        self.ack_voice.is_some()
    }

    /// Selecting a unit: its "What" line, or after three in a row on the same unit, its
    /// "Pissed" lines in order (game.dll FUN_6f697c30 and FUN_6f690dd0).
    fn select_ack(&mut self, h: u32) {
        if h != self.ack_unit {
            self.ack_unit = h;
            self.ack_clicks = 0;
        }
        let playing = self.ack_voice.is_some_and(|v| self.audio.as_ref().is_some_and(|a| a.is_playing(v)));
        if playing {
            return;
        }
        if self.ack_clicks >= 3 {
            if self.ack(h, "Pissed", Some(self.ack_clicks - 3)) {
                self.ack_clicks += 1;
                return;
            }
            self.ack_clicks = 0;
        }
        if self.ack(h, "What", None) {
            self.ack_clicks += 1;
        }
    }

    /// Plays an interface sound label.
    pub fn ui_sound(&mut self, label: &str) {
        if let Some(a) = self.audio.as_mut() {
            a.play_label(label, None);
        }
    }

    /// Reports a command error: its text from Units\CommandStrings.txt [Errors] and the
    /// race's spoken warning.
    pub fn error(&mut self, key: &str) {
        if key.is_empty() {
            return;
        }
        let Some(g) = &self.game else { return };
        let race = g.world.players.get(g.world.local_player as usize).map_or(1, |p| p.race);
        let race_name = match race { 2 => "Orc", 3 => "Undead", 4 => "NightElf", _ => "Human" };
        // Per-race lists run Human, Orc, Undead, Night Elf.
        let texts = self.commands.profile.get_list("Errors", key);
        let i = match race { 2 => 1, 3 => 2, 4 => 3, _ => 0 };
        let text = texts.get(i).or(texts.first()).cloned().unwrap_or_default();
        log::info!("[error] {text}");
        let label = match key {
            "Nogold" => format!("NoGold{race_name}"),
            "Nolumber" => format!("NoLumber{race_name}"),
            "Nofood" => format!("NoFood{race_name}"),
            "Nomana" => format!("NoMana{race_name}"),
            _ => "InterfaceError".to_string(),
        };
        self.ui_sound(&label);
    }

    /// Plays requested sounds and music and moves the listener with the camera.
    pub fn update_audio(&mut self, assets: &Assets) {
        let mut pending = std::mem::take(&mut self.pending_audio);
        // Acknowledgments of the local player's units.
        let local = self.game.as_ref().map_or(0, |g| g.world.local_player);
        let owned = |s: &Self, h: u32| s.game.as_ref().and_then(|g| g.world.unit(h)).is_some_and(|u| u.owner == local);
        let acks: Vec<(u32, &'static str)> = pending
            .iter()
            .filter_map(|r| match r {
                w3_game::world::Request::Ack { unit, kind } => Some((*unit, *kind)),
                _ => None,
            })
            .collect();
        pending.retain(|r| !matches!(r, w3_game::world::Request::Ack { .. }));
        for (h, kind) in acks {
            if owned(self, h) {
                self.ack(h, kind, None);
            }
        }
        let Some(audio) = self.audio.as_mut() else { return };
        let Some(g) = self.game.as_mut() else { return };
        for r in pending {
            match r {
                w3_game::world::Request::PlaySound(h) => {
                    let Some(w3_game::types::Obj::Sound(s)) = g.world.handles.get(h) else { continue };
                    let s = (**s).clone();
                    let pos = if s.is_3d { s.position.map(|p| [p.0, p.1, p.2]) } else { None };
                    let id = match (&s.label, s.path.is_empty()) {
                        // A label supplies the file list and parameters unless a file is named.
                        (Some(l), true) => audio.play_label(l, pos),
                        _ => {
                            let def = s.label.as_deref().and_then(|l| audio.def(l)).cloned();
                            let d = def.map_or((s.min_dist, s.max_dist, s.cutoff), |d| (d.min_distance, d.max_distance, d.cutoff));
                            audio.play_file(&s.path, s.volume.clamp(0, 127) as f32 / 127.0, s.pitch.max(0.01), pos, d, s.looping)
                        }
                    };
                    if let Some(id) = id {
                        self.voices.insert(h, id);
                    }
                }
                w3_game::world::Request::StopSound { sound, .. } => {
                    if let Some(id) = self.voices.remove(&sound) {
                        audio.stop(id);
                    }
                }
                w3_game::world::Request::PlayMusic(list) => {
                    // "Music" and similar names are war3skins.txt keys for the local race.
                    let list = if list.contains('\\') || list.contains('/') { list } else { assets.frames.skin_value(&list).unwrap_or(list) };
                    audio.play_music(&list, true, 0);
                }
                w3_game::world::Request::StopMusic(_) => audio.stop_music(),
                w3_game::world::Request::UiSound { players, label } if players & (1 << local) != 0 => {
                    audio.play_label(&label, None);
                }
                w3_game::world::Request::WorldSound { label, x, y, z } => {
                    if g.world.fog_state(x, y, local) == w3_game::fog::FOG_VISIBLE {
                        audio.play_label(&label, Some([x, y, z]));
                    }
                }
                _ => {}
            }
        }
        // Script sounds report when they finish.
        self.voices.retain(|h, id| {
            let playing = audio.is_playing(*id);
            if !playing {
                if let Some(w3_game::types::Obj::Sound(s)) = g.world.handles.get_mut(*h) {
                    s.playing = false;
                }
            }
            playing
        });
        // Animation sound events of units and doodads the local player can see.
        let mut events: Vec<([u8; 4], [f32; 3])> = Vec::new();
        let mut drain = |inst: &mut ModelInstance| {
            if inst.sound_events.is_empty() {
                return;
            }
            let p = inst.transform.w_axis;
            for e in inst.sound_events.drain(..) {
                if inst.visible {
                    events.push((e, [p.x, p.y, p.z]));
                }
            }
        };
        for v in self.units.values_mut() {
            drain(&mut v.inst);
        }
        for (inst, _) in self.dests.values_mut() {
            drain(inst);
        }
        for inst in &mut self.instances {
            drain(inst);
        }
        for (code, pos) in events {
            let code = String::from_utf8_lossy(&code).to_ascii_uppercase();
            if let Some(label) = audio.anim_lookups.get(&code).cloned() {
                audio.play_label(&label, Some(pos));
            }
        }
        let t = self.camera.target;
        audio.update([t.x, t.y, t.z]);
    }

    fn load_dnc(chain: &w3_mpq::Chain, path: &str) -> Option<w3_formats::mdx::Model> {
        let p = if path.to_ascii_lowercase().ends_with(".mdl") { format!("{}.mdx", &path[..path.len() - 4]) } else { path.to_string() };
        chain.read(&p).ok().and_then(|b| w3_formats::mdx::Model::parse(&b).ok())
    }

    fn selection_decals(&self, device: &wgpu::Device, queue: &wgpu::Queue, assets: &mut Assets) -> Vec<w3_render::decal::Decal> {
        let Some(g) = &self.game else { return vec![] };
        let w = &g.world;
        let misc = &w.misc;
        let num = |k: &str, d: f32| misc.get_str("SelectionCircle", k).and_then(|v| v.trim().parse().ok()).unwrap_or(d);
        let argb = |k: &str| {
            let v: Vec<f32> = misc.get_list("SelectionCircle", k).iter().filter_map(|x| x.trim().parse().ok()).collect();
            if v.len() == 4 { [v[1] / 255.0, v[2] / 255.0, v[3] / 255.0, v[0] / 255.0] } else { [0.0, 1.0, 0.0, 1.0] }
        };
        let mut out = Vec::new();
        for &h in &self.selection {
            let Some(u) = w.unit(h) else { continue };
            let Some(o) = w.objects.units.get(u.type_id) else { continue };
            // UI\MiscData.txt [SelectionCircle]: size = SLK scale x ScaleFactor; the texture
            // is chosen by the first SizeNN at least that large.
            let size = o.real("scale") * num("ScaleFactor", 72.0);
            let sizes = num("NumSizes", 3.0) as usize;
            let mut idx = sizes.saturating_sub(1);
            for i in 0..sizes.saturating_sub(1) {
                if size <= num(&format!("Size{i:02}"), f32::MAX) {
                    idx = i;
                    break;
                }
            }
            let Some(tex) = misc.get_str("SelectionCircle", &format!("Texture{idx:02}")) else { continue };
            let texture = assets.textures.get_or_white(device, queue, &format!("{tex}.blp"));
            let rel = if u.owner == w.local_player {
                "ColorFriend"
            } else if w3_game::natives::players::is_ally(w, w.local_player, u.owner) {
                "ColorFriend"
            } else if u.owner >= w3_game::types::PLAYER_NEUTRAL_AGGRESSIVE {
                "ColorNeutral"
            } else {
                "ColorEnemy"
            };
            out.push(w3_render::decal::Decal {
                texture,
                center: [u.x, u.y, self.terrain.height_at(u.x, u.y)],
                size: [size, size],
                color: argb(rel),
                additive: false,
            });
        }
        out
    }

    /// World point under a screen position, intersecting the terrain.
    pub fn ground_at(&self, sx: f32, sy: f32, size: (u32, u32)) -> Option<(f32, f32)> {
        let aspect = size.0 as f32 / size.1 as f32;
        let (nx, ny) = (sx / size.0 as f32 * 2.0 - 1.0, 1.0 - sy / size.1 as f32 * 2.0);
        let (o, d) = self.camera.ray(aspect, nx, ny);
        let mut t = 0.0;
        let mut prev = o;
        for _ in 0..2000 {
            t += 8.0;
            let p = o + d * t;
            if p.z <= self.terrain.height_at(p.x, p.y) {
                // Refine between the last two samples.
                let mut a = prev;
                let mut b = p;
                for _ in 0..12 {
                    let m = (a + b) * 0.5;
                    if m.z <= self.terrain.height_at(m.x, m.y) { b = m } else { a = m }
                }
                return Some((b.x, b.y));
            }
            prev = p;
        }
        None
    }

    fn project(&self, p: Vec3, size: (u32, u32)) -> Option<(f32, f32)> {
        let aspect = size.0 as f32 / size.1 as f32;
        let c = self.camera.view_proj(aspect) * p.extend(1.0);
        if c.w <= 0.0 {
            return None;
        }
        Some(((c.x / c.w + 1.0) / 2.0 * size.0 as f32, (1.0 - c.y / c.w) / 2.0 * size.1 as f32))
    }

    /// The unit under a screen position, if any.
    pub fn pick_unit(&self, sx: f32, sy: f32, size: (u32, u32)) -> Option<u32> {
        let g = self.game.as_ref()?;
        let w = &g.world;
        let mut best: Option<(f32, u32)> = None;
        for &h in &w.unit_order {
            let Some(u) = w.unit(h) else { continue };
            if u.hidden || (u.dead && !u.is_building) {
                continue;
            }
            let o = w.objects.units.get(u.type_id);
            let radius = o.map_or(32.0, |o| o.real("scale") * w.misc_f32("ScaleFactor", 72.0) * 0.5).max(16.0);
            let z = self.terrain.height_at(u.x, u.y) + u.fly_height;
            let Some(c) = self.project(Vec3::new(u.x, u.y, z + radius * 0.5), size) else { continue };
            let Some(e) = self.project(Vec3::new(u.x + radius, u.y, z + radius * 0.5), size) else { continue };
            let r = (e.0 - c.0).abs().max(8.0);
            let d = (c.0 - sx).hypot(c.1 - sy);
            if d <= r && best.is_none_or(|b| d < b.0) {
                best = Some((d, h));
            }
        }
        best.map(|b| b.1)
    }

    /// The living destructable under a screen position, if any.
    pub fn pick_destructable(&self, sx: f32, sy: f32, size: (u32, u32)) -> Option<u32> {
        let g = self.game.as_ref()?;
        let w = &g.world;
        let mut best: Option<(f32, u32)> = None;
        for &h in &w.destructable_order {
            let Some(w3_game::types::Obj::Destructable(d)) = w.handles.get(h) else { continue };
            if d.life <= 0.0 || !d.visible {
                continue;
            }
            // Test against the model's bounds: its height, and a radius at the canopy.
            let Some((inst, _)) = self.dests.get(&h) else { continue };
            let ext = inst.model.mdx.extent;
            let radius = ((ext.max[0] - ext.min[0]).max(ext.max[1] - ext.min[1]) * 0.35 * d.scale).max(24.0);
            let height = ext.max[2] * d.scale * 0.8;
            let Some(base) = self.project(Vec3::new(d.x, d.y, d.z), size) else { continue };
            let Some(top) = self.project(Vec3::new(d.x, d.y, d.z + height), size) else { continue };
            let Some(e) = self.project(Vec3::new(d.x + radius, d.y, d.z), size) else { continue };
            let r = (e.0 - base.0).abs().max(8.0);
            let (vx, vy) = (top.0 - base.0, top.1 - base.1);
            let len2 = (vx * vx + vy * vy).max(1e-3);
            let t = (((sx - base.0) * vx + (sy - base.1) * vy) / len2).clamp(0.0, 1.0);
            let dd = (base.0 + vx * t - sx).hypot(base.1 + vy * t - sy);
            if dd <= r && best.is_none_or(|b| dd < b.0) {
                best = Some((dd, h));
            }
        }
        best.map(|b| b.1)
    }

    /// Selects with a click or a drag rectangle, the way the local player would.
    pub fn select(&mut self, a: (f32, f32), b: (f32, f32), size: (u32, u32), add: bool) {
        let Some(g) = self.game.as_ref() else { return };
        let local = g.world.local_player;
        let mut picked: Vec<u32> = Vec::new();
        if (a.0 - b.0).abs() < 4.0 && (a.1 - b.1).abs() < 4.0 {
            if let Some(h) = self.pick_unit(b.0, b.1, size) {
                picked.push(h);
            }
        } else {
            let (x0, x1) = (a.0.min(b.0), a.0.max(b.0));
            let (y0, y1) = (a.1.min(b.1), a.1.max(b.1));
            let w = &g.world;
            for &h in &w.unit_order {
                let Some(u) = w.unit(h) else { continue };
                if u.owner != local || u.dead || u.hidden {
                    continue;
                }
                let z = self.terrain.height_at(u.x, u.y) + u.fly_height;
                if let Some((px, py)) = self.project(Vec3::new(u.x, u.y, z), size) {
                    if px >= x0 && px <= x1 && py >= y0 && py <= y1 {
                        picked.push(h);
                    }
                }
            }
            // Box selection prefers units over structures.
            if picked.iter().any(|&h| w.unit(h).is_some_and(|u| !u.is_building)) {
                picked.retain(|&h| w.unit(h).is_some_and(|u| !u.is_building));
            }
            picked.truncate(12);
        }
        if add {
            for h in picked {
                if !self.selection.contains(&h) && self.selection.len() < 12 {
                    self.selection.push(h);
                }
            }
        } else if !picked.is_empty() || (a.0 - b.0).abs() >= 4.0 {
            self.selection = picked;
        } else {
            self.selection.clear();
        }
        if let Some(&h) = self.selection.first() {
            self.select_ack(h);
        }
    }

    /// Where the corners of the world view meet the ground.
    fn camera_footprint(&self) -> Vec<(f32, f32)> {
        let aspect = 16.0 / 9.0;
        let ndc_top = (0.58 / 0.6) * 2.0 - 1.0;
        let ndc_bottom = (0.13 / 0.6) * 2.0 - 1.0;
        [(-1.0, ndc_top), (1.0, ndc_top), (1.0, ndc_bottom), (-1.0, ndc_bottom)]
            .iter()
            .filter_map(|&(x, y)| {
                let (o, d) = self.camera.ray(aspect, x, y);
                (d.z < -1e-3).then(|| {
                    let t = (self.camera.target.z - o.z) / d.z;
                    (o.x + d.x * t, o.y + d.y * t)
                })
            })
            .collect()
    }

    /// Converts a screen position to interface coordinates.
    pub fn to_ui(&self, sx: f32, sy: f32, size: (u32, u32)) -> (f32, f32) {
        let width = self.hud.as_ref().map_or(w3_ui::UI_WIDTH, |h| h.tree.width);
        (sx / size.0 as f32 * width, (1.0 - sy / size.1 as f32) * w3_ui::UI_HEIGHT)
    }

    /// Whether a screen position is over the console rather than the world.
    pub fn over_console(&self, sx: f32, sy: f32, size: (u32, u32)) -> bool {
        let (_, y) = self.to_ui(sx, sy, size);
        self.hud.is_some() && !(0.13..=0.58).contains(&y)
    }

    /// Runs a command card action for the selection.
    pub fn card_action(&mut self, action: crate::card::Action) {
        use crate::card::Action as A;
        let Some(g) = self.game.as_mut() else { return };
        let local = g.world.local_player;
        let mine: Vec<u32> = self.selection.iter().copied().filter(|&h| g.world.unit(h).is_some_and(|u| u.owner == local)).collect();
        let Some(&first) = mine.first() else { return };
        match action {
            A::Order { order, targeted: true } if !order.is_empty() => self.targeting = Some(order),
            A::Order { order, targeted: false } if !order.is_empty() => {
                let id = w3_game::natives::basic::order_id(order);
                for h in mine {
                    g.world.order_unit(h, id, 0.0, 0.0, 0);
                }
                self.order_ack();
            }
            A::Order { .. } => {}
            A::Train(id) => {
                if let Err(e) = g.world.train(first, id) {
                    self.error(e);
                }
            }
            A::BuildMenu => {
                if let Some(h) = self.hud.as_mut() {
                    h.page = crate::card::Page::Build;
                }
            }
            A::LearnMenu => {
                if let Some(h) = self.hud.as_mut() {
                    h.page = crate::card::Page::Learn;
                }
            }
            A::Learn(id) => {
                g.world.learn_skill(first, id);
                if let Some(h) = self.hud.as_mut() {
                    if g.world.unit(first).is_none_or(|u| u.skill_points <= 0) {
                        h.page = crate::card::Page::Main;
                    }
                }
            }
            A::Build(id) => {
                self.placing = Some(id);
                if let Some(h) = self.hud.as_mut() {
                    h.page = crate::card::Page::Main;
                }
            }
            A::Cancel => {
                self.targeting = None;
                self.placing = None;
                if let Some(h) = self.hud.as_mut() {
                    h.page = crate::card::Page::Main;
                }
            }
            A::CancelTrain => g.world.cancel_last(first),
            A::Research(id) => {
                if let Err(e) = g.world.research_at(first, id) {
                    self.error(e);
                }
            }
            A::Upgrade(id) => {
                if let Err(e) = g.world.upgrade_structure(first, id) {
                    self.error(e);
                }
            }
        }
    }

    /// Handles a left click: interface first, then targeting, placement, or selection start.
    /// Returns true when the click was consumed and should not start a selection drag.
    pub fn left_click(&mut self, sx: f32, sy: f32, size: (u32, u32)) -> bool {
        let (ux, uy) = self.to_ui(sx, sy, size);
        if let Some(b) = self.hud.as_ref().and_then(|h| h.button_at(ux, uy)).cloned() {
            if b.enabled {
                self.ui_sound("InterfaceClick");
                self.card_action(b.action);
            }
            return true;
        }
        if let Some((wx, wy)) = self.hud.as_ref().and_then(|h| h.minimap_to_world(ux, uy)) {
            self.camera.target.x = wx;
            self.camera.target.y = wy;
            return true;
        }
        if self.over_console(sx, sy, size) {
            return true;
        }
        if let Some(order) = self.targeting.take() {
            let target = self.pick_unit(sx, sy, size).or_else(|| self.pick_destructable(sx, sy, size));
            let point = self.ground_at(sx, sy, size);
            if let Some(g) = self.game.as_mut() {
                let id = w3_game::natives::basic::order_id(order);
                for &h in &self.selection {
                    if g.world.unit(h).is_none_or(|u| u.owner != g.world.local_player) {
                        continue;
                    }
                    match (target, point) {
                        (Some(t), _) => g.world.order_unit(h, id, 0.0, 0.0, t),
                        (None, Some((x, y))) => g.world.order_unit(h, id, x, y, 0),
                        _ => {}
                    }
                }
            }
            self.order_ack();
            return true;
        }
        if let Some(id) = self.placing.take() {
            let Some((x, y)) = self.ground_at(sx, sy, size) else { return true };
            if let Some(g) = self.game.as_mut() {
                let (x, y) = g.world.snap_placement(x, y);
                if let Some(&worker) = self.selection.first() {
                    if let Some(u) = g.world.unit_mut(worker) {
                        u.action = w3_game::types::Action::Build { id, x, y };
                        u.path.clear();
                    }
                }
            }
            return true;
        }
        false
    }

    /// A key press: command card hotkeys and Escape.
    pub fn key(&mut self, c: char) {
        if c == '\u{1b}' && (self.targeting.is_some() || self.placing.is_some()) {
            self.card_action(crate::card::Action::Cancel);
            return;
        }
        let c = c.to_ascii_uppercase();
        if let Some(b) = self.hud.as_ref().and_then(|h| h.card.iter().find(|b| b.hotkey.map(|k| k.to_ascii_uppercase()) == Some(c)).cloned()) {
            if b.enabled {
                self.card_action(b.action);
            }
        }
    }

    /// Right-click: a smart order to every selected unit the local player owns.
    pub fn command(&mut self, sx: f32, sy: f32, size: (u32, u32)) {
        if self.targeting.take().is_some() || self.placing.take().is_some() {
            return;
        }
        let (ux, uy) = self.to_ui(sx, sy, size);
        if let Some((wx, wy)) = self.hud.as_ref().and_then(|h| h.minimap_to_world(ux, uy)) {
            if let Some(g) = self.game.as_mut() {
                let smart = w3_game::natives::basic::order_id("smart");
                for &h in &self.selection {
                    if g.world.unit(h).is_some_and(|u| u.owner == g.world.local_player) {
                        g.world.order_unit(h, smart, wx, wy, 0);
                    }
                }
            }
            return;
        }
        if self.over_console(sx, sy, size) {
            return;
        }
        let target = self.pick_unit(sx, sy, size).or_else(|| self.pick_destructable(sx, sy, size));
        let point = self.ground_at(sx, sy, size);
        let Some(g) = self.game.as_mut() else { return };
        let local = g.world.local_player;
        let smart = w3_game::natives::basic::order_id("smart");
        for &h in &self.selection {
            if g.world.unit(h).is_none_or(|u| u.owner != local) {
                continue;
            }
            match (target, point) {
                (Some(t), _) if t != h => g.world.order_unit(h, smart, 0.0, 0.0, t),
                (_, Some((x, y))) => g.world.order_unit(h, smart, x, y, 0),
                _ => {}
            }
        }
        self.order_ack();
    }

    /// The first selected unit acknowledges an order: "YesAttack" when it is now
    /// attacking, else "Yes".
    fn order_ack(&mut self) {
        let Some(g) = &self.game else { return };
        let Some(&h) = self.selection.iter().find(|&&h| g.world.unit(h).is_some_and(|u| u.owner == g.world.local_player && !u.is_building)) else { return };
        let attacking = g.world.unit(h).is_some_and(|u| matches!(u.action, w3_game::types::Action::Attack { .. } | w3_game::types::Action::AttackMove { .. }));
        self.ack_clicks = 0;
        self.ack(h, if attacking { "YesAttack" } else { "Yes" }, None);
    }

    /// Creates and updates render instances for destructables.
    pub fn sync_destructables(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, assets: &mut Assets) {
        let Some(g) = self.game.as_ref() else { return };
        let w = &g.world;
        self.dests.retain(|h, _| matches!(w.handles.get(*h), Some(w3_game::types::Obj::Destructable(_))));
        for &h in &w.destructable_order {
            let Some(w3_game::types::Obj::Destructable(d)) = w.handles.get(h) else { continue };
            if !self.dests.contains_key(&h) {
                let Some(o) = w.objects.destructables.get(d.type_id) else { continue };
                let file = o.str("file");
                let model = assets
                    .model(device, queue, &format!("{file}{}", d.variation))
                    .or_else(|| assets.model(device, queue, &file));
                let Some(model) = model else { continue };
                let mut inst = ModelInstance::new(model);
                inst.transform = Mat4::from_scale_rotation_translation(
                    Vec3::splat(d.scale),
                    Quat::from_rotation_z(d.facing.to_radians()),
                    Vec3::new(d.x, d.y, d.z),
                );
                let tex_id = o.int("texID") as u32;
                let tex_file = o.str("texFile");
                if tex_id != 0 && !tex_file.is_empty() {
                    let t = assets.textures.get_or_white(device, queue, &format!("{tex_file}.blp"));
                    inst.replaceable.insert(tex_id, t);
                }
                let alive = d.life > 0.0;
                inst.play(if alive { "stand" } else { "death" });
                if !alive {
                    // Already dead when placed: show the end of the death animation.
                    inst.frame = f32::MAX / 4.0;
                }
                self.dests.insert(h, (inst, alive));
            }
            let (inst, alive) = self.dests.get_mut(&h).unwrap();
            inst.visible = d.visible;
            if *alive && d.life <= 0.0 {
                *alive = false;
                inst.play("death");
            } else if !*alive && d.life > 0.0 {
                *alive = true;
                inst.play("birth");
            }
        }
    }

    /// Creates, updates, and removes render instances to match the game's units.
    pub fn sync_units(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, assets: &mut Assets) {
        let Some(g) = self.game.as_ref() else { return };
        let w = &g.world;
        self.units.retain(|h, _| w.unit(*h).is_some());
        self.selection.retain(|h| w.unit(*h).is_some_and(|u| !u.dead));
        // The portrait shows the first selected unit's portrait model.
        let want = self.selection.first().copied();
        if self.portrait.as_ref().map(|p| p.0) != want {
            self.portrait = want.and_then(|h| {
                let u = w.unit(h)?;
                let o = w.objects.units.get(u.type_id)?;
                let file = o.str("file");
                let base = file.strip_suffix(".mdl").or_else(|| file.strip_suffix(".mdx")).unwrap_or(&file);
                let model = assets.model(device, queue, &format!("{base}_Portrait")).or_else(|| assets.model(device, queue, &file))?;
                let mut v = UnitView { inst: ModelInstance::new(model), serial: 0, requested: "portrait".into(), anim: String::new(), rng: h };
                v.inst.team = u.color.clamp(0, 15) as u8;
                v.pick("portrait");
                Some((h, v))
            });
        }
        if let Some((_, v)) = self.portrait.as_mut() {
            if v.inst.looped || v.inst.finished {
                v.inst.looped = false;
                v.pick("portrait");
            }
        }
        let footprint = self.camera_footprint();
        let team_rgb = self.team_rgb;
        if let Some(hud) = self.hud.as_mut() {
            hud.minimap_dots = w
                .unit_order
                .iter()
                .filter(|&&h| w.unit_visible_to(h, w.local_player) || w.unit(h).is_some_and(|u| u.is_building && w.fog_state(u.x, u.y, w.local_player) != w3_game::fog::FOG_MASKED))
                .filter_map(|&h| w.unit(h))
                .filter(|u| !u.dead && !u.hidden)
                .map(|u| {
                    let c = team_rgb[u.color.clamp(0, 15) as usize];
                    (u.x, u.y, [c[0], c[1], c[2], 1.0], if u.is_building { 0.004 } else { 0.0025 })
                })
                .collect();
            hud.camera_box = footprint;
            // The minimap's fog: FoggedTerrain over fogged cells and black over masked ones.
            if let (Some(area), true) = (hud.minimap_area, w.fog.is_some()) {
                let n = crate::hud::MINIMAP_FOG_SIZE;
                let (sx, sy, side) = area.square();
                let argb = |key: &str| -> [u8; 4] {
                    let v: Vec<u8> = w.misc.get_list("FogOfWar", key).iter().filter_map(|s| s.trim().parse().ok()).collect();
                    if v.len() < 4 { [0, 0, 0, 255] } else { [v[1], v[2], v[3], v[0]] }
                };
                let (fogged, masked) = (argb("FoggedTerrain"), argb("BlackMaskedTerrain"));
                let mut rgba = vec![0u8; (n * n * 4) as usize];
                for py in 0..n {
                    for px in 0..n {
                        let x = sx + (px as f32 + 0.5) / n as f32 * side;
                        // Image rows run from the top (north).
                        let y = sy + (1.0 - (py as f32 + 0.5) / n as f32) * side;
                        let c = match w.fog_state(x, y, w.local_player) {
                            w3_game::fog::FOG_VISIBLE => [0; 4],
                            w3_game::fog::FOG_FOGGED => fogged,
                            _ => masked,
                        };
                        let i = ((py * n + px) * 4) as usize;
                        rgba[i..i + 4].copy_from_slice(&c);
                    }
                }
                assets.textures.set_dynamic(device, queue, crate::hud::MINIMAP_FOG, n, n, &rgba);
                hud.minimap_fog = true;
            }
            hud.card = match self.selection.first() {
                Some(&h) if w.unit(h).is_some_and(|u| u.owner == w.local_player) => self.commands.card(&assets.frames, w, h, hud.page),
                _ => Vec::new(),
            };
        }
        for &h in &w.unit_order {
            let Some(u) = w.unit(h) else { continue };
            if !self.units.contains_key(&h) {
                let Some(o) = w.objects.units.get(u.type_id) else { continue };
                let file = o.str("file");
                let Some(model) = assets.model(device, queue, &file) else { continue };
                let inst = ModelInstance::new(model);
                let mut view = UnitView { inst, serial: u32::MAX, requested: String::new(), anim: String::new(), rng: h };
                view.update(&u.anim, u.anim_serial);
                self.units.insert(h, view);
            }
            let view = self.units.get_mut(&h).unwrap();
            view.update(&u.anim, u.anim_serial);
            // Structures under construction show their birth animation in step with progress.
            if let (Some(left), Some(seq)) = (u.constructing, view.inst.sequence.and_then(|s| view.inst.model.mdx.sequences.get(s))) {
                let len = (seq.end - seq.start) as f32;
                view.inst.frame = len * (1.0 - left / u.build_time.max(0.01)).clamp(0.0, 1.0);
            }
            let inst = &mut view.inst;
            let z = self.terrain.height_at(u.x, u.y) + u.fly_height;
            inst.transform = Mat4::from_scale_rotation_translation(
                Vec3::splat(u.scale),
                Quat::from_rotation_z(u.facing.to_radians()),
                Vec3::new(u.x, u.y, z),
            );
            inst.team = w.players.get(u.owner as usize).map_or(0, |p| p.color.clamp(0, 15) as u8);
            inst.team = u.color.clamp(0, 15) as u8;
            // Enemy units show only where the local player sees; structures stay where
            // they have been explored.
            let state = w.fog_state(u.x, u.y, w.local_player);
            let seen = w.unit_visible_to(h, w.local_player) || u.is_building && state != w3_game::fog::FOG_MASKED;
            inst.visible = !u.hidden && seen;
        }
    }

    pub fn update(&mut self, dt: f32) {
        self.time += dt;
        if let Some(g) = self.game.as_mut() {
            g.tick(dt);
        }
        self.apply_requests();
        // Without audio, keep only the music requests for when it starts.
        if self.audio.is_none() {
            self.pending_audio.retain(|r| matches!(r, w3_game::world::Request::PlayMusic(_) | w3_game::world::Request::StopMusic(_)));
        }
        for inst in &mut self.instances {
            inst.advance(dt * 1000.0);
        }
        for (h, v) in self.units.iter_mut() {
            let building = self.game.as_ref().and_then(|g| g.world.unit(*h)).is_some_and(|u| u.constructing.is_some());
            if !building {
                v.inst.advance(dt * 1000.0);
            }
        }
        for (inst, _) in self.dests.values_mut() {
            inst.advance(dt * 1000.0);
        }
        if let Some((_, v)) = self.portrait.as_mut() {
            v.inst.advance(dt * 1000.0);
        }
        self.camera.target.z = self.terrain.height_at(self.camera.target.x, self.camera.target.y);
    }

    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        color: &wgpu::TextureView,
        depth: &wgpu::TextureView,
        size: (u32, u32),
        assets: &mut Assets,
    ) {
        let aspect = size.0 as f32 / size.1 as f32;
        let view_proj = self.camera.view_proj(aspect);
        let eye = self.camera.eye();
        let rot = self.camera.billboard_rotation();
        let mut globals = default_globals(view_proj, eye);
        let light = self.light;
        globals.light_dir = [light.x, light.y, light.z, 0.0];
        if let Some(g) = &self.game {
            let paths = g.world.day_night_models.clone();
            if paths != self.dnc_paths {
                self.dnc = (Self::load_dnc(&g.world.chain, &paths.0), Self::load_dnc(&g.world.chain, &paths.1));
                self.dnc_paths = paths;
            }
            let hour = g.world.time_of_day;
            if let Some(l) = self.dnc.0.as_ref().and_then(|m| w3_render::dnc::light_at(m, hour)) {
                globals.light_dir = [l.direction.x, l.direction.y, l.direction.z, 0.0];
                globals.terrain_light_color = [l.diffuse.x, l.diffuse.y, l.diffuse.z, 1.0];
                globals.terrain_ambient = [l.ambient.x, l.ambient.y, l.ambient.z, 1.0];
            }
            if let Some(l) = self.dnc.1.as_ref().and_then(|m| w3_render::dnc::light_at(m, hour)) {
                globals.light_color = [l.diffuse.x, l.diffuse.y, l.diffuse.z, 1.0];
                globals.ambient = [l.ambient.x, l.ambient.y, l.ambient.z, 1.0];
            }
        }
        if let (Some(tex), Some(g)) = (&self.fog, &self.game) {
            let w = &g.world;
            // UI\MiscData.txt [FogOfWar]: alpha, red, green, blue.
            let argb = |key: &str| -> [f32; 4] {
                let v: Vec<f32> = w.misc.get_list("FogOfWar", key).iter().filter_map(|s| s.trim().parse().ok()).collect();
                if v.len() < 4 {
                    return [0.0; 4];
                }
                [v[1] / 255.0, v[2] / 255.0, v[3] / 255.0, v[0] / 255.0]
            };
            if let (Some(f), Some((_, _, img))) = (w.fog.as_ref(), w.fog_image(w.local_player, 128)) {
                tex.write(queue, &img);
                globals.fow_extent = [f.origin.0, f.origin.1, f.width as f32 * w3_game::fog::FOG_CELL, f.height as f32 * w3_game::fog::FOG_CELL];
                globals.fow_terrain = argb("FoggedTerrain");
                globals.fow_object = argb("FoggedObject");
            }
        }
        for inst in &mut self.instances {
            // Cull instances well outside the view.
            let p = inst.transform.w_axis.truncate();
            if p.distance(self.camera.target) > self.camera.far * 1.5 {
                continue;
            }
            self.models.submit(inst, eye, rot);
        }
        for v in self.units.values_mut() {
            self.models.submit(&mut v.inst, eye, rot);
        }
        let far = self.camera.far * 1.5;
        let target = self.camera.target;
        for (inst, _) in self.dests.values_mut() {
            if inst.transform.w_axis.truncate().distance(target) > far {
                continue;
            }
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
        if self.hud.is_some() {
            // The world shows only inside the game interface's world frame, which CGameUI
            // places from 0.13 to 0.58 of the 0.6-tall screen (game.dll 0x6f367710).
            let top = ((0.6 - 0.58) / 0.6 * size.1 as f32) as u32;
            let bottom = ((0.6 - 0.13) / 0.6 * size.1 as f32) as u32;
            pass.set_scissor_rect(0, top, size.0, bottom - top);
        }
        self.terrain.render_ground(&mut pass);
        let decals = self.selection_decals(device, queue, assets);
        let terrain = &self.terrain;
        self.decals.render(device, queue, &mut pass, &decals, &|x, y| terrain.height_at(x, y));
        self.models.render(device, queue, &mut pass, &globals);
        self.terrain.render_water(&mut pass, self.time);
        drop(pass);
        if let (Some(hud), Some(g)) = (self.hud.as_mut(), self.game.as_ref()) {
            hud.update(&assets.frames, &g.world);
            hud.show_unit(&assets.frames, &g.world, self.selection.first().copied());
            let draws = hud.draws(&assets.frames);
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("interface"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: color,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
                })],
                ..Default::default()
            });
            self.ui.render(device, queue, &mut assets.textures, &mut pass, &draws, hud.tree.width, size);
            drop(pass);
            let rect = hud.portrait_rect();
            self.render_portrait(device, queue, encoder, color, depth, size, rect);
        }
    }

    /// Draws the portrait model through its own camera in the console's portrait slot.
    #[allow(clippy::too_many_arguments)]
    fn render_portrait(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, color: &wgpu::TextureView, depth: &wgpu::TextureView, size: (u32, u32), rect: w3_ui::Rect) {
        let Some((_, v)) = self.portrait.as_mut() else { return };
        let Some(cam) = v.inst.model.mdx.cameras.first().cloned() else { return };
        let width = self.hud.as_ref().map_or(w3_ui::UI_WIDTH, |h| h.tree.width);
        let px = |x: f32| x / width * size.0 as f32;
        let py = |y: f32| (1.0 - y / w3_ui::UI_HEIGHT) * size.1 as f32;
        let (x0, x1, y0, y1) = (px(rect.x0), px(rect.x1), py(rect.y1), py(rect.y0));
        if x1 <= x0 || y1 <= y0 {
            return;
        }
        let eye = Vec3::from_array(cam.position);
        let target = Vec3::from_array(cam.target);
        let view = Mat4::look_at_rh(eye, target, Vec3::Z);
        let aspect = (x1 - x0) / (y1 - y0);
        // MDX camera field of view is the vertical angle in radians.
        let proj = Mat4::perspective_rh(cam.fov, aspect, cam.near_clip.max(1.0), cam.far_clip.max(100.0));
        let mut globals = default_globals(proj * view, eye);
        globals.light_dir = [0.3, 0.3, -1.0, 0.0];
        globals.ambient = [0.4, 0.4, 0.4, 1.0];
        let rot = Quat::IDENTITY;
        v.inst.transform = Mat4::IDENTITY;
        self.portrait_renderer.submit(&mut v.inst, eye, rot);
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("portrait"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: color,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth,
                depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                stencil_ops: None,
            }),
            ..Default::default()
        });
        pass.set_viewport(x0, y0, x1 - x0, y1 - y0, 0.0, 1.0);
        pass.set_scissor_rect(x0 as u32, y0 as u32, (x1 - x0) as u32, (y1 - y0) as u32);
        self.portrait_renderer.render(device, queue, &mut pass, &globals);
    }
}
