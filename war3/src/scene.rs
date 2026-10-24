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

/// A unit's command card: the local player's own buttons (and goods), or a neutral
/// shop's goods, which show to everyone.
fn card_of(commands: &crate::card::Commands, db: &w3_ui::FrameDb, w: &w3_game::World, h: u32, page: crate::card::Page) -> Vec<crate::card::Button> {
    if w3_game::natives::players::controls(w, w.local_player, h) {
        let mut card = commands.card(db, w, h, page);
        if page == crate::card::Page::Main {
            card.extend(commands.shop_card(w, h));
        }
        card
    } else if w.ability_with_code(h, "Aneu").or_else(|| w.ability_with_code(h, "Ane2")).is_some() {
        commands.shop_card(w, h)
    } else {
        Vec::new()
    }
}

/// A special effect's model: born, standing, and dying once destroyed.
pub struct EffectView {
    pub inst: ModelInstance,
    /// Whether its destruction has been seen (its death playing).
    dying: bool,
    /// Finished and no longer drawn.
    done: bool,
}

/// An attachment point of a model instance in world space: the attachment node whose
/// name (less "Ref") has the same words as `name` ("hand,left" is "Hand Left Ref"), else
/// one with all of them; the origin for "origin" or when none matches.
pub fn attachment_point(inst: &ModelInstance, name: &str) -> Vec3 {
    let origin = inst.transform.w_axis.truncate();
    let want: Vec<String> = name.split([' ', ',']).filter(|s| !s.is_empty()).map(|s| s.to_ascii_lowercase()).collect();
    let words = |n: &str| -> Vec<String> { n.split_whitespace().map(|s| s.to_ascii_lowercase()).filter(|s| s != "ref").collect() };
    let mdx = &inst.model.mdx;
    let same = |n: &str| {
        let w = words(n);
        w.len() == want.len() && want.iter().all(|x| w.contains(x))
    };
    let has = |n: &str| {
        let w = words(n);
        want.iter().all(|x| w.contains(x))
    };
    let Some(a) = mdx.attachments.iter().find(|a| same(&a.node.name)).or_else(|| mdx.attachments.iter().find(|a| has(&a.node.name))) else { return origin };
    let id = a.node.object_id as usize;
    let pivot = mdx.pivots.get(id).map_or(Vec3::ZERO, |p| Vec3::from_array(*p));
    let local = inst.skeleton.world.get(id).copied().unwrap_or(Mat4::IDENTITY).transform_point3(pivot);
    inst.transform.transform_point3(local)
}

/// Draws the models on a view's attachment nodes: each shown while its attachment's
/// visibility track (KATV) is on, at the node, playing the sequence named like the
/// parent's at the same point.
fn submit_children(models: &mut w3_render::model::ModelRenderer, v: &mut UnitView, eye: Vec3, rot: Quat) {
    if v.children.is_empty() || !v.inst.visible {
        return;
    }
    let parent = &v.inst;
    let mdx = &parent.model.mdx;
    let time = parent.time();
    let seq = parent.sequence.and_then(|s| mdx.sequences.get(s));
    for (ai, child) in &mut v.children {
        let Some(a) = mdx.attachments.get(*ai) else { continue };
        let shown = w3_render::anim::sample(a.tracks.f32(b"KATV"), &time, &mdx.global_sequences, 1.0) > 0.0;
        child.visible = shown;
        if !shown {
            continue;
        }
        let id = a.node.object_id as usize;
        let pivot = mdx.pivots.get(id).map_or(Vec3::ZERO, |p| Vec3::from_array(*p));
        let node = parent.skeleton.world.get(id).copied().unwrap_or(Mat4::IDENTITY);
        child.transform = parent.transform * node * Mat4::from_translation(pivot);
        if let Some(s) = seq {
            let name = s.name.split_whitespace().next().unwrap_or("stand").to_string();
            let want = w3_render::anim::find_sequence(&child.model.mdx, &name);
            if want.is_some() && child.sequence != want {
                child.set_sequence(want);
            }
            if let Some(cs) = child.sequence.and_then(|c| child.model.mdx.sequences.get(c)) {
                let plen = (s.end - s.start).max(1) as f32;
                child.frame = parent.frame / plen * (cs.end - cs.start) as f32;
            }
        }
        child.global_time = parent.global_time;
        models.submit(child, eye, rot);
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum LoopKey {
    Moving(u32),
    Building(u32),
    /// A burning structure's fire.
    Fire(u32),
    Doodad(usize),
}

pub struct UnitView {
    pub inst: ModelInstance,
    /// The unit type the model is for (a morph changes it).
    pub type_id: [u8; 4],
    /// Models on attachment nodes that name one (a structure's "BirthLink"), by
    /// attachment index.
    pub children: Vec<(usize, ModelInstance)>,
    serial: u32,
    requested: String,
    anim: String,
    rng: u32,
    /// How many of QueueUnitAnimation's animations have played.
    queued: usize,
}

impl UnitView {
    fn pick(&mut self, name: &str) {
        // SetUnitAnimationByIndex: the model's sequence of that number.
        if let Some(n) = name.strip_prefix('#').and_then(|n| n.parse::<usize>().ok()) {
            if n < self.inst.model.mdx.sequences.len() {
                self.inst.set_sequence(Some(n));
                return;
            }
        }
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
    fn update(&mut self, anim: &str, serial: u32, queue: &[String]) {
        if serial != self.serial || anim != self.requested {
            self.serial = serial;
            self.requested = anim.to_string();
            self.anim = anim.to_string();
            self.queued = 0;
            self.pick(anim);
            return;
        }
        if self.inst.finished {
            // QueueUnitAnimation: the queued ones play in turn.
            if let Some(next) = queue.get(self.queued) {
                self.queued += 1;
                self.anim = next.clone();
                self.pick(next);
                return;
            }
            // A stand of any kind ("stand work gold") plays again.
            let again = self.requested.starts_with("stand").then(|| self.requested.clone());
            let next = match self.anim.as_str() {
                "death" => Some("decay flesh".to_string()),
                "decay flesh" => Some("decay bone".to_string()),
                "decay bone" => None,
                _ => Some(again.unwrap_or_else(|| "stand".to_string())),
            };
            if let Some(n) = next {
                self.anim = n.clone();
                self.pick(&n);
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
    /// Each destructable's last script animation request and its speed.
    dest_anims: HashMap<u32, (u32, f32)>,
    /// Render instances for items lying on the ground.
    pub items: HashMap<u32, ModelInstance>,
    /// Special effects (AddSpecialEffect and ability art), by handle.
    pub effects: HashMap<u32, EffectView>,
    pub selection: Vec<u32>,
    selection_serial: u32,
    /// The world's blight count the ground was last built with.
    blight_serial: u32,
    /// Splats\UberSplatData.slk, and when each splat (by unit or ubersplat handle)
    /// was first drawn.
    uber_slk: w3_formats::slk::Table,
    /// The map as the game was started with it, for saved games.
    pub map_path: String,
    /// Where saved games are kept.
    pub saves_dir: std::path::PathBuf,
    /// A saved game the player chose to load.
    pub load_requested: Option<std::path::PathBuf>,
    /// The game starts over (Restart, RestartGame).
    pub restart_requested: bool,
    /// The setup the game started with, for restarting it.
    pub lobby: Option<Vec<w3_game::LobbySlot>>,
    /// UI\\HelpStrings.txt, and UI\\TipStrings.txt's tips and the one shown.
    help_text: String,
    tips: Vec<String>,
    tip: usize,
    /// The player's options, the options page open in the game menu, whether this is a
    /// campaign mission (the options show its difficulty), and the window's size.
    pub options: crate::options::Options,
    options_page: &'static str,
    campaign: bool,
    screen: (u32, u32),
    /// A game menu slider being dragged; an options page to fill in.
    menu_drag: bool,
    /// The replay being watched, if any.
    pub playback: Option<w3_game::replay::Playback>,
    /// The network game this is, if any.
    pub net: Option<crate::net::Session>,
    /// The players the host waits on, and whether the host is silent.
    net_waiting: (Vec<u8>, bool),
    /// The Allies button was pressed; the dialog opens with the next interface update.
    alliance_requested: bool,
    /// The chat recipient the chat dialog chose, once it has.
    chat_default: Option<u8>,
    /// The chat dialog opens with the next interface update.
    chat_dialog_requested: bool,
    /// The minimap's signal button was pressed: the next click signals there.
    signaling: bool,
    options_fill: bool,
    /// Structures' and destructables' shadows stamped into the terrain.
    shadows: crate::shadows::Shadows,
    /// The pointer's screen position, and whether Alt is held (health bars show).
    cursor: (f32, f32),
    pub alt_held: bool,
    /// Shift is down: orders queue after the ones under way.
    pub shift_held: bool,
    /// Ctrl is down: a click selects the unit's type on screen.
    pub ctrl_held: bool,
    /// The unit last clicked and when, for double-clicks.
    last_pick: Option<(u32, f32)>,
    weather: crate::weather::WeatherView,
    lightning: crate::lightning::LightningView,
    /// Ambient loops of enabled weather effects the camera is inside, by weather handle.
    weather_sounds: HashMap<u32, u64>,
    /// Doodads' looping sounds (Doodads.slk soundLoop) and where they are.
    doodad_loops: Vec<(String, [f32; 3])>,
    /// Playing looping sounds: units moving (MovementSoundLabel) or under construction
    /// (BuildingSoundLabel), and doodads in earshot; with their fade-out milliseconds.
    loops: HashMap<LoopKey, (u64, f32)>,
    splat_births: HashMap<u32, f32>,
    pub info: MapInfo,
    pub terrain: TerrainRenderer,
    pub models: ModelRenderer,
    pub instances: Vec<ModelInstance>,
    /// Each doodad instance's type.
    doodad_ids: Vec<[u8; 4]>,
    /// The cinematic camera model in use, by path.
    cine_camera: Option<(String, Option<w3_formats::mdx::Model>)>,
    /// The camera target last drawn, which CameraSetSmoothingFactor eases from, and the
    /// frame's length.
    smoothed_target: Option<Vec3>,
    frame_dt: f32,
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
    pub targeting: Option<i32>,
    /// Missiles in flight by id, and those that landed playing their death.
    missiles: HashMap<u32, EffectView>,
    /// The model of the structure being placed, under the pointer (CConstructUI).
    ghost: Option<(w3_game::objects::Id, ModelInstance)>,
    /// A structure waiting for a placement click.
    pub placing: Option<w3_game::objects::Id>,
    commands: crate::card::Commands,
    /// Player colors, read from the solid ReplaceableTextures\TeamColor images.
    team_rgb: [[f32; 3]; 16],
    portrait_renderer: ModelRenderer,
    /// The console's SPRITE frames (CTimeOfDayIndicator) and their renderer.
    hud_sprites: ModelRenderer,
    /// The sky (SetSkyModel): its model around the eye, drawn before everything else
    /// (CWorldFrameWar3's first layer), and its renderer.
    sky: Option<(String, ModelInstance)>,
    sky_renderer: ModelRenderer,
    /// Indicators flashed on widgets (UnitAddIndicator, and the target of a right
    /// click in TargetingColors' SelTargetColor): the widget, its color, and when.
    indicators: Vec<(u32, [f32; 4], f32)>,
    /// The autocast indicator on each card slot whose autocast is on.
    quest_fx: Option<ModelInstance>,
    /// The race's cursor model, the sequence it plays, and the edge scroll direction the
    /// app last moved the camera in (x, y each -1, 0, or 1).
    cursor_model: Option<ModelInstance>,
    cursor_sequence: String,
    pub scroll_dir: (i32, i32),
    /// Minimap pings: where, until when, their color, and their model.
    pings: Vec<(f32, f32, f32, [f32; 4], Option<ModelInstance>)>,
    autocast_fx: HashMap<(u32, u32), ModelInstance>,
    time_indicator: Option<ModelInstance>,
    portrait: Option<(u32, UnitView)>,
    /// ChangeLevel's next map, for the app to load.
    pub change_level: Option<String>,
    /// The transmission's portrait: its unit type and model.
    cine_portrait: Option<(w3_game::objects::Id, UnitView)>,
    fog: Option<Arc<w3_render::fog::FogTexture>>,
    /// Sound output; absent when running without audio.
    pub audio: Option<w3_audio::Audio>,
    /// Sound and music requests waiting for [`Scene::update_audio`].
    pending_audio: Vec<w3_game::world::Request>,
    /// Script sound handles to playing voices.
    voices: HashMap<u32, u64>,
    /// The game has ended (EndGame); the application returns to the menus.
    pub ended: bool,
    /// The scores to show when the game ends with the score screen.
    pub final_score: Option<crate::scorescreen::ScoreData>,
    /// Exit Program was chosen.
    pub exit: bool,
    menu_requested: bool,
    /// DisplayLoadDialog: the menu opens on its load panel.
    load_dialog_requested: bool,
    /// The quest log is to open (F9 or the Quests button).
    quests_requested: bool,
    /// An item waiting for the click on its target: the unit and the slot.
    item_target: Option<(u32, usize)>,
    /// An item picked up with a right-click to be moved, dropped, or handed over.
    held_item: Option<(u32, usize)>,
    /// Control groups 0 to 9, and the last one chosen and when (to look at it when
    /// chosen again soon after).
    groups: [Vec<u32>; 10],
    last_group: Option<(usize, f32)>,
    /// The message log is to open (F12 or the Log button).
    log_requested: bool,
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
        lobby: Option<&[w3_game::LobbySlot]>,
        restore: Option<w3_game::save::SaveGame>,
        replay: Option<w3_game::replay::Replay>,
    ) -> anyhow::Result<Scene> {
        // A replay plays its own setup and profile.
        let replay_lobby = replay.as_ref().and_then(|r| r.lobby.clone());
        let lobby = if replay.is_some() { replay_lobby.as_deref() } else { lobby };
        let campaign = replay.as_ref().map_or(data.campaign_game.get(), |r| r.campaign);
        data.chain.unmount(PRIORITY_MAP);
        // A map on disk, or one inside the game's archives (the campaigns' maps).
        let archive = if map_path.is_file() {
            w3_mpq::Archive::open(map_path).with_context(|| format!("opening map {}", map_path.display()))?
        } else {
            let name = map_path.to_string_lossy().replace('/', "\\");
            let bytes = data.chain.read(&name).with_context(|| format!("reading map {name}"))?;
            w3_mpq::Archive::from_bytes(map_path, bytes).with_context(|| format!("opening map {name}"))?
        };
        let archive = Arc::new(archive);
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
            models.lights.clone(),
            &terrain,
            &terrain_slk,
            &water_slk,
            &cliff_slk,
            &{
                // Light, blight, and other settings, with the tilesets' blight textures.
                let mut p = data.misc.clone();
                if let Ok(s) = chain.read_string("UI\\WorldEditData.txt") {
                    let we = w3_formats::profile::Profile::parse(&s);
                    if let Some(t) = we.sections.get("tilesets") {
                        p.sections.insert("tilesets".into(), t.clone());
                    }
                }
                p
            },
            &mut load_mdx,
            chain.read("war3map.shd").ok().as_deref(),
        );

        let mut instances = Vec::new();
        let mut doodad_ids = Vec::new();
        let mut doodad_loops = Vec::new();
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
                        let t = assets.textures.get_or_white(device, queue, &tex_file);
                        inst.replaceable.insert(tex_id, t);
                    }
                }
                inst.play("Stand");
                instances.push(inst);
                doodad_ids.push(d.id);
                let sound = text(table, &id, "soundLoop");
                if !sound.is_empty() && sound != "_" && sound != "-" {
                    doodad_loops.push((sound, d.position));
                }
            }
        }

        // Run the map script: config, then main with the local player in the first
        // user slot.  Other slots stay empty until computer players are implemented.
        // The customkeys option lays the player's CustomKeys.txt, beside the game's
        // archives, over the game's hotkeys, tips, and button positions.
        let custom_keys = crate::options::Options::load().customkeys.then(|| std::fs::read(data.dir.join("CustomKeys.txt")).ok()).flatten().map(|b| w3_game::objects::custom_keys(&String::from_utf8_lossy(&b)));
        let mut game = match w3_game::Game::load_with_keys(data.chain.clone(), data.version as i32, custom_keys.as_ref()) {
            Ok(mut g) if restore.is_some() => {
                g.restore(restore.unwrap())?;
                g.world.saves_dir = crate::data::save_dir(data.version);
                Some(g)
            }
            Ok(mut g) => {
                g.world.saves_dir = crate::data::save_dir(data.version);
                // The profile comes in before the script runs (InitGameCache reads saved
                // caches); campaign games take its difficulty.
                g.world.persist = replay.as_ref().map_or_else(|| data.persist.borrow().clone(), |r| r.persist.clone());
                let persist_at_start = g.world.persist.clone();
                if campaign && g.world.persist.difficulty > 0 {
                    g.world.difficulty = g.world.persist.difficulty;
                }
                if let Err(e) = g.config() {
                    log::error!("config: {e:#}");
                }
                let mut first = true;
                let res = match lobby {
                    Some(slots) => g.start_lobby(slots),
                    // Without a setup screen: the local player in the first user slot and
                    // computers in the rest.
                    None => g.start(|_, controller| {
                        if controller == w3_game::types::map_control::USER && first {
                            first = false;
                            w3_game::SlotFill::Local
                        } else {
                            w3_game::SlotFill::Computer
                        }
                    }),
                };
                if let Err(e) = res {
                    log::error!("main: {e:#}");
                }
                // The game is recorded from its start; a replay's is locked to it.
                if replay.is_some() {
                    g.locked = true;
                } else {
                    g.recorder = Some(w3_game::replay::Recorder::new(w3_game::replay::Replay {
                        map: map_path.to_string_lossy().to_string(),
                        lobby: lobby.map(|l| l.to_vec()),
                        persist: persist_at_start,
                        campaign,
                        version: data.version as i32,
                        frames: Vec::new(),
                    }));
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
        // The game camera starts on the local player's start location unless the map's
        // script placed it.
        if let Some(g) = game.as_mut() {
            if g.world.camera.x == 0.0 && g.world.camera.y == 0.0 {
                let (sx, sy) = g.world.start_of(g.world.local_player);
                let (sx, sy) = if sx == 0.0 && sy == 0.0 { (start[0], start[1]) } else { (sx, sy) };
                g.world.camera.pan_to(sx, sy, 0.0);
            }
        }
        let dir: Vec<f32> = data.misc.get_list("Light", "Direction").iter().filter_map(|v| v.trim().parse().ok()).collect();
        let light = if dir.len() == 3 { Vec3::new(dir[0], dir[1], dir[2]) } else { Vec3::new(0.3, 0.3, -0.25) };
        let hud = game.as_ref().map(|g| {
            let race = g.world.players[g.world.local_player as usize].race;
            crate::hud::Hud::new(&mut assets.frames, aspect, race)
        });
        let mut hud = hud;
        // The map's minimap icons (war3map.mmp: count, then kind, x, y, and color each).
        if let (Some(h), Ok(b)) = (hud.as_mut(), data.chain.read("war3map.mmp")) {
            let int = |o: usize| b.get(o..o + 4).map(|s| i32::from_le_bytes(s.try_into().unwrap()));
            let n = int(4).unwrap_or(0).max(0) as usize;
            h.minimap_icons = (0..n).filter_map(|i| Some((int(8 + i * 16)?, int(12 + i * 16)? as f32, int(16 + i * 16)? as f32))).collect();
        }
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
        let hud_sprites = ModelRenderer::new(device, queue, &mut assets.textures, color_format);
        let sky_renderer = ModelRenderer::new(device, queue, &mut assets.textures, color_format);
        let decals = w3_render::decal::DecalRenderer::new(device, color_format, &models.globals_buf);
        // The map's interface settings (war3mapSkin.txt): command strings and errors, and
        // [CustomSkin] keys over the skins.
        let mut commands = crate::card::Commands::load(&data.chain, custom_keys.as_ref());
        let map_skin = game.as_ref().and_then(|g| g.world.chain.read_string("war3mapSkin.txt").ok()).unwrap_or_default();
        commands.profile.merge(&map_skin);
        assets.frames.custom_skin = w3_formats::profile::Profile::parse(&map_skin).sections.remove("customskin").unwrap_or_default();
        let mut scene = Scene { game, units: HashMap::new(), dests: HashMap::new(), items: HashMap::new(), dest_anims: HashMap::new(), effects: HashMap::new(), selection: Vec::new(), selection_serial: 0, blight_serial: 0, uber_slk: data.slk("Splats\\UberSplatData.slk"), map_path: map_path.to_string_lossy().to_string(), saves_dir: crate::data::save_dir(data.version).unwrap_or_default(), load_requested: None, restart_requested: false, lobby: lobby.map(|l| l.to_vec()), help_text: data.chain.read_string("UI\\HelpStrings.txt").unwrap_or_default().replace('\r', ""), tips: load_tips(&data.chain), tip: 0, options: crate::options::Options::load(), options_page: "OptionsPanel", campaign: data.campaign_game.get(), screen: (0, 0), menu_drag: false, options_fill: false, shadows: Default::default(), cursor: (-1.0, -1.0), alt_held: false, shift_held: false, ctrl_held: false, last_pick: None, weather: Default::default(), lightning: Default::default(), weather_sounds: HashMap::new(), doodad_loops, loops: HashMap::new(), splat_births: HashMap::new(), info, terrain: terrain_r, models, instances, doodad_ids, cine_camera: None, smoothed_target: None, frame_dt: 0.0, camera, time: 0.0, light, dnc: (None, None), dnc_paths: Default::default(), hud, ui, decals, drag: None, targeting: None, missiles: HashMap::new(), ghost: None, placing: None, commands, team_rgb: team_colors(&data.chain), portrait_renderer, hud_sprites, sky: None, sky_renderer, indicators: Vec::new(), quest_fx: None, cursor_model: None, cursor_sequence: String::new(), scroll_dir: (0, 0), pings: Vec::new(), autocast_fx: HashMap::new(), time_indicator: None, portrait: None, cine_portrait: None, change_level: None, fog: None, audio: None, pending_audio: Vec::new(), voices: HashMap::new(), ack_unit: 0, ack_clicks: 0, ack_voice: None, ended: false, final_score: None, exit: false, menu_requested: false, load_dialog_requested: false, quests_requested: false, item_target: None, held_item: None, groups: Default::default(), last_group: None, log_requested: false, playback: replay.map(|r| w3_game::replay::Playback { replay: r, next: 0, owed: 0.0 }), net: None, net_waiting: (Vec::new(), false), alliance_requested: false, chat_default: None, chat_dialog_requested: false, signaling: false };
        if let Some(f) = scene.game.as_ref().and_then(|g| g.world.fog.as_ref()) {
            let tex = Arc::new(w3_render::fog::FogTexture::new(device, queue, f.width as u32, f.height as u32));
            scene.terrain.set_fog(device, &tex);
            scene.models.set_fog(device, tex.clone());
            scene.fog = Some(tex);
        }
        // A replay's panel takes the command card's place.
        if let (Some(p), Some(h), Some(g)) = (scene.playback.as_ref(), scene.hud.as_mut(), scene.game.as_mut()) {
            h.show_replay(&assets.frames, &g.world, &scene.team_rgb, p.replay.length());
            if let Some(r) = h.replay.as_mut() {
                r.leave(&mut g.world);
            }
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
                w3_game::world::Request::DisplayText { players, x, y, text, duration } => {
                    log::info!("[text] {text}");
                    if players & local != 0 {
                        if let Some(h) = self.hud.as_mut() {
                            h.unit_message(&text, x, y, duration);
                        }
                    }
                }
                // A chat line: "[All] name: line" in the player's color.
                w3_game::world::Request::Chat { player, text, to } => {
                    let Some(w) = self.game.as_ref().map(|g| &g.world) else { continue };
                    // Allies' lines reach the speaker and their allies only, a private line
                    // the speaker and its player.
                    let me = w.local_player;
                    if to == 1 && me != player && !w3_game::natives::players::is_ally(w, player, me) {
                        continue;
                    }
                    if to >= 3 && me != player && me + 3 != to {
                        continue;
                    }
                    // Squelched players are not heard.
                    if self.hud.as_ref().is_some_and(|h| h.squelched.contains(&player)) {
                        continue;
                    }
                    let recipient = w.localized(match to {
                        0 => "CHAT_RECIPIENT_ALL",
                        1 => "CHAT_RECIPIENT_ALLIES",
                        2 => "CHAT_RECIPIENT_OBSERVERS",
                        _ => "CHAT_RECIPIENT_PRIVATE",
                    });
                    let p = &w.players[player as usize % w.players.len()];
                    let c = self.team_rgb[p.color as usize % 16].map(|v| (v * 255.0).round() as u8);
                    let line = format!("{recipient} |c{:02x}{:02x}{:02x}{:02x}{}: |r{text}", 0xff, c[0], c[1], c[2], p.name);
                    log::info!("[chat] {line}");
                    if let Some(h) = self.hud.as_mut() {
                        h.unit_message(&line, 0.0, 0.0, -1.0);
                        h.chat_history.push(line);
                    }
                }
                w3_game::world::Request::DoodadAnimation { id, area, anim, random } => self.doodad_animation(id, &area, &anim, random),
                w3_game::world::Request::Indicator { widget, color } => self.add_indicator(widget, color.map(|c| c as f32 / 255.0)),
                // ForceUIKey: the interface acts on the key as if pressed; ForceUICancel
                // drops a targeting or placement in progress.
                w3_game::world::Request::ForceUIKey(k) => {
                    if let Some(c) = k.chars().next() {
                        self.key(c);
                    }
                }
                w3_game::world::Request::ForceUICancel => {
                    self.targeting = None;
                    self.placing = None;
                    self.ghost = None;
                }
                w3_game::world::Request::DisplayLoadDialog => {
                    self.menu_requested = true;
                    self.load_dialog_requested = true;
                }
                // A player's signal: five seconds in their color, for them and their
                // allies (unless /hideminimapsignals hides others').
                w3_game::world::Request::AllySignal { player, x, y } => {
                    let Some(w) = self.game.as_ref().map(|g| &g.world) else { continue };
                    let me = w.local_player;
                    let hidden = self.hud.as_ref().is_some_and(|h| h.hide_signals);
                    if me != player && (hidden || !w3_game::natives::players::is_ally(w, player, me)) {
                        continue;
                    }
                    let c = self.team_rgb[w.players[player as usize % w.players.len()].color as usize % 16];
                    self.pings.push((x, y, self.time + 5.0, [c[0], c[1], c[2], 1.0], None));
                    if self.pings.len() > 16 {
                        self.pings.remove(0);
                    }
                }
                // A ping lasts its duration (1.5 seconds when none), in its color or
                // UI\MiscData.txt [PingColor] PingColor (FUN_6f3b0a30).
                w3_game::world::Request::Ping { x, y, duration, color } => {
                    let color = match color {
                        Some(c) => [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0, 1.0],
                        None => {
                            let v: Vec<f32> = self.game.as_ref().map(|g| g.world.misc.get_list("PingColor", "PingColor")).unwrap_or_default().iter().filter_map(|s| s.trim().parse().ok()).collect();
                            if v.len() == 4 { [v[1] / 255.0, v[2] / 255.0, v[3] / 255.0, v[0] / 255.0] } else { [0.0, 1.0, 0.0, 1.0] }
                        }
                    };
                    let until = self.time + if duration > 0.0 { duration } else { 1.5 };
                    self.pings.push((x, y, until, color, None));
                    if self.pings.len() > 16 {
                        self.pings.remove(0);
                    }
                }
                w3_game::world::Request::ClearText { players } if players & local != 0 => {
                    if let Some(h) = self.hud.as_mut() {
                        h.unit_messages.clear();
                    }
                }
                w3_game::world::Request::EndGame { score } => {
                    self.ended = true;
                    if score {
                        self.final_score = self.game.as_ref().map(|g| crate::scorescreen::ScoreData::from_world(&g.world));
                    }
                }
                w3_game::world::Request::Restart => self.restart_requested = true,
                w3_game::world::Request::SaveGame(name) => {
                    if let Some(path) = self.game.as_ref().and_then(|g| g.world.save_path(&name)) {
                        self.write_save(&path);
                    }
                }
                w3_game::world::Request::LoadGame(name) => {
                    self.load_requested = self.game.as_ref().and_then(|g| g.world.save_path(&name)).filter(|p| p.exists());
                }
                w3_game::world::Request::ChangeLevel { map, score } => {
                    self.change_level = Some(map);
                    if score {
                        self.final_score = self.game.as_ref().map(|g| crate::scorescreen::ScoreData::from_world(&g.world));
                    }
                }
                r @ (w3_game::world::Request::PlaySound(_)
                | w3_game::world::Request::UiSound { .. }
                | w3_game::world::Request::WorldSound { .. }
                | w3_game::world::Request::Ack { .. }
                | w3_game::world::Request::StopSound { .. }
                | w3_game::world::Request::PlayMusic(_)
                | w3_game::world::Request::PlayThematic(_)
                | w3_game::world::Request::EndThematic
                | w3_game::world::Request::ResumeMusic
                | w3_game::world::Request::MusicVolume(_)
                | w3_game::world::Request::StopMusic(_)) => self.pending_audio.push(r),
                _ => {}
            }
        }
    }

    /// Plays a unit's acknowledgment line (UnitAckSounds `<unitSound><kind>`), replacing
    /// the one playing.  Returns whether a line started.
    fn ack(&mut self, h: u32, kind: &str, variant: Option<usize>) -> bool {
        // The Unit Responses option.
        if !self.options.unit {
            return false;
        }
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
        let text = g.world.localized(&text);
        log::info!("[error] {text}");
        if let Some(h) = self.hud.as_mut() {
            h.error(&text);
        }
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
        let owned = |s: &Self, h: u32| s.game.as_ref().is_some_and(|g| w3_game::natives::players::controls(&g.world, local, h));
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
        let listener = self.camera.target;
        let Some(audio) = self.audio.as_mut() else { return };
        let Some(g) = self.game.as_mut() else { return };
        for r in pending {
            match r {
                w3_game::world::Request::PlaySound(h) => {
                    let Some(w3_game::types::Obj::Sound(s)) = g.world.handles.get(h) else { continue };
                    let s = (**s).clone();
                    let pos = if s.is_3d { s.position.map(|p| [p.0, p.1, p.2]) } else { None };
                    // CreateSound's stopWhenOutOfRange: a 3D sound beyond its cutoff from the
                    // listener does not play.
                    let cutoff = s.label.as_deref().and_then(|l| audio.def(l)).map_or(s.cutoff, |d| d.cutoff);
                    if let (true, Some(p)) = (s.stop_when_out_of_range && cutoff > 0.0, pos) {
                        if Vec3::from_array(p).distance(listener) > cutoff {
                            continue;
                        }
                    }
                    let id = match (&s.label, s.path.is_empty()) {
                        // A label supplies the file list and parameters unless a file is named.
                        (Some(l), true) => audio.play_label(l, pos),
                        _ => {
                            let def = s.label.as_deref().and_then(|l| audio.def(l)).cloned();
                            let d = def.map_or((s.min_dist, s.max_dist, s.cutoff), |d| (d.min_distance, d.max_distance, d.cutoff));
                            let id = audio.play_file(&s.path, s.volume.clamp(0, 127) as f32 / 127.0, s.pitch.max(0.01), pos, d, s.looping);
                            // Its channel (SetSoundChannel) puts it in a volume group.
                            audio.set_channel(id, s.channel)
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
                w3_game::world::Request::PlayThematic(path) => audio.play_thematic(&path),
                w3_game::world::Request::EndThematic => audio.end_thematic(),
                w3_game::world::Request::ResumeMusic => audio.resume_music(),
                w3_game::world::Request::MusicVolume(v) => audio.set_script_music_volume(v),
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
        // Looping sounds: units moving or under construction that the local player sees,
        // and doodads, within the label's cutoff.
        let mut want: Vec<(LoopKey, String, [f32; 3], f32, f32)> = Vec::new();
        let in_range = |audio: &w3_audio::Audio, label: &str, p: [f32; 3]| {
            let Some(d) = audio.def(label) else { return false };
            let r = if d.cutoff > 0.0 { d.cutoff } else { d.max_distance };
            (p[0] - t.x).powi(2) + (p[1] - t.y).powi(2) + (p[2] - t.z).powi(2) <= r * r
        };
        for (&h, v) in &self.units {
            let Some(u) = g.world.unit(h) else { continue };
            if !v.inst.visible || u.dead {
                continue;
            }
            let Some(o) = g.world.objects.units.get(u.type_id) else { continue };
            let p = v.inst.transform.w_axis;
            let p = [p.x, p.y, p.z];
            let fades = (o.real("LoopingSoundFadeIn"), o.real("LoopingSoundFadeOut"));
            if let Some(label) = g.world.fire_sound(h).filter(|l| in_range(audio, l, p)) {
                want.push((LoopKey::Fire(h), label, p, 0.0, 0.0));
            }
            let (key, label) = if u.constructing.is_some() {
                (LoopKey::Building(h), o.str("BuildingSoundLabel"))
            } else if u.anim.starts_with("walk") && self.options.movement {
                (LoopKey::Moving(h), o.str("MovementSoundLabel"))
            } else {
                continue;
            };
            if !label.is_empty() && in_range(audio, &label, p) {
                want.push((key, label, p, fades.0, fades.1));
            }
        }
        // Doodads' and weather's ambient sounds go with the Ambient Sounds option.
        let ambient = self.options.ambient;
        for (i, (label, p)) in self.doodad_loops.iter().enumerate() {
            if ambient && in_range(audio, label, *p) {
                want.push((LoopKey::Doodad(i), label.clone(), *p, 0.0, 0.0));
            }
        }
        self.loops.retain(|k, (id, fade_out)| {
            let keep = want.iter().any(|w| w.0 == *k) && audio.is_playing(*id);
            if !keep {
                audio.fade_out(*id, *fade_out);
            }
            keep
        });
        for (key, label, p, fade_in, fade_out) in want {
            match self.loops.get(&key) {
                Some((id, _)) => audio.set_position(*id, p),
                None => {
                    if let Some(id) = audio.play_label_repeating(&label, Some(p)) {
                        if fade_in > 0.0 {
                            audio.fade_in(id, fade_in);
                        }
                        self.loops.insert(key, (id, fade_out));
                    }
                }
            }
        }
        // Weather ambience plays while the camera is inside an enabled effect's rect.
        let mut want = Vec::new();
        for (&h, o) in g.world.handles.iter() {
            if let w3_game::types::Obj::Weather(w) = o {
                let r = w.rect;
                if ambient && w.enabled && (r.min_x..=r.max_x).contains(&t.x) && (r.min_y..=r.max_y).contains(&t.y) {
                    if let Some(label) = self.weather.ambient(w.id) {
                        want.push((h, label));
                    }
                }
            }
        }
        self.weather_sounds.retain(|h, id| {
            let keep = want.iter().any(|(w, _)| w == h);
            if !keep {
                audio.stop(*id);
            }
            keep
        });
        for (h, label) in want {
            if !self.weather_sounds.contains_key(&h) {
                if let Some(id) = audio.play_label_looping(&label) {
                    self.weather_sounds.insert(h, id);
                }
            }
        }
        let (sound, music) = self.options.volumes();
        audio.set_volumes(sound, music);
        if let Some(g) = self.game.as_ref() {
            audio.set_channel_gains(g.world.channel_gains());
        }
        audio.update([t.x, t.y, t.z]);
    }

    fn load_dnc(chain: &w3_mpq::Chain, path: &str) -> Option<w3_formats::mdx::Model> {
        let p = if path.to_ascii_lowercase().ends_with(".mdl") { format!("{}.mdx", &path[..path.len() - 4]) } else { path.to_string() };
        chain.read(&p).ok().and_then(|b| w3_formats::mdx::Model::parse(&b).ok())
    }

    /// Ground splats (Splats\UberSplatData.slk): a structure's UnitUI uberSplat from the
    /// start of its construction until it decays after death, and scripts' ubersplats.
    /// Each fades from its start color to its middle over BirthTime, holds, and fades to
    /// its end over Decay once its owner is gone; it is twice its Scale across.
    fn ubersplat_decals(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, assets: &mut Assets) -> Vec<w3_render::decal::Decal> {
        let Some(g) = &self.game else { return vec![] };
        let w = &g.world;
        let local = w.local_player;
        let slk = &self.uber_slk;
        let num = |row: &str, col: &str| -> f32 {
            match slk.get(row, col) {
                Some(w3_formats::slk::Value::Num(n)) => *n as f32,
                Some(v) => v.as_str().trim().parse().unwrap_or(0.0),
                None => 0.0,
            }
        };
        let color = |row: &str, phase: &str| [num(row, &format!("{phase}R")), num(row, &format!("{phase}G")), num(row, &format!("{phase}B")), num(row, &format!("{phase}A"))].map(|v| (v / 255.0).clamp(0.0, 1.0));
        let mut wanted: Vec<(u32, String, f32, f32, Option<f32>, [f32; 4])> = Vec::new();
        for &h in &w.unit_order {
            let Some(u) = w.unit(h) else { continue };
            let Some(o) = w.objects.units.get(u.type_id) else { continue };
            let code = o.str("uberSplat");
            if code.is_empty() || u.hidden || w.fog_state(u.x, u.y, local) == w3_game::fog::FOG_MASKED {
                continue;
            }
            let dead_for = u.dead.then(|| w.time - u.death_time);
            wanted.push((h, code, u.x, u.y, dead_for, [1.0; 4]));
        }
        for (&h, o) in w.handles.iter() {
            if let w3_game::types::Obj::Ubersplat(s) = o {
                if s.shown && (s.render || s.render_always) && s.age.is_some() {
                    // CreateUbersplat's noBirthTime starts it past its birth.
                    if s.no_birth {
                        self.splat_births.entry(h).or_insert(f32::NEG_INFINITY);
                    }
                    wanted.push((h, s.name.clone(), s.x, s.y, None, s.color.map(|c| c as f32 / 255.0)));
                }
            }
        }
        let now = self.time;
        self.splat_births.retain(|h, _| wanted.iter().any(|x| x.0 == *h));
        let mut out = Vec::new();
        for (h, code, x, y, dead_for, tint) in wanted {
            if slk.rows.get(&code).is_none() {
                continue;
            }
            let born = *self.splat_births.entry(h).or_insert(now);
            let age = now - born;
            let (birth, decay) = (num(&code, "BirthTime").max(0.001), num(&code, "Decay").max(0.001));
            let (start, mid, end) = (color(&code, "Start"), color(&code, "Middle"), color(&code, "End"));
            let lerp = |a: [f32; 4], b: [f32; 4], t: f32| [0, 1, 2, 3].map(|i| a[i] + (b[i] - a[i]) * t.clamp(0.0, 1.0));
            let mut c = if age < birth { lerp(start, mid, age / birth) } else { mid };
            if let Some(d) = dead_for {
                if d >= decay {
                    continue;
                }
                c = lerp(mid, end, d / decay);
            }
            let c = [c[0] * tint[0], c[1] * tint[1], c[2] * tint[2], c[3] * tint[3]];
            let dir = slk.get(&code, "Dir").map(|v| v.as_str()).unwrap_or_default();
            let file = slk.get(&code, "file").map(|v| v.as_str()).unwrap_or_default();
            let texture = assets.textures.get_or_white(device, queue, &format!("{dir}\\{file}.blp"));
            let size = num(&code, "Scale") * 2.0;
            out.push(w3_render::decal::Decal { texture, center: [x, y, self.terrain.height_at(x, y)], size: [size, size], color: c, additive: num(&code, "BlendMode") != 0.0 });
        }
        out
    }

    /// Images (CreateImage) laid on the ground: those shown, rendered always
    /// (SetImageRenderAlways, flag 0x40) or rendered where not fogged (SetImageRender,
    /// 0x20); each spans its size from its position less its origin, in its color.
    fn image_decals(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, assets: &mut Assets) -> Vec<w3_render::decal::Decal> {
        let Some(g) = &self.game else { return vec![] };
        let w = &g.world;
        let mut out = Vec::new();
        for (_, o) in w.handles.iter() {
            let w3_game::types::Obj::Image(i) = o else { continue };
            let (x0, y0) = (i.pos[0] - i.origin[0], i.pos[1] - i.origin[1]);
            let (cx, cy) = (x0 + i.size[0] / 2.0, y0 + i.size[1] / 2.0);
            let seen = i.render && w.fog_state(cx, cy, w.local_player) == w3_game::fog::FOG_VISIBLE;
            if !i.shown || !(i.render_always || seen) {
                continue;
            }
            let texture = assets.textures.get_or_white(device, queue, &i.file);
            let z = i.constant_height.unwrap_or_else(|| self.terrain.height_at(cx, cy));
            out.push(w3_render::decal::Decal { texture, center: [cx, cy, z], size: [i.size[0], i.size[1]], color: i.color.map(|c| c as f32 / 255.0), additive: false });
        }
        out
    }

    /// Units' and items' shadow images on the ground (see crate::shadows).
    fn shadow_decals(&self, device: &wgpu::Device, queue: &wgpu::Queue, assets: &mut Assets) -> Vec<w3_render::decal::Decal> {
        let Some(g) = &self.game else { return vec![] };
        let w = &g.world;
        let color = crate::shadows::image_color(w);
        let mut out = Vec::new();
        let mut push = |name: &str, x: f32, y: f32, size: [f32; 2], at: [f32; 2]| {
            if name.is_empty() || name == "_" {
                return;
            }
            let texture = assets.textures.get_or_white(device, queue, &format!("ReplaceableTextures\\Shadows\\{name}.blp"));
            let (cx, cy) = (x - at[0] + size[0] / 2.0, y - at[1] + size[1] / 2.0);
            out.push(w3_render::decal::Decal { texture, center: [cx, cy, self.terrain.height_at(cx, cy)], size, color, additive: false });
        };
        for (&h, v) in &self.units {
            let Some(u) = w.unit(h) else { continue };
            if !v.inst.visible || u.dead || u.hidden {
                continue;
            }
            let Some(o) = w.objects.units.get(u.type_id) else { continue };
            push(&o.str("unitShadow"), u.x, u.y, [o.real("shadowW"), o.real("shadowH")], [o.real("shadowX"), o.real("shadowY")]);
        }
        let pair = |k: &str, d: f32| {
            let v: Vec<f32> = w.misc.get_list("Misc", k).iter().filter_map(|x| x.trim().parse().ok()).collect();
            [v.first().copied().unwrap_or(d), v.get(1).copied().unwrap_or(d)]
        };
        let (file, size, at) = (w.misc.get_str("Misc", "ItemShadowFile").unwrap_or_default(), pair("ItemShadowSize", 120.0), pair("ItemShadowOffset", 50.0));
        for (&h, inst) in &self.items {
            let Some(w3_game::types::Obj::Item(it)) = w.handles.get(h) else { continue };
            if inst.visible && it.visible && it.owner_unit == 0 {
                push(file.trim(), it.x, it.y, size, at);
            }
        }
        out
    }

    /// One of the eight indicator slots (CWorldFrameWar3 +0x378), replacing the oldest.
    fn add_indicator(&mut self, widget: u32, color: [f32; 4]) {
        self.indicators.retain(|i| i.0 != widget);
        if self.indicators.len() >= 8 {
            self.indicators.remove(0);
        }
        self.indicators.push((widget, color, self.time));
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
        // Indicators show 1.1 seconds (0x6fce5a04) as a circle the size of the unit's.
        for &(h, color, start) in &self.indicators {
            if self.time - start > 1.1 {
                continue;
            }
            let Some((x, y)) = w.widget_pos(h) else { continue };
            let scale = w.unit(h).and_then(|u| w.objects.units.get(u.type_id)).map_or(1.0, |o| o.real("scale"));
            let size = scale * num("ScaleFactor", 72.0);
            let Some(tex) = misc.get_str("SelectionCircle", "Texture00") else { continue };
            let texture = assets.textures.get_or_white(device, queue, &format!("{tex}.blp"));
            out.push(w3_render::decal::Decal { texture, center: [x, y, self.terrain.height_at(x, y)], size: [size, size], color, additive: false });
        }
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

    /// A structure awaiting its placement: its footprint's cells under the pointer,
    /// translucent green where it can be built and red where not (CPlacementBox,
    /// 0x4000f000 and 0x40f00000).
    fn placement_decals(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, assets: &mut Assets) -> Vec<w3_render::decal::Decal> {
        let Some(id) = self.placing else { return Vec::new() };
        let Some((x, y)) = self.ground_at(self.cursor.0, self.cursor.1, self.screen) else { return Vec::new() };
        let Some(g) = self.game.as_mut() else { return Vec::new() };
        let (x, y) = g.world.snap_placement(x, y);
        let cells = g.world.placement_cells(id, x, y);
        let texture = assets.textures.get_or_white(device, queue, "");
        let argb = |c: u32| [((c >> 16) & 0xff) as f32 / 255.0, ((c >> 8) & 0xff) as f32 / 255.0, (c & 0xff) as f32 / 255.0, (c >> 24) as f32 / 255.0];
        cells
            .into_iter()
            .map(|((cx, cy), ok)| w3_render::decal::Decal {
                texture: texture.clone(),
                center: [cx, cy, self.terrain.height_at(cx, cy)],
                size: [32.0, 32.0],
                color: argb(if ok { 0x4000f000 } else { 0x40f00000 }),
                additive: false,
            })
            .collect()
    }

    /// While a spell with an area awaits its target, the race's PlacementCursor
    /// (SpellAreaOfEffect) on the ground under the pointer, as wide as the area.
    fn area_cursor_decal(&self, device: &wgpu::Device, queue: &wgpu::Queue, assets: &mut Assets) -> Option<w3_render::decal::Decal> {
        let order = self.targeting?;
        let g = self.game.as_ref()?;
        let &h = self.selection.first()?;
        let a = g.world.ability_for_order(h, order)?;
        let lvl = g.world.ability_level(h, a).max(1) as u32;
        let area = g.world.data(a, "Area", lvl);
        if area <= 0.0 {
            return None;
        }
        let (x, y) = self.ground_at(self.cursor.0, self.cursor.1, self.screen)?;
        let tex = assets.frames.skin_value("PlacementCursor")?;
        let texture = assets.textures.get_or_white(device, queue, tex.trim_matches('"'));
        Some(w3_render::decal::Decal { texture, center: [x, y, self.terrain.height_at(x, y)], size: [area * 2.0, area * 2.0], color: [1.0; 4], additive: false })
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

    /// Health bars over units (CPreselectUI's CStatBar): the unit under the pointer's,
    /// or every visible unit's while Alt is held or the Always Show Health Bars option
    /// is on.  A bar is UI\MiscData.txt [SelectionCircle] ScaleFactor times the unit's
    /// scale times 0.0005 wide and 0.004 tall, on black 0.001 around it, its fill
    /// (SimpleHpBarConsoleSmall) colored by life: green to yellow above 60%, then red
    /// with less green (FUN_6f3daba0).  It sits on top of the unit's stand sequence's
    /// extent (measured from the original).
    fn health_bar_draws(&self, db: &w3_ui::FrameDb, size: (u32, u32)) -> Vec<w3_ui::Draw> {
        let Some(g) = self.game.as_ref() else { return Vec::new() };
        let w = &g.world;
        if self.hud.as_ref().is_some_and(|h| h.cinematic) {
            return Vec::new();
        }
        let all = self.alt_held || self.options.healthbars;
        let hovered = if all { None } else { self.pick_unit(self.cursor.0, self.cursor.1, size) };
        if !all && hovered.is_none() {
            return Vec::new();
        }
        let width = w3_ui::UI_HEIGHT * (size.0 as f32 / size.1.max(1) as f32).max(4.0 / 3.0);
        let factor = w.misc.get_str("SelectionCircle", "ScaleFactor").and_then(|v| v.trim().parse().ok()).unwrap_or(72.0) * 0.0005;
        let fill = db.decorate("SimpleHpBarConsoleSmall");
        let mut out = Vec::new();
        for (&h, v) in &self.units {
            if hovered.is_some_and(|x| x != h) {
                continue;
            }
            let Some(u) = w.unit(h) else { continue };
            if !v.inst.visible || u.dead || u.hidden || w.is_locust(u) {
                continue;
            }
            let Some(o) = w.objects.units.get(u.type_id) else { continue };
            let mdx = &v.inst.model.mdx;
            let top = w3_render::anim::sequences_named(mdx, "stand").first().and_then(|&s| mdx.sequences.get(s)).map_or(mdx.extent.max[2], |s| s.extent.max[2]);
            let p = v.inst.transform.transform_point3(Vec3::new(0.0, 0.0, top));
            let Some((px, py)) = self.project(p, size) else { continue };
            let (cx, bottom) = (px / size.0 as f32 * width, (1.0 - py / size.1 as f32) * w3_ui::UI_HEIGHT);
            let (bw, bh, border) = (factor * o.real("scale"), 0.004, 0.001);
            let y0 = bottom + border;
            let rect = w3_ui::Rect { x0: cx - bw / 2.0, x1: cx + bw / 2.0, y0, y1: y0 + bh };
            let black = w3_ui::Rect { x0: rect.x0 - border, x1: rect.x1 + border, y0: rect.y0 - border, y1: rect.y1 + border };
            out.push(w3_ui::Draw::Image { texture: String::new(), rect: black, uv: [0.0, 0.0, 1.0, 1.0], color: [0.0, 0.0, 0.0, 1.0], blend: w3_ui::Blend::Alpha, rotated: false });
            let f = (u.life / u.max_life.max(1.0)).clamp(0.0, 1.0);
            let color = if f > 0.6 {
                [(255.0 - (f - 0.5) * 255.0 * 2.0).clamp(0.0, 255.0) / 255.0, 1.0, 0.0, 1.0]
            } else {
                let d = if f > 0.3 { 0.6 } else { 0.8 };
                [1.0, (f / d).min(1.0), 0.0, 1.0]
            };
            let filled = w3_ui::Rect { x1: rect.x0 + bw * f, ..rect };
            out.push(w3_ui::Draw::Image { texture: fill.clone(), rect: filled, uv: [0.0, 0.0, f, 1.0], color, blend: w3_ui::Blend::Alpha, rotated: false });
        }
        out
    }

    /// Text tags (CTextTagManager, 0x6f9a1040): each at its projected world position plus
    /// velocity times age, left-aligned, with a black shadow, fading after the fade point.
    /// Their units span the 4:3 screen as 1 by 0.75, so one is 0.8 interface units
    /// (measured against the original's gold text).
    fn text_tag_draws(&self, db: &w3_ui::FrameDb, size: (u32, u32)) -> Vec<w3_ui::Draw> {
        let Some(g) = self.game.as_ref() else { return Vec::new() };
        let w = &g.world;
        if self.hud.as_ref().is_some_and(|h| h.cinematic) {
            return Vec::new();
        }
        let width = w3_ui::UI_HEIGHT * (size.0 as f32 / size.1.max(1) as f32).max(4.0 / 3.0);
        let font = db.decorate("MasterFont");
        let mut out = Vec::new();
        for (_, o) in w.handles.iter() {
            let w3_game::types::Obj::TextTag(t) = o else { continue };
            if !t.visible || t.text.is_empty() {
                continue;
            }
            let (x, y, z) = match t.unit {
                Some((u, dz)) => match w.unit(u) {
                    Some(u) if !u.hidden => (u.x, u.y, w.ground_z(u.x, u.y) + dz),
                    _ => continue,
                },
                None => (t.x, t.y, t.z),
            };
            let Some((px, py)) = self.project(Vec3::new(x, y, z), size) else { continue };
            const UNIT: f32 = 0.8;
            let ux = px / size.0 as f32 * width + t.velocity.0 * t.age * UNIT;
            let uy = (1.0 - py / size.1 as f32) * w3_ui::UI_HEIGHT + t.velocity.1 * t.age * UNIT;
            let mut alpha = t.color[3] as f32 / 255.0;
            if !t.permanent && t.age > t.fadepoint && t.lifespan > t.fadepoint {
                alpha *= (1.0 - (t.age - t.fadepoint) / (t.lifespan - t.fadepoint)).clamp(0.0, 1.0);
            }
            let em = t.height * UNIT;
            let color = [t.color[0] as f32 / 255.0, t.color[1] as f32 / 255.0, t.color[2] as f32 / 255.0, alpha];
            let tw = db.text_width(&font, em, &t.text);
            let rect = w3_ui::Rect { x0: ux, x1: ux + tw + em, y0: uy, y1: uy + em * 1.2 };
            out.push(w3_ui::Draw::Text { font: font.clone(), size: em, rect, text: t.text.clone(), color, justify_h: w3_ui::Justify::Start, justify_v: w3_ui::Justify::End, shadow: Some(([0.0, 0.0, 0.0, alpha], 0.0015 * UNIT, -0.0015 * UNIT)) });
        }
        out
    }

    pub fn project(&self, p: Vec3, size: (u32, u32)) -> Option<(f32, f32)> {
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
            if u.hidden || (u.dead && !u.is_building) || w.is_locust(u) {
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

    /// Whether the player may act: EnableUserControl and EnableUserUI both allow it
    /// (FUN_6f376f60); cinematics and fades take it away.
    pub fn user_control(&self) -> bool {
        self.game.as_ref().is_none_or(|g| g.world.cinematic.user_control && g.world.cinematic.user_ui)
    }

    /// Selects with a click or a drag rectangle, the way the local player would.
    pub fn select(&mut self, a: (f32, f32), b: (f32, f32), size: (u32, u32), add: bool) {
        if !self.user_control() {
            return;
        }
        let Some(g) = self.game.as_ref() else { return };
        // EnableSelect and EnableDragSelect.
        let dragged = (a.0 - b.0).abs() >= 4.0 || (a.1 - b.1).abs() >= 4.0;
        if !g.world.select_allowed.0 || dragged && !g.world.select_allowed.1 {
            return;
        }
        let local = g.world.local_player;
        let mut picked: Vec<u32> = Vec::new();
        if (a.0 - b.0).abs() < 4.0 && (a.1 - b.1).abs() < 4.0 {
            if let Some(h) = self.pick_unit(b.0, b.1, size) {
                picked.push(h);
                // Ctrl or a second click on the same unit: the player's units of its
                // type on the screen.
                let double = self.last_pick.is_some_and(|(p, t)| p == h && self.time - t < 0.4);
                self.last_pick = Some((h, self.time));
                let w = &g.world;
                if let Some(t) = w.unit(h).filter(|_| w3_game::natives::players::controls(w, local, h) && (self.ctrl_held || double)).map(|u| u.type_id) {
                    for &o in &w.unit_order {
                        if o == h || picked.len() >= 12 {
                            continue;
                        }
                        let Some(u) = w.unit(o).filter(|u| w3_game::natives::players::controls(w, local, o) && u.type_id == t && !u.dead && !u.hidden) else { continue };
                        let z = self.terrain.height_at(u.x, u.y) + u.fly_height;
                        if self.project(Vec3::new(u.x, u.y, z), size).is_some_and(|(px, py)| px >= 0.0 && py >= 0.0 && px <= size.0 as f32 && py <= size.1 as f32) {
                            picked.push(o);
                        }
                    }
                }
            }
        } else {
            let (x0, x1) = (a.0.min(b.0), a.0.max(b.0));
            let (y0, y1) = (a.1.min(b.1), a.1.max(b.1));
            let w = &g.world;
            for &h in &w.unit_order {
                let Some(u) = w.unit(h) else { continue };
                if !w3_game::natives::players::controls(w, local, h) || u.dead || u.hidden || w.is_locust(u) {
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
        let mine: Vec<u32> = self.selection.iter().copied().filter(|&h| w3_game::natives::players::controls(&g.world, local, h)).collect();
        // Shops need not be the local player's own.
        let first_any = self.selection.first().copied().unwrap_or(0);
        if matches!(action, A::Buy(_) | A::Hire(_) | A::Awaken(_)) {
            match action {
                A::Buy(id) => {
                    if let Err(e) = g.apply(w3_game::replay::Command::BuyItem { player: local, shop: first_any, id }).ok() {
                        self.error(e);
                    }
                }
                A::Hire(id) => {
                    if let Err(e) = g.apply(w3_game::replay::Command::HireUnit { player: local, shop: first_any, id }).ok() {
                        self.error(e);
                    }
                }
                A::Awaken(hero) => {
                    if let Err(e) = g.apply(w3_game::replay::Command::Awaken { player: local, tavern: first_any, hero }).ok() {
                        self.error(e);
                    }
                }
                _ => {}
            }
            return;
        }
        let Some(&first) = mine.first() else { return };
        match action {
            A::Order { order, targeted: true } if !order.is_empty() => self.targeting = Some(w3_game::natives::basic::order_id(order)),
            A::OrderId { order, targeted: true } => self.targeting = Some(order),
            A::Order { order, targeted: false } if !order.is_empty() => {
                let id = w3_game::natives::basic::order_id(order);
                for h in mine {
                    g.apply(w3_game::replay::Command::Order { unit: h, order: id, x: 0.0, y: 0.0, target: 0 });
                }
                self.order_ack();
            }
            A::OrderId { order, targeted: false } => {
                for h in mine {
                    g.apply(w3_game::replay::Command::Order { unit: h, order, x: 0.0, y: 0.0, target: 0 });
                }
                self.order_ack();
            }
            A::Order { .. } => {}
            A::Train(id) => {
                if let Err(e) = g.apply(w3_game::replay::Command::Train { building: first, id }).ok() {
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
                g.apply(w3_game::replay::Command::LearnSkill { hero: first, id });
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
            A::CancelTrain => {
                g.apply(w3_game::replay::Command::CancelLast { building: first });
            }
            A::Buy(id) => {
                let local = g.world.local_player;
                if let Err(e) = g.apply(w3_game::replay::Command::BuyItem { player: local, shop: first_any, id }).ok() {
                    self.error(e);
                }
            }
            A::Hire(id) => {
                let local = g.world.local_player;
                if let Err(e) = g.apply(w3_game::replay::Command::HireUnit { player: local, shop: first_any, id }).ok() {
                    self.error(e);
                }
            }
            A::Revive(hero) => {
                if let Err(e) = g.apply(w3_game::replay::Command::StartRevive { altar: first, hero }).ok() {
                    self.error(e);
                }
            }
            // Taken with the shop actions above.
            A::Awaken(_) => {}
            A::Research(id) => {
                if let Err(e) = g.apply(w3_game::replay::Command::Research { building: first, id }).ok() {
                    self.error(e);
                }
            }
            A::Upgrade(id) => {
                if let Err(e) = g.apply(w3_game::replay::Command::Upgrade { building: first, id }).ok() {
                    self.error(e);
                }
            }
        }
    }

    /// Handles a left click: interface first, then targeting, placement, or selection start.
    /// Returns true when the click was consumed and should not start a selection drag.
    pub fn left_click(&mut self, sx: f32, sy: f32, size: (u32, u32)) -> bool {
        let (ux, uy) = self.to_ui(sx, sy, size);
        if !self.user_control() && self.hud.as_ref().is_none_or(|h| h.menu.is_none() && h.dialog.is_none()) {
            return true;
        }
        // A network game's waiting dialog: drop the players waited on, or leave.
        if let Some(d) = self.hud.as_mut().and_then(|h| h.net_dialog.as_mut()) {
            match d.click(ux, uy) {
                Some(crate::netdialog::Event::Drop) => {
                    if let Some(s) = self.net.as_mut() {
                        s.drop_lagging();
                    }
                }
                Some(crate::netdialog::Event::Disconnect) => {
                    self.ended = true;
                    self.final_score = self.game.as_ref().map(|g| crate::scorescreen::ScoreData::from_world(&g.world));
                }
                None => {}
            }
            return true;
        }
        // The chat dialog takes every click while it is open; OK keeps its recipient.
        if let Some(d) = self.hud.as_mut().and_then(|h| h.chat_dialog.as_mut()) {
            if let Some(r) = d.click(ux, uy) {
                self.chat_default = Some(r);
                self.hud.as_mut().unwrap().chat_dialog = None;
            }
            return true;
        }
        // The Allies dialog takes every click while it is open.
        if let Some(d) = self.hud.as_mut().and_then(|h| h.alliance.as_mut()) {
            let shift = self.shift_held;
            match d.click(ux, uy, true, shift) {
                Some(crate::alliance::Event::Accept(sends, changes)) => {
                    if let Some(g) = self.game.as_mut() {
                        let local = g.world.local_player;
                        for (other, flags) in changes {
                            g.apply(w3_game::replay::Command::Alliance { player: local, other, flags });
                        }
                        for (other, gold, lumber) in sends {
                            g.apply(w3_game::replay::Command::Trade { player: local, other, gold, lumber });
                        }
                    }
                    self.hud.as_mut().unwrap().alliance = None;
                }
                Some(crate::alliance::Event::Cancel) => self.hud.as_mut().unwrap().alliance = None,
                None => {}
            }
            return true;
        }
        // The game menu takes every click while it is open.
        if let Some(p) = self.hud.as_mut().and_then(|h| h.save_panel.as_mut()) {
            if p.click(ux, uy) {
                return true;
            }
        }
        self.screen = size;
        if let Some(m) = self.hud.as_mut().and_then(|h| h.menu.as_mut()) {
            // A slider follows the pointer until it is let go.
            if m.pointer_down(ux, uy).is_some_and(|i| m.frames[i].def.kind == "SLIDER") {
                self.menu_drag = true;
                return true;
            }
            if let Some(i) = m.pointer_up(ux, uy) {
                let name = m.frames[i].name.clone();
                self.menu_button(&name);
            }
            return true;
        }
        // The message log takes every click while it is open; OK closes it.
        if let Some(h) = self.hud.as_mut() {
            if let Some(l) = h.log_view.as_mut() {
                if l.click(ux, uy) {
                    h.log_view = None;
                }
                return true;
            }
        }
        // The quest log takes every click while it is open; Accept closes it.
        if let Some(h) = self.hud.as_mut() {
            if let Some(q) = h.quests.as_mut() {
                if q.click(ux, uy) {
                    h.quests = None;
                }
                return true;
            }
        }
        // The upper buttons: Menu opens the game menu, Quests the quest log.
        if let Some(h) = self.hud.as_mut() {
            if let Some(i) = h.tree.control_at(ux, uy) {
                if h.tree.frames[i].name == "UpperButtonBarMenuButton" {
                    self.open_menu(size);
                    return true;
                }
                if h.tree.frames[i].name == "UpperButtonBarAlliesButton" && h.tree.frames[i].enabled {
                    if h.alliance.take().is_none() {
                        self.alliance_requested = true;
                    }
                    return true;
                }
                if h.tree.frames[i].name == "UpperButtonBarQuestsButton" && h.tree.frames[i].enabled {
                    self.open_quests();
                    return true;
                }
                if h.tree.frames[i].name == "UpperButtonBarChatButton" && h.tree.frames[i].enabled {
                    self.open_log();
                    return true;
                }
            }
        }
        // A dialog takes every click while it is up.
        if let Some(d) = self.hud.as_mut().and_then(|h| h.dialog.as_mut()) {
            d.tree.pointer_down(ux, uy);
            if let Some(i) = d.tree.pointer_up(ux, uy) {
                let button = d.buttons.iter().find(|b| b.0 == i).map(|b| b.1);
                if let (Some(b), Some(g)) = (button, self.game.as_mut()) {
                    let local = g.world.local_player;
                    g.apply(w3_game::replay::Command::DialogButton { player: local, button: b });
                }
            }
            return true;
        }
        // The replay panel's buttons, vision menu, and check boxes.
        if let Some(h) = self.hud.as_mut() {
            if let Some(r) = h.replay.as_mut() {
                let mut event = None;
                if r.click(&mut h.tree, ux, uy, &mut event) {
                    if let Some(crate::replaypanel::Event::Restart) = event {
                        self.restart_requested = true;
                    }
                    if let (Some(g), Some(r)) = (self.game.as_mut(), self.hud.as_mut().and_then(|h| h.replay.as_mut())) {
                        // The view changes at once.
                        r.enter(&mut g.world);
                        r.leave(&mut g.world);
                    }
                    return true;
                }
            }
        }
        // The multiboard's minimize button.
        if let Some(b) = self.hud.as_ref().and_then(|h| h.boards.as_ref()).and_then(|b| b.minimize_at(ux, uy)) {
            if let Some(w3_game::types::Obj::Multiboard(m)) = self.game.as_mut().and_then(|g| g.world.handles.get_mut(b)) {
                m.minimized = !m.minimized;
            }
            return true;
        }
        // Clicking an inventory item uses it.
        if let Some(k) = self.hud.as_ref().and_then(|h| h.minimap_button_at(ux, uy)) {
            self.ui_sound("InterfaceClick");
            // The signal button waits for the place to signal (CSignalMode).
            if k == 0 {
                self.signaling = true;
                return true;
            }
            if let Some(h) = self.hud.as_mut() {
                h.press_minimap_button(k);
            }
            return true;
        }
        // A signal, chosen on the minimap or the ground (or Alt+click), goes to allies
        // with InterfaceClick (FUN_6f3c4de0).
        if std::mem::take(&mut self.signaling) || self.alt_held {
            let at = self.hud.as_ref().and_then(|h| h.minimap_to_world(ux, uy)).or_else(|| if self.over_console(sx, sy, size) { None } else { self.ground_at(sx, sy, size) });
            if let (Some((x, y)), Some(g)) = (at, self.game.as_mut()) {
                let local = g.world.local_player;
                g.apply(w3_game::replay::Command::Signal { player: local, x, y });
                self.ui_sound("InterfaceClick");
                return true;
            }
        }
        // The idle worker button selects an idle worker and looks at it.
        let now = self.time;
        if let Some(worker) = self.hud.as_mut().and_then(|h| h.pick_idle_worker(ux, uy, now)) {
            self.select_worker(worker);
            return true;
        }
        // A cargo slot unloads the unit in it.
        if let Some(c) = self.hud.as_ref().and_then(|h| h.cargo_at(ux, uy)) {
            if let (Some(&t), Some(g)) = (self.selection.first(), self.game.as_mut()) {
                g.apply(w3_game::replay::Command::Order { unit: t, order: w3_game::natives::basic::order_id("unload"), x: 0.0, y: 0.0, target: c });
            }
            return true;
        }
        // A hero bar button selects the hero.
        if let Some(hero) = self.hud.as_ref().and_then(|h| h.hero_at(ux, uy)) {
            self.selection = vec![hero];
            return true;
        }
        // A group button selects its unit alone.
        if let Some(unit) = self.hud.as_ref().and_then(|h| h.group_at(ux, uy)) {
            self.selection = vec![unit];
            return true;
        }
        // A held item goes to the slot clicked.
        if let Some((u, from)) = self.held_item {
            if let Some(slot) = self.hud.as_ref().and_then(|h| h.inventory_at(ux, uy)) {
                self.held_item = None;
                if let Some(g) = self.game.as_mut() {
                    g.apply(w3_game::replay::Command::MoveItem { unit: u, from, to: slot });
                }
                return true;
            }
        }
        if let Some(slot) = self.hud.as_ref().and_then(|h| h.inventory_at(ux, uy)) {
            if let (Some(&u), Some(g)) = (self.selection.first(), self.game.as_mut()) {
                if w3_game::natives::players::controls(&g.world, g.world.local_player, u) {
                    // An item used on a unit or a point waits for the click on it.
                    if g.world.item_targeting(u, slot) == w3_game::ability::Targeting::None {
                        g.apply(w3_game::replay::Command::UseItem { unit: u, slot });
                    } else {
                        self.item_target = Some((u, slot));
                    }
                }
            }
            return true;
        }
        if let Some(b) = self.hud.as_ref().and_then(|h| h.button_at(ux, uy)).cloned() {
            if b.enabled {
                self.ui_sound("InterfaceClick");
                self.card_action(b.action);
            }
            return true;
        }
        if let Some((wx, wy)) = self.hud.as_ref().and_then(|h| h.minimap_to_world(ux, uy)) {
            if let Some(g) = self.game.as_mut() {
                let (wx, wy) = g.world.clamp_to_camera_bounds(wx, wy);
                g.world.camera.pan_to(wx, wy, 0.0);
            }
            self.sync_camera();
            return true;
        }
        if self.over_console(sx, sy, size) {
            return true;
        }
        // A held item is handed to the unit clicked, or dropped where the ground is.
        if let Some((u, slot)) = self.held_item.take() {
            let target = self.pick_unit(sx, sy, size).filter(|&t| t != u);
            let point = self.ground_at(sx, sy, size);
            if let Some(g) = self.game.as_mut() {
                match (target, point) {
                    (Some(t), _) => {
                        g.apply(w3_game::replay::Command::GiveItemTo { unit: u, slot, target: t });
                    }
                    (None, Some((x, y))) => {
                        g.apply(w3_game::replay::Command::DropItemAt { unit: u, slot, x, y });
                    }
                    _ => {}
                }
            }
            return true;
        }
        if let Some((u, slot)) = self.item_target.take() {
            let target = self.pick_unit(sx, sy, size).or_else(|| self.pick_destructable(sx, sy, size));
            let point = self.ground_at(sx, sy, size);
            if let Some(g) = self.game.as_mut() {
                let point_only = g.world.item_targeting(u, slot) == w3_game::ability::Targeting::Point;
                let (t, (x, y)) = if point_only { (0, point.unwrap_or_default()) } else { (target.unwrap_or(0), point.unwrap_or_default()) };
                g.apply(w3_game::replay::Command::UseItemAt { unit: u, slot, target: t, x, y });
            }
            return true;
        }
        if let Some(order) = self.targeting.take() {
            let target = self.pick_unit(sx, sy, size).or_else(|| self.pick_destructable(sx, sy, size));
            let point = self.ground_at(sx, sy, size);
            let (mut ordered, mut error) = (false, None);
            if let Some(g) = self.game.as_mut() {
                let id = order;
                for &h in &self.selection {
                    if !w3_game::natives::players::controls(&g.world, g.world.local_player, h) {
                        continue;
                    }
                    g.world.order_error = None;
                    match (target, point) {
                        (Some(t), _) => {
                            g.apply(w3_game::replay::Command::Issue { unit: h, order: id, x: 0.0, y: 0.0, target: t, queue: self.shift_held });
                        }
                        (None, Some((x, y))) => {
                            g.apply(w3_game::replay::Command::Issue { unit: h, order: id, x, y, target: 0, queue: self.shift_held });
                        }
                        _ => {}
                    }
                    match g.world.order_error.take() {
                        Some(e) => error = Some(e),
                        None => ordered = true,
                    }
                }
            }
            // A target no selected unit may take is refused with its error, and the
            // targeting goes on.
            if let (false, Some(e)) = (ordered, error) {
                self.targeting = Some(order);
                self.error(e);
                return true;
            }
            self.order_ack();
            return true;
        }
        if let Some(id) = self.placing.take() {
            let Some((x, y)) = self.ground_at(sx, sy, size) else { return true };
            if let Some(g) = self.game.as_mut() {
                let (x, y) = g.world.snap_placement(x, y);
                if let Some(&worker) = self.selection.first() {
                    g.apply(w3_game::replay::Command::Build { worker, id, x, y });
                }
            }
            return true;
        }
        false
    }

    /// A digit key: with Ctrl the selection becomes control group `n`, with Shift it
    /// joins it; alone the group (its living units) is selected, and the camera goes
    /// to it when it is chosen again within half a second.
    pub fn control_group(&mut self, n: usize, ctrl: bool, shift: bool) {
        if n >= 10 || !self.user_control() {
            return;
        }
        if ctrl {
            self.groups[n] = self.selection.clone();
            return;
        }
        if shift {
            for &h in &self.selection.clone() {
                if !self.groups[n].contains(&h) {
                    self.groups[n].push(h);
                }
            }
            return;
        }
        let Some(g) = self.game.as_ref() else { return };
        self.groups[n].retain(|&h| g.world.unit(h).is_some_and(|u| !u.dead));
        if self.groups[n].is_empty() {
            return;
        }
        let again = self.last_group.is_some_and(|(k, t)| k == n && self.time - t < 0.5);
        self.last_group = Some((n, self.time));
        self.selection = self.groups[n].clone();
        if again {
            if let Some(g) = self.game.as_mut() {
                if let Some((x, y)) = self.selection.first().and_then(|&h| g.world.unit(h)).map(|u| (u.x, u.y)) {
                    g.world.camera.pan_to(x, y, 0.0);
                }
            }
        }
    }

    fn select_worker(&mut self, worker: u32) {
        self.selection = vec![worker];
        if let Some(g) = self.game.as_mut() {
            if let Some((x, y)) = g.world.unit(worker).map(|u| (u.x, u.y)) {
                g.world.camera.pan_to(x, y, 0.0);
            }
        }
    }

    /// F1 to F3: selects the hero on that hero bar button, or looks at it when it is
    /// the selection already.
    pub fn hero_key(&mut self, k: usize) {
        let Some(hero) = self.hud.as_ref().and_then(|h| h.heroes.get(k)).map(|e| e.0) else { return };
        if self.selection == [hero] {
            self.select_worker(hero);
        } else {
            self.selection = vec![hero];
        }
    }

    /// The idle worker button's hotkey (F8, as IDLE_PEON shows it).
    pub fn idle_worker_key(&mut self) {
        let now = self.time;
        if let Some(worker) = self.hud.as_mut().and_then(|h| h.next_idle_worker(now)) {
            self.select_worker(worker);
        }
    }

    /// Opens the game menu (F10) on the next frame.
    pub fn open_menu(&mut self, _size: (u32, u32)) {
        self.menu_requested = true;
    }

    /// Opens the message log (F12) on the next frame; in a network game the chat dialog.
    pub fn open_log(&mut self) {
        if self.hud.as_ref().is_some_and(|h| h.menu.is_none()) {
            if self.game.as_ref().is_some_and(|g| g.world.network) {
                self.chat_dialog_requested = true;
            } else {
                self.log_requested = true;
            }
        }
    }

    /// The chat bar's slash commands, which stay on this computer (FUN_6f371e80):
    /// /mapname, /squelch or /ignore a player by name (SQUELCH_PLAYER_NOTIFY),
    /// /unsquelch or /unignore (UNSQUELCH_PLAYER_NOTIFY), and /showminimapsignals and
    /// /hideminimapsignals.  Returns whether the line was one.
    fn slash_command(&mut self, line: &str) -> bool {
        let Some(rest) = line.strip_prefix('/') else { return false };
        let starts = |word: &str| rest.len() >= word.len() && rest[..word.len()].eq_ignore_ascii_case(word);
        let Some(g) = self.game.as_ref() else { return false };
        let w = &g.world;
        let player_named = |name: &str| (0..12u8).find(|&p| w.players[p as usize].name.eq_ignore_ascii_case(name.trim()));
        let colored = |p: u8| {
            let c = self.team_rgb[w.players[p as usize].color as usize % 16].map(|v| (v * 255.0).round() as u8);
            format!("|cff{:02x}{:02x}{:02x}{}|r", c[0], c[1], c[2], w.players[p as usize].name)
        };
        let message = if rest.eq_ignore_ascii_case("mapname") {
            let name = w.localized(&self.info.name);
            Some(if name.is_empty() { "Unknown map name!".to_string() } else { name })
        } else if let Some(word) = ["squelch", "ignore", "unsquelch", "unignore"].into_iter().find(|w| starts(w) && rest[w.len()..].starts_with(' ')) {
            let on = !word.starts_with("un");
            let Some(p) = player_named(&rest[word.len()..]) else { return true };
            let Some(h) = self.hud.as_mut() else { return true };
            if h.squelched.contains(&p) == on {
                return true;
            }
            if on {
                h.squelched.push(p);
            } else {
                h.squelched.retain(|&q| q != p);
            }
            let key = if on { "SQUELCH_PLAYER_NOTIFY" } else { "UNSQUELCH_PLAYER_NOTIFY" };
            Some(w.localized(key).replace("%s", &colored(p)))
        } else if rest.eq_ignore_ascii_case("showminimapsignals") || rest.eq_ignore_ascii_case("hideminimapsignals") {
            let show = rest.eq_ignore_ascii_case("showminimapsignals");
            if let Some(h) = self.hud.as_mut() {
                h.hide_signals = !show;
            }
            Some(if show { "Showing incoming minimap signals" } else { "Hiding incoming minimap signals" }.to_string())
        } else {
            return false;
        };
        if let (Some(text), Some(h)) = (message, self.hud.as_mut()) {
            h.unit_message(&text, 0.0, 0.0, 10.0);
        }
        true
    }

    /// Whom Enter's chat goes to: the chat dialog's choice, which starts as allies when
    /// the player has any and everyone otherwise (CChatDialog's constructor).
    fn chat_recipient(&self) -> u8 {
        self.chat_default.unwrap_or_else(|| {
            let Some(w) = self.game.as_ref().map(|g| &g.world) else { return 0 };
            let me = w.local_player;
            let allied = (0..12u8).any(|p| p != me && w.players[p as usize].slot_state == w3_game::types::slot_state::PLAYING && w3_game::natives::players::is_ally(w, me, p));
            if allied { 1 } else { 0 }
        })
    }

    /// Opens the quest log (F9) on the next frame, when the map has quests.
    pub fn open_quests(&mut self) {
        if self.hud.as_ref().is_some_and(|h| h.menu.is_none()) && self.game.as_ref().is_some_and(|g| !g.world.quests.is_empty()) {
            self.quests_requested = true;
        }
    }

    /// The game menu's buttons (CEscMenuMainPanel).
    fn menu_button(&mut self, name: &str) {
        let Some(h) = self.hud.as_mut() else { return };
        let Some(m) = h.menu.as_mut() else { return };
        match name {
            "ReturnButton" => h.menu = None,
            "EndGameButton" => crate::hud::Hud::menu_panel(m, "EndGamePanel"),
            "PreviousButton" => crate::hud::Hud::menu_panel(m, "MainPanel"),
            "QuitButton" => {
                h.menu = None;
                self.ended = true;
            }
            "ExitButton" => crate::hud::Hud::menu_panel(m, "ConfirmQuitPanel"),
            "ConfirmQuitCancelButton" => crate::hud::Hud::menu_panel(m, "EndGamePanel"),
            "ConfirmQuitQuitButton" => self.exit = true,
            "HelpButton" => {
                crate::hud::Hud::menu_panel(m, "HelpPanel");
                m.set_text("HelpTextArea", &self.help_text);
                m.set_selected("HelpTextArea", 0);
            }
            "HelpOKButton" | "TipsOKButton" => crate::hud::Hud::menu_panel(m, "MainPanel"),
            "OptionsButton" => {
                crate::hud::Hud::menu_panel(m, "EscMenuOptionsPanel");
                self.options_page = "OptionsPanel";
                crate::options::panel::show(m, "OptionsPanel");
            }
            "OptionsPreviousButton" => crate::hud::Hud::menu_panel(m, "MainPanel"),
            "GameplayButton" | "VideoButton" | "SoundButton" => {
                self.options_page = match name {
                    "GameplayButton" => "GameplayPanel",
                    "VideoButton" => "VideoPanel",
                    _ => "SoundPanel",
                };
                self.options_fill = true;
                crate::options::panel::show(m, self.options_page);
            }
            "OptionsOKButton" => {
                let speed = crate::options::panel::read(m, self.options_page, &mut self.options);
                if let (Some(s), Some(g)) = (speed, self.game.as_mut()) {
                    g.world.game_speed = s.min(2) as i32;
                }
                self.options.store();
                self.options_page = "OptionsPanel";
                crate::options::panel::show(m, "OptionsPanel");
            }
            "OptionsCancelButton" => {
                self.options_page = "OptionsPanel";
                crate::options::panel::show(m, "OptionsPanel");
            }
            "SoundCheckBox" | "MusicCheckBox" | "MouseScrollDisable" => crate::options::panel::sync(m),
            "TipsButton" | "TipsBackButton" | "TipsNextButton" => {
                let n = self.tips.len().max(1);
                self.tip = match name {
                    "TipsNextButton" => (self.tip + 1) % n,
                    "TipsBackButton" => (self.tip + n - 1) % n,
                    _ => self.tip,
                };
                crate::hud::Hud::menu_panel(m, "TipsPanel");
                m.set_text("TipsTextArea", self.tips.get(self.tip).map_or("", |s| s.as_str()));
                m.set_selected("TipsTextArea", 0);
            }
            // DisableRestartMission takes Restart away.
            "RestartButton" if !self.game.as_ref().is_some_and(|g| g.world.restart_disabled) => {
                h.menu = None;
                self.restart_requested = true;
            }
            "SaveGameButton" | "LoadGameButton" => {
                crate::hud::Hud::menu_panel(m, "EscMenuSaveGamePanel");
                h.save_panel = Some(crate::saveload::SavePanel::new(name == "LoadGameButton", self.saves_dir.clone()));
            }
            "SaveGameCancelButton" | "LoadGameCancelButton" => {
                h.save_panel = None;
                crate::hud::Hud::menu_panel(m, "MainPanel");
            }
            "SaveGameSaveButton" => {
                let Some(p) = h.save_panel.as_mut() else { return };
                let Some(path) = p.target() else { return };
                if path.exists() {
                    p.confirm = Some(crate::saveload::Confirm::Overwrite(path));
                } else {
                    self.write_save(&path);
                }
            }
            "OverwriteOverwriteButton" => {
                if let Some(crate::saveload::Confirm::Overwrite(path)) = h.save_panel.as_mut().and_then(|p| p.confirm.take()) {
                    self.write_save(&path);
                }
            }
            "SaveGameDeleteButton" => {
                if let Some(p) = h.save_panel.as_mut() {
                    p.confirm = p.selected_path().map(crate::saveload::Confirm::Delete);
                }
            }
            "DeleteDeleteButton" => {
                if let Some(p) = h.save_panel.as_mut() {
                    if let Some(crate::saveload::Confirm::Delete(path)) = p.confirm.take() {
                        if let Err(e) = std::fs::remove_file(&path) {
                            log::error!("deleting {}: {e}", path.display());
                        }
                        p.refresh();
                    }
                }
            }
            "OverwriteCancelButton" | "DeleteCancelButton" => {
                if let Some(p) = h.save_panel.as_mut() {
                    p.confirm = None;
                }
            }
            "LoadGameLoadButton" => {
                if let Some(path) = h.save_panel.as_ref().and_then(|p| p.selected_path()) {
                    self.load_requested = Some(path);
                    h.save_panel = None;
                    h.menu = None;
                }
            }
            other => log::debug!("game menu button {other} has no handler yet"),
        }
    }

    /// Writes the game to a saved game file and returns to the game.
    fn write_save(&mut self, path: &std::path::Path) {
        let res = self.save_game().and_then(|bytes| {
            if let Some(d) = path.parent() {
                std::fs::create_dir_all(d)?;
            }
            std::fs::write(path, bytes).map_err(Into::into)
        });
        if let Err(e) = res {
            log::error!("saving {}: {e:#}", path.display());
        }
        if let Some(h) = self.hud.as_mut() {
            h.save_panel = None;
            h.menu = None;
        }
    }

    /// Pointer movement, for tooltips.
    /// Lets go of the left button: ends dragging a game menu slider.
    pub fn left_release(&mut self, sx: f32, sy: f32, size: (u32, u32)) {
        if !std::mem::take(&mut self.menu_drag) {
            return;
        }
        let (ux, uy) = self.to_ui(sx, sy, size);
        if let Some(m) = self.hud.as_mut().and_then(|h| h.menu.as_mut()) {
            if let Some(i) = m.pointer_up(ux, uy) {
                let name = m.frames[i].name.clone();
                self.menu_button(&name);
            }
        }
    }

    pub fn hover(&mut self, sx: f32, sy: f32, size: (u32, u32)) {
        self.cursor = (sx, sy);
        let p = self.to_ui(sx, sy, size);
        if let Some(h) = self.hud.as_mut() {
            h.pointer = p;
            if let Some(m) = h.menu.as_mut().filter(|_| self.menu_drag) {
                m.pointer_move(p.0, p.1);
            }
            if let Some(s) = h.save_panel.as_mut() {
                s.pointer_move(p.0, p.1);
            }
            if let Some(q) = h.quests.as_mut() {
                q.pointer_move(p.0, p.1);
            }
            if let Some(l) = h.log_view.as_mut() {
                l.pointer_move(p.0, p.1);
            }
        }
    }

    /// A key press: command card hotkeys and Escape.
    /// Plays as a network game: the interface's commands go to the host, and the game
    /// moves only by the host's frames.
    pub fn start_net(&mut self, role: crate::net::Role, speed: u8) {
        if let Some(g) = self.game.as_mut() {
            g.outbox = Some(Vec::new());
            g.world.network = true;
            g.world.game_speed = speed as i32;
        }
        self.net = Some(crate::net::Session::new(role));
    }

    /// The replay of the game played, to keep when it ends.
    pub fn take_replay(&mut self) -> Option<w3_game::replay::Replay> {
        let r = self.game.as_mut()?.recorder.take()?;
        (!r.replay.frames.is_empty()).then_some(r.replay)
    }

    /// A line the player typed: a cheat, or chat shown as game.dll formats it
    /// (FUN_6f373470: "%s |c%2.2x%2.2x%2.2x%2.2x%s: |r%s", the recipient, the player's
    /// color and name, and the line).
    fn send_chat(&mut self, line: &str) {
        if self.slash_command(line) {
            return;
        }
        let to = self.hud.as_ref().map_or(0, |h| h.chat_to);
        let Some(g) = self.game.as_mut() else { return };
        let local = g.world.local_player;
        g.apply(w3_game::replay::Command::Chat { player: local, text: line.to_string(), to });
    }

    pub fn key(&mut self, c: char) {
        if let Some(h) = self.hud.as_mut().filter(|h| h.chat.is_none() && h.menu.is_none()) {
            if h.replay.as_mut().is_some_and(|r| r.key(c)) {
                return;
            }
        }
        // The save panel's edit box takes typing; Enter saves and Escape cancels.
        if let Some(p) = self.hud.as_mut().and_then(|h| h.save_panel.as_mut()) {
            if p.confirm.is_none() {
                let load = p.load;
                match c {
                    '\u{1b}' => return self.menu_button(if load { "LoadGameCancelButton" } else { "SaveGameCancelButton" }),
                    '\r' | '\n' => return self.menu_button(if load { "LoadGameLoadButton" } else { "SaveGameSaveButton" }),
                    '\u{8}' if !p.load => {
                        p.text.pop();
                    }
                    c if !p.load && !c.is_control() && !"\\/:*?\"<>|".contains(c) => p.text.push(c),
                    _ => {}
                }
            } else if c == '\u{1b}' {
                p.confirm = None;
            }
            return;
        }
        // The chat bar takes typing while it is open; Enter sends the line, Escape drops it.
        if let Some(text) = self.hud.as_mut().and_then(|h| h.chat.as_mut()) {
            match c {
                '\u{1b}' => {
                    self.hud.as_mut().unwrap().chat = None;
                }
                '\r' | '\n' => {
                    let line = self.hud.as_mut().unwrap().chat.take().unwrap_or_default();
                    if !line.trim().is_empty() {
                        self.send_chat(&line);
                    }
                }
                '\u{8}' => {
                    text.pop();
                }
                c if !c.is_control() && text.chars().count() < 127 => text.push(c),
                _ => {}
            }
            return;
        }
        if (c == '\r' || c == '\n') && self.user_control() && self.hud.as_ref().is_some_and(|h| h.menu.is_none() && h.quests.is_none() && h.log_view.is_none()) {
            // In a network game Ctrl+Enter speaks to allies, Shift+Enter to all, and Enter
            // to the chat dialog's choice, which starts as allies when the player has any
            // and everyone otherwise (CChatDialog's constructor); alone the prompt is
            // "Message:" (FUN_6f3af7f0, FUN_6f3d0500).
            let network = self.game.as_ref().is_some_and(|g| g.world.network);
            let default = self.chat_recipient();
            let to = if self.ctrl_held {
                1
            } else if self.shift_held {
                0
            } else {
                default
            };
            let key = if !network {
                "COLON_MESSAGE_SINGLEPLAYER"
            } else if to == 1 {
                "COLON_MESSAGE_ALLIES"
            } else {
                "COLON_MESSAGE_ALL"
            };
            let prompt = self.game.as_ref().map(|g| g.world.localized(key)).unwrap_or_default();
            if let Some(h) = self.hud.as_mut() {
                h.chat = Some(String::new());
                h.chat_to = if network { to } else { 0 };
                h.chat_prompt = prompt;
            }
            return;
        }
        // Escape is the player's end-cinematic event (EVENT_PLAYER_END_CINEMATIC).
        if c == '\u{1b}' {
            if let Some(g) = self.game.as_mut() {
                let p = g.world.local_player;
                g.apply(w3_game::replay::Command::PlayerEvent { player: p, event: w3_game::event_ids::EVENT_PLAYER_END_CINEMATIC });
            }
        }
        if !self.user_control() && c != '\u{1b}' {
            return;
        }
        if c == '\u{1b}' {
            if let Some(h) = self.hud.as_mut().filter(|h| h.menu.is_some()) {
                h.menu = None;
                return;
            }
            if let Some(h) = self.hud.as_mut().filter(|h| h.quests.is_some()) {
                h.quests = None;
                return;
            }
            if let Some(h) = self.hud.as_mut().filter(|h| h.log_view.is_some()) {
                h.log_view = None;
                return;
            }
        }
        if c == '\u{1b}' && (self.item_target.take().is_some() || self.held_item.take().is_some()) {
            return;
        }
        if c == '\u{1b}' && (self.targeting.is_some() || self.placing.is_some()) {
            self.card_action(crate::card::Action::Cancel);
            return;
        }
        // Tab makes the next subgroup (units of the next type) active.
        if c == '\t' {
            if let Some(g) = self.game.as_ref() {
                let first = self.selection.first().and_then(|&h| g.world.unit(h)).map(|u| u.type_id);
                if let Some(k) = self.selection.iter().position(|&h| g.world.unit(h).map(|u| u.type_id) != first) {
                    self.selection.rotate_left(k);
                }
            }
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
        if !self.user_control() {
            return;
        }
        if self.targeting.take().is_some() || self.placing.take().is_some() || self.item_target.take().is_some() || self.held_item.take().is_some() {
            return;
        }
        let (ux, uy) = self.to_ui(sx, sy, size);
        // A right click in the Allies dialog takes an amount away.
        let shift = self.shift_held;
        if let Some(d) = self.hud.as_mut().and_then(|h| h.alliance.as_mut()) {
            d.click(ux, uy, false, shift);
            return;
        }
        // A right-click on an item picks it up to move.
        if let Some(slot) = self.hud.as_ref().and_then(|h| h.inventory_at(ux, uy)) {
            if let (Some(&u), Some(g)) = (self.selection.first(), self.game.as_ref()) {
                if g.world.unit(u).is_some_and(|un| w3_game::natives::players::controls(&g.world, g.world.local_player, u) && un.inventory.get(slot).is_some_and(|&i| i != 0)) {
                    self.held_item = Some((u, slot));
                }
            }
            return;
        }
        if let Some((wx, wy)) = self.hud.as_ref().and_then(|h| h.minimap_to_world(ux, uy)) {
            if let Some(g) = self.game.as_mut() {
                let smart = w3_game::natives::basic::order_id("smart");
                for &h in &self.selection {
                    if w3_game::natives::players::controls(&g.world, g.world.local_player, h) {
                        g.apply(w3_game::replay::Command::Order { unit: h, order: smart, x: wx, y: wy, target: 0 });
                    }
                }
            }
            return;
        }
        // Right-clicking an autocast button toggles its autocast for the selection.
        if let Some((on, off, state)) = self.hud.as_ref().and_then(|h| h.button_at(ux, uy)).and_then(|b| b.autocast) {
            if let Some(g) = self.game.as_mut() {
                let order = w3_game::natives::basic::order_id(if state { off } else { on });
                let local = g.world.local_player;
                for &h in &self.selection {
                    if w3_game::natives::players::controls(&g.world, local, h) {
                        g.apply(w3_game::replay::Command::Order { unit: h, order, x: 0.0, y: 0.0, target: 0 });
                    }
                }
            }
            return;
        }
        if self.over_console(sx, sy, size) {
            return;
        }
        let target = self.pick_unit(sx, sy, size).or_else(|| self.pick_item(sx, sy, size)).or_else(|| self.pick_destructable(sx, sy, size));
        let point = self.ground_at(sx, sy, size);
        let Some(g) = self.game.as_mut() else { return };
        let local = g.world.local_player;
        let smart = w3_game::natives::basic::order_id("smart");
        let mut ordered = false;
        for &h in &self.selection {
            if !w3_game::natives::players::controls(&g.world, local, h) {
                continue;
            }
            match (target, point) {
                (Some(t), _) if t != h => {
                    g.apply(w3_game::replay::Command::Issue { unit: h, order: smart, x: 0.0, y: 0.0, target: t, queue: self.shift_held });
                    ordered = true;
                }
                (_, Some((x, y))) => {
                    g.apply(w3_game::replay::Command::Issue { unit: h, order: smart, x, y, target: 0, queue: self.shift_held });
                }
                _ => {}
            }
        }
        // The target flashes in SelTargetColor (FUN_6f3814e0).
        if let (Some(t), true) = (target, ordered) {
            let v: Vec<f32> = g.world.misc.get_list("TargetingColors", "SelTargetColor").iter().filter_map(|x| x.trim().parse().ok()).collect();
            let c = if v.len() == 4 { [v[0] / 255.0, v[1] / 255.0, v[2] / 255.0, 1.0] } else { [1.0; 4] };
            self.add_indicator(t, c);
        }
        self.order_ack();
    }

    /// The first selected unit acknowledges an order: "YesAttack" when it is now
    /// attacking, else "Yes".
    fn order_ack(&mut self) {
        let Some(g) = &self.game else { return };
        let Some(&h) = self.selection.iter().find(|&&h| g.world.unit(h).is_some_and(|u| w3_game::natives::players::controls(&g.world, g.world.local_player, h) && !u.is_building)) else { return };
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
                if tex_id != 0 && !tex_file.is_empty() && tex_file != "_" {
                    let t = assets.textures.get_or_white(device, queue, &tex_file);
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
            // A script's animation (SetDestructableAnimation), at its speed.
            let seen = self.dest_anims.get(&h).map_or(0, |a| a.0);
            if d.anim_serial != seen {
                if let Some(name) = &d.animation {
                    inst.play(name);
                }
            }
            self.dest_anims.insert(h, (d.anim_serial, d.anim_speed));
        }
    }

    /// Shows items on the ground with their ItemData model and scale.
    pub fn sync_items(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, assets: &mut Assets) {
        let Some(g) = self.game.as_ref() else { return };
        let w = &g.world;
        let local = w.local_player;
        self.items.retain(|h, _| w.item(*h).is_some_and(|i| i.owner_unit == 0 && i.visible));
        for &h in &w.item_order {
            let Some(it) = w.item(h) else { continue };
            if it.owner_unit != 0 || !it.visible {
                continue;
            }
            if !self.items.contains_key(&h) {
                let Some(o) = w.objects.items.get(it.type_id) else { continue };
                let Some(model) = assets.model(device, queue, &o.str("file")) else { continue };
                let mut inst = ModelInstance::new(model);
                inst.play("stand");
                let scale = o.real("scale");
                let z = self.terrain.height_at(it.x, it.y);
                inst.transform = Mat4::from_scale_rotation_translation(Vec3::splat(if scale > 0.0 { scale } else { 1.0 }), Quat::IDENTITY, Vec3::new(it.x, it.y, z));
                self.items.insert(h, inst);
            }
            let inst = self.items.get_mut(&h).unwrap();
            let z = self.terrain.height_at(it.x, it.y);
            let s = inst.transform.to_scale_rotation_translation().0;
            inst.transform = Mat4::from_scale_rotation_translation(s, Quat::IDENTITY, Vec3::new(it.x, it.y, z));
            inst.visible = w.fog_state(it.x, it.y, local) != w3_game::fog::FOG_MASKED;
        }
    }

    /// The ground item under a screen point.
    pub fn pick_item(&self, sx: f32, sy: f32, size: (u32, u32)) -> Option<u32> {
        let g = self.game.as_ref()?;
        let mut best: Option<(f32, u32)> = None;
        for (&h, inst) in &self.items {
            if !inst.visible {
                continue;
            }
            let p = inst.transform.w_axis.truncate();
            let Some(c) = self.project(p + Vec3::Z * 30.0, size) else { continue };
            let Some(e) = self.project(p + Vec3::new(40.0, 0.0, 30.0), size) else { continue };
            let r = (e.0 - c.0).abs().max(10.0);
            let d = (c.0 - sx).hypot(c.1 - sy);
            if d <= r && best.is_none_or(|b| d < b.0) {
                best = Some((d, h));
            }
        }
        let _ = g;
        best.map(|b| b.1)
    }

    /// Creates, updates, and removes effect instances to match the game's effects: each
    /// plays its birth then stand; a destroyed one plays its death (or finishes what it
    /// plays when it has none) and is then dropped from the game.
    pub fn sync_effects(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, assets: &mut Assets) {
        let Some(g) = self.game.as_ref() else { return };
        let w = &g.world;
        let local = w.local_player;
        self.effects.retain(|h, _| matches!(w.handles.get(*h), Some(w3_game::types::Obj::Effect(_))));
        let mut finished = Vec::new();
        for (&h, o) in w.handles.iter() {
            let w3_game::types::Obj::Effect(e) = o else { continue };
            if !self.effects.contains_key(&h) {
                let Some(model) = (!e.model.is_empty()).then(|| assets.model(device, queue, &e.model)).flatten() else {
                    if e.destroyed {
                        finished.push(h);
                    }
                    continue;
                };
                let mut inst = ModelInstance::new(model);
                let has = |i: &ModelInstance, n: &str| w3_render::anim::find_sequence(&i.model.mdx, n).is_some();
                let first = if e.destroyed && has(&inst, "death") { "death" } else if has(&inst, "birth") { "birth" } else { "stand" };
                inst.play(first);
                self.effects.insert(h, EffectView { inst, dying: e.destroyed, done: false });
            }
            let v = self.effects.get_mut(&h).unwrap();
            if e.destroyed && !v.dying {
                v.dying = true;
                if w3_render::anim::find_sequence(&v.inst.model.mdx, "death").is_some() {
                    v.inst.play("death");
                } else if !v.inst.model.mdx.sequences.get(v.inst.sequence.unwrap_or(usize::MAX)).is_some_and(|s| s.non_looping) {
                    v.done = true;
                }
            }
            if v.inst.finished || v.inst.looped {
                if v.dying {
                    v.done = true;
                } else if v.inst.finished || v.inst.sequence == w3_render::anim::find_sequence(&v.inst.model.mdx, "birth") {
                    v.inst.play("stand");
                }
                v.inst.looped = false;
            }
            if v.done {
                finished.push(h);
                v.inst.visible = false;
                continue;
            }
            let pos = match &e.target {
                Some((t, attach)) => match self.units.get(t) {
                    Some(uv) if uv.inst.visible => Some(attachment_point(&uv.inst, attach)),
                    Some(_) => None,
                    None => w.widget_pos(*t).map(|(x, y)| Vec3::new(x, y, self.terrain.height_at(x, y))),
                },
                None => Some(Vec3::new(e.x, e.y, e.z)),
            };
            match pos {
                Some(p) => {
                    v.inst.transform = Mat4::from_rotation_translation(Quat::from_rotation_z(e.facing.to_radians()), p);
                    v.inst.visible = w.fog_state(p.x, p.y, local) != w3_game::fog::FOG_MASKED;
                }
                None => v.inst.visible = false,
            }
        }
        if let Some(g) = self.game.as_mut() {
            for h in finished {
                if matches!(g.world.handles.get(h), Some(w3_game::types::Obj::Effect(e)) if e.destroyed) {
                    g.world.handles.remove(h);
                    self.effects.remove(&h);
                }
            }
        }
    }

    /// Draws each missile in flight at its place, rising from its launch height to its
    /// target's impact height (impactZ) along an arc whose peak is its arc times the
    /// distance (a parabola; the engine's own curve was not traced), facing along its
    /// path.  A missile that has landed plays its death where it fell, then goes.
    pub fn sync_missiles(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, assets: &mut Assets) {
        let Some(g) = self.game.as_ref() else { return };
        let w = &g.world;
        let local = w.local_player;
        let live: std::collections::HashSet<u32> = w.missiles.iter().map(|m| m.id).collect();
        // Landed: play the death, or drop at once without one.
        self.missiles.retain(|id, v| {
            if live.contains(id) {
                return true;
            }
            if !v.dying {
                v.dying = true;
                if w3_render::anim::find_sequence(&v.inst.model.mdx, "death").is_some() {
                    v.inst.play("death");
                    return true;
                }
                return false;
            }
            !(v.inst.finished || v.inst.looped)
        });
        for m in &w.missiles {
            if m.id == 0 || m.model.is_empty() {
                continue;
            }
            if !self.missiles.contains_key(&m.id) {
                let Some(model) = assets.model(device, queue, &m.model) else { continue };
                let mut inst = ModelInstance::new(model);
                inst.play("stand");
                inst.team = w.unit(m.source).and_then(|u| w.players.get(u.owner as usize)).map_or(0, |p| p.color as u8);
                self.missiles.insert(m.id, EffectView { inst, dying: false, done: false });
            }
            let target = m.impact.or_else(|| w.widget_pos(m.target)).unwrap_or((m.x, m.y));
            let target_z = w.unit(m.target).map_or(self.terrain.height_at(target.0, target.1), |t| {
                let impact = w.objects.units.get(t.type_id).map_or(60.0, |o| o.real("impactZ"));
                self.terrain.height_at(t.x, t.y) + t.fly_height + impact
            });
            let total = (target.0 - m.start.0).hypot(target.1 - m.start.1).max(1.0);
            let left = (target.0 - m.x).hypot(target.1 - m.y);
            let t = (1.0 - left / total).clamp(0.0, 1.0);
            let z = m.z + (target_z - m.z) * t + m.arc * total * 4.0 * t * (1.0 - t);
            let slope = (target_z - m.z) / total + m.arc * 4.0 * (1.0 - 2.0 * t);
            let yaw = (target.1 - m.y).atan2(target.0 - m.x);
            let v = self.missiles.get_mut(&m.id).unwrap();
            v.inst.transform = Mat4::from_rotation_translation(Quat::from_rotation_z(yaw) * Quat::from_rotation_y(-slope.atan()), Vec3::new(m.x, m.y, z));
            v.inst.visible = w.fog_state(m.x, m.y, local) == w3_game::fog::FOG_VISIBLE;
        }
    }

    /// Creates, updates, and removes render instances to match the game's units.
    pub fn sync_units(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, assets: &mut Assets) {
        self.sync_effects(device, queue, assets);
        self.sync_missiles(device, queue, assets);
        if let Some(g) = self.game.as_ref() {
            let terrain = &self.terrain;
            self.weather.sync(device, queue, assets, &g.world, |x, y| terrain.height_at(x, y), self.camera.target);
        }
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
                let mut v = UnitView { inst: ModelInstance::new(model), type_id: [0; 4], children: Vec::new(), serial: 0, requested: "portrait".into(), anim: String::new(), rng: h, queued: 0 };
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
        // The transmission's speaker: "portrait talk" while the voice plays.
        match w.cinematic.scene.as_ref() {
            Some(s) => {
                if self.cine_portrait.as_ref().map(|p| p.0) != Some(s.unit_type) {
                    self.cine_portrait = w.objects.units.get(s.unit_type).and_then(|o| {
                        let file = o.str("file");
                        let base = file.strip_suffix(".mdl").or_else(|| file.strip_suffix(".mdx")).unwrap_or(&file);
                        let model = assets.model(device, queue, &format!("{base}_Portrait")).or_else(|| assets.model(device, queue, &file))?;
                        let mut v = UnitView { inst: ModelInstance::new(model), type_id: [0; 4], children: Vec::new(), serial: 0, requested: String::new(), anim: String::new(), rng: 1, queued: 0 };
                        v.inst.team = s.color.clamp(0, 15) as u8;
                        Some((s.unit_type, v))
                    });
                }
                let talking = w.time < s.voice_ends;
                if let Some((_, v)) = self.cine_portrait.as_mut() {
                    let want = if talking { "portrait talk" } else { "portrait" };
                    if v.requested != want || v.inst.looped || v.inst.finished {
                        v.requested = want.into();
                        v.inst.looped = false;
                        v.pick(want);
                    }
                }
            }
            None => self.cine_portrait = None,
        }
        let footprint = self.camera_footprint();
        let team_rgb = self.team_rgb;
        if let Some(hud) = self.hud.as_mut() {
            let ally_mode = hud.ally_mode;
            hud.minimap_dots = w
                .unit_order
                .iter()
                .filter(|&&h| w.unit_visible_to(h, w.local_player) || w.unit(h).is_some_and(|u| u.is_building && w.fog_state(u.x, u.y, w.local_player) != w3_game::fog::FOG_MASKED))
                .filter_map(|&h| w.unit(h))
                .filter(|u| !u.dead && !u.hidden && !w.is_locust(u))
                .map(|u| {
                    // Ally color modes use [FogOfWar] FogColorPlayer, FogColorAlly,
                    // FogColorEnemy, and FogColorCreepNormal.
                    let c = if ally_mode == 0 {
                        team_rgb[u.color.clamp(0, 15) as usize]
                    } else {
                        let key = if u.owner == w.local_player {
                            "FogColorPlayer"
                        } else if u.owner as usize == w3_game::types::PLAYER_NEUTRAL_AGGRESSIVE as usize {
                            "FogColorCreepNormal"
                        } else if w3_game::natives::players::is_ally(w, w.local_player, u.owner) {
                            "FogColorAlly"
                        } else {
                            "FogColorEnemy"
                        };
                        let v: Vec<f32> = w.misc.get_list("FogOfWar", key).iter().filter_map(|s| s.trim().parse().ok()).collect();
                        if v.len() == 4 { [v[1] / 255.0, v[2] / 255.0, v[3] / 255.0] } else { team_rgb[u.color.clamp(0, 15) as usize] }
                    };
                    (u.x, u.y, [c[0], c[1], c[2], 1.0], if u.is_building { 0.004 } else { 0.0025 })
                })
                .collect();
            hud.alt_icon = w.alt_minimap_icon.clone();
            hud.minimap_alt = w
                .unit_order
                .iter()
                .filter_map(|&h| w.unit(h).map(|u| (h, u)))
                .filter(|(h, u)| u.extra.alt_icon && !u.dead && !u.hidden && w.unit_visible_to(*h, w.local_player))
                .map(|(_, u)| (u.x, u.y))
                .collect();
            hud.camera_box = footprint;
            // The minimap's fog: black over the map image, leaving FogColorFogMaskAlpha of it
            // in fogged cells and FogColorBlackMaskAlpha (or FogColorDarkMaskAlpha where
            // masked areas are partially visible) in masked ones.
            if let (Some(area), true) = (hud.minimap_area, w.fog.is_some()) {
                let n = crate::hud::MINIMAP_FOG_SIZE;
                let (sx, sy, side) = area.square();
                let cover = |key: &str, default: u8| -> [u8; 4] {
                    let shown: u8 = w.misc.get_str("FogOfWar", key).and_then(|v| v.trim().parse().ok()).unwrap_or(default);
                    [0, 0, 0, 255 - shown]
                };
                let dark = self.info.flags & 0x10 != 0;
                let (fogged, masked) = (cover("FogColorFogMaskAlpha", 192), if dark { cover("FogColorDarkMaskAlpha", 96) } else { cover("FogColorBlackMaskAlpha", 0) });
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
        }
        for &h in &w.unit_order {
            let Some(u) = w.unit(h) else { continue };
            // A unit that changed type (a morph) takes its new model.
            if self.units.get(&h).is_some_and(|v| v.type_id != u.type_id) {
                self.units.remove(&h);
            }
            if !self.units.contains_key(&h) {
                let Some(o) = w.objects.units.get(u.type_id) else { continue };
                let file = o.str("file");
                let Some(model) = assets.model(device, queue, &file) else { continue };
                let children = model
                    .mdx
                    .attachments
                    .iter()
                    .enumerate()
                    .filter(|(_, a)| !a.path.is_empty())
                    .filter_map(|(i, a)| assets.model(device, queue, &a.path).map(|m| (i, ModelInstance::new(m))))
                    .collect();
                let inst = ModelInstance::new(model);
                let mut view = UnitView { inst, type_id: u.type_id, children, serial: u32::MAX, requested: String::new(), anim: String::new(), rng: h, queued: 0 };
                view.update(&u.anim, u.anim_serial, &u.extra.anim_queue);
                self.units.insert(h, view);
            }
            let view = self.units.get_mut(&h).unwrap();
            // A unit's animations cross-fade over its blend (UnitUI blend, or
            // SetUnitBlendTime).
            let blend = u.extra.blend_time.unwrap_or_else(|| w.objects.units.get(u.type_id).map_or(0.15, |o| o.real("blend")));
            view.inst.blend_ms = blend * 1000.0;
            // The unit's color and transparency (UnitUI red, green, blue;
            // SetUnitVertexColor).
            view.inst.tint = glam::Vec4::new(u.vertex_color[0] as f32, u.vertex_color[1] as f32, u.vertex_color[2] as f32, u.vertex_color[3] as f32) / 255.0;
            // An uprooted ancient takes the alternate of each animation but its walk; the
            // type's Animprops ("upgrade,first" for a Keep on the Town Hall's model) and
            // those scripts add qualify every animation.
            let mut anim = if u.extra.uprooted && !u.anim.starts_with("walk") && !u.anim.starts_with("morph") && !u.anim.contains("alternate") { format!("{} alternate", u.anim) } else { u.anim.clone() };
            let type_props = w.objects.units.get(u.type_id).map(|o| o.str("Animprops")).unwrap_or_default();
            for p in type_props.split(',').map(|p| p.trim().to_ascii_lowercase()).chain(u.extra.anim_props.iter().cloned()) {
                if !p.is_empty() && p != "_" && !anim.split_whitespace().any(|w| w == p) {
                    anim.push(' ');
                    anim.push_str(&p);
                }
            }
            view.update(&anim, u.anim_serial, &u.extra.anim_queue);
            // Structures under construction show their birth animation in step with progress.
            if let (Some(left), Some(seq)) = (u.constructing, view.inst.sequence.and_then(|s| view.inst.model.mdx.sequences.get(s))) {
                let len = (seq.end - seq.start) as f32;
                view.inst.frame = len * (1.0 - left / u.build_time.max(0.01)).clamp(0.0, 1.0);
            }
            // SetUnitLookAt: the bone faces the target unit, offset.
            view.inst.look_at = u.extra.look_at.as_ref().and_then(|(bone, t, off)| {
                let tu = w.unit(*t)?;
                let tz = self.terrain.height_at(tu.x, tu.y) + tu.fly_height;
                Some((bone.clone(), Vec3::new(tu.x + off[0], tu.y + off[1], tz + off[2])))
            });
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
        self.frame_dt = dt;
        // The game menu pauses a single-player game; the game speed (slow, normal, fast:
        // 3/5, 4/5, and 5/5 of real time in the original) scales the game and its
        // animation.
        // FlashQuestDialogButton: QuestLogModified, and QuestChangedParticles on the
        // Quests button for [QuestIndicatorTimeout] (at least 2 seconds) unless the
        // quest log is open (FUN_6f36c9b0, FUN_6f3b0ce0).
        if let Some(g) = self.game.as_mut().filter(|g| g.world.quest_flash) {
            g.world.quest_flash = false;
            let timeout = g.world.misc.get_str("QuestIndicatorTimeout", "QuestIndicatorTimeout").and_then(|v| v.trim().parse().ok()).unwrap_or(10.0f32).max(2.0);
            if let Some(h) = self.hud.as_mut().filter(|h| h.quests.is_none()) {
                h.quest_indicator = Some(h.now + timeout);
            }
            self.ui_sound("QuestLogModified");
        }
        // A network game does not stop for one player's menus.
        let paused = self.net.is_none() && self.hud.as_ref().is_some_and(|h| h.menu.is_some() || h.quests.is_some() || h.log_view.is_some());
        let speed = self.game.as_ref().map_or(1.0, |g| [0.6, 0.8, 1.0][g.world.game_speed.clamp(0, 2) as usize]);
        let gdt = if paused { 0.0 } else { dt * speed };
        // The selection: script changes come to the interface, the player's go to the
        // game (with its selection events).
        if let Some(g) = self.game.as_mut() {
            let local = g.world.local_player;
            if g.world.selection_serial != self.selection_serial {
                self.selection_serial = g.world.selection_serial;
                self.selection = g.world.selection.get(&local).cloned().unwrap_or_default();
            } else {
                let sel = self.selection.clone();
                g.apply(w3_game::replay::Command::Select { player: local, units: sel });
            }
        }
        // A network game: the commands go out, and the host's frames play as they come.
        if let (Some(g), Some(s)) = (self.game.as_mut(), self.net.as_mut()) {
            let local = g.world.local_player;
            let out = g.outbox.as_mut().map(std::mem::take).unwrap_or_default();
            s.send_commands(local, out);
            s.pump(gdt);
            let mut errors = Vec::new();
            for f in s.due(gdt) {
                for (p, o) in g.play_frame(&f) {
                    if let (true, w3_game::replay::Outcome::Error(e)) = (p == local, o) {
                        errors.push(e);
                    }
                }
            }
            let lost = s.lost;
            // The host waits on players who fall behind; a player waits on a silent host.
            self.net_waiting = (s.lagging(), s.host_silent());
            for e in errors {
                self.error(e);
            }
            // Without the host the game is over.
            if lost {
                self.ended = true;
                self.final_score = self.game.as_ref().map(|g| crate::scorescreen::ScoreData::from_world(&g.world));
            }
        } else if let Some(g) = self.game.as_mut().filter(|_| !paused) {
            match self.playback.as_mut() {
                // A replay plays its frames as their time comes, at the panel's speed, with
                // the game's own player and fog.
                Some(p) => {
                    let panel = self.hud.as_mut().and_then(|h| h.replay.as_mut());
                    p.owed += panel.as_ref().map_or(gdt, |r| r.scaled(gdt));
                    if let Some(r) = panel.as_ref() {
                        r.enter(&mut g.world);
                    }
                    while let Some(f) = p.replay.frames.get(p.next) {
                        if f.dt > p.owed {
                            break;
                        }
                        p.owed -= f.dt;
                        g.play_frame(f);
                        p.next += 1;
                    }
                    if let Some(r) = panel {
                        r.leave(&mut g.world);
                    }
                    let elapsed: f32 = p.replay.frames[..p.next].iter().map(|f| f.dt).sum();
                    if let Some(h) = self.hud.as_mut() {
                        if let Some(r) = h.replay.as_ref() {
                            r.sync(&mut h.tree, elapsed);
                        }
                    }
                }
                None => g.tick(gdt),
            }
        }
        self.apply_requests();
        let now = self.time;
        if let Some(h) = self.hud.as_mut() {
            h.tick(now);
        }
        // Without audio, keep only the music requests for when it starts.
        if self.audio.is_none() {
            self.pending_audio.retain(|r| {
                use w3_game::world::Request as R;
                matches!(r, R::PlayMusic(_) | R::StopMusic(_) | R::PlayThematic(_) | R::EndThematic | R::ResumeMusic | R::MusicVolume(_))
            });
        }
        for inst in &mut self.instances {
            inst.advance(gdt * 1000.0);
        }
        for (h, v) in self.units.iter_mut() {
            let progress = self.game.as_ref().and_then(|g| g.world.unit(*h)).and_then(|u| u.constructing.map(|left| 1.0 - left / u.build_time.max(0.01)));
            // SetUnitTimeScale speeds or slows the unit's animation, and a walk plays at
            // the unit's speed over its UnitUI walk speed.
            let scale = self.game.as_ref().and_then(|g| g.world.unit(*h).map(|u| (g, u))).map_or(1.0, |(g, u)| {
                let walk = if u.anim.starts_with("walk") {
                    let base = g.world.objects.units.get(u.type_id).map_or(0.0, |o| o.real("walk"));
                    if base > 0.0 { g.world.current_move_speed(*h) / base } else { 1.0 }
                } else {
                    1.0
                };
                u.time_scale * walk
            });
            match progress {
                // A structure's birth plays along with its construction.
                Some(p) => {
                    if let Some(s) = v.inst.sequence.and_then(|s| v.inst.model.mdx.sequences.get(s)) {
                        v.inst.frame = (s.end - s.start) as f32 * p.clamp(0.0, 1.0);
                    }
                    v.inst.global_time += gdt * 1000.0;
                }
                None => v.inst.advance(gdt * 1000.0 * scale),
            }
        }
        for (h, (inst, _)) in self.dests.iter_mut() {
            let speed = self.dest_anims.get(h).map_or(1.0, |a| a.1);
            inst.advance(gdt * 1000.0 * speed);
        }
        for inst in self.items.values_mut() {
            inst.advance(gdt * 1000.0);
        }
        for v in self.effects.values_mut() {
            v.inst.advance(gdt * 1000.0);
        }
        for v in self.missiles.values_mut() {
            v.inst.advance(gdt * 1000.0);
        }
        self.weather.advance(gdt * 1000.0);
        for fx in self.autocast_fx.values_mut() {
            fx.advance(gdt * 1000.0);
        }
        if let Some(fx) = self.quest_fx.as_mut() {
            fx.advance(dt * 1000.0);
        }
        if let Some(c) = self.cursor_model.as_mut() {
            c.advance(dt * 1000.0);
        }
        for p in self.pings.iter_mut() {
            if let Some(fx) = p.4.as_mut() {
                fx.advance(dt * 1000.0);
            }
        }
        if let Some((_, v)) = self.portrait.as_mut() {
            v.inst.advance(gdt * 1000.0);
        }
        if let Some((_, v)) = self.cine_portrait.as_mut() {
            v.inst.advance(gdt * 1000.0);
        }
        self.sync_camera();
    }

    /// The render camera from the game's: target, distance, angles, field of view, far
    /// plane, and height offset.
    pub fn sync_camera(&mut self) {
        use w3_game::camera::*;
        if let Some(g) = self.game.as_ref() {
            let c = &g.world.camera;
            self.camera.target.x = c.x;
            self.camera.target.y = c.y;
            self.camera.distance = c.fields[FIELD_TARGET_DISTANCE];
            self.camera.far = c.fields[FIELD_FAR_Z];
            self.camera.angle_of_attack = c.fields[FIELD_ANGLE_OF_ATTACK];
            self.camera.fov = c.fields[FIELD_FIELD_OF_VIEW];
            self.camera.roll = c.fields[FIELD_ROLL];
            self.camera.rotation = c.fields[FIELD_ROTATION];
            self.camera.target.z = self.camera_ground(c.x, c.y) + c.fields[FIELD_ZOFFSET];
            // A cinematic camera model's first camera, over its first sequence (holding
            // its last frame), places the eye and target in map coordinates.
            if let Some((path, start)) = c.cinematic.clone() {
                if self.cine_camera.as_ref().map(|c| &c.0) != Some(&path) {
                    self.cine_camera = Some((path.clone(), Self::load_dnc(&g.world.chain, &path)));
                }
                let now = g.world.time;
                if let Some(m) = self.cine_camera.as_ref().and_then(|c| c.1.as_ref()) {
                    if let Some(cam) = m.cameras.first() {
                        let (s0, s1) = m.sequences.first().map_or((0, 0), |s| (s.start, s.end));
                        let frame = (s0 as f32 + (now - start).max(0.0) * 1000.0).min(s1 as f32) as u32;
                        let time = w3_render::anim::Time { frame, seq_start: s0, seq_end: s1, global: (now * 1000.0) as u32 };
                        let track = |tag: &[u8; 4]| cam.tracks.vec3s.iter().find(|(t, _)| t == tag).map(|(_, tr)| tr);
                        let pos = Vec3::from_array(cam.position) + Vec3::from_array(w3_render::anim::sample(track(b"KCTR"), &time, &m.global_sequences, [0.0; 3]));
                        let target = Vec3::from_array(cam.target) + Vec3::from_array(w3_render::anim::sample(track(b"KTTR"), &time, &m.global_sequences, [0.0; 3]));
                        let roll = cam.tracks.f32s.iter().find(|(t, _)| t == b"KCRL").map_or(0.0, |(_, tr)| w3_render::anim::sample(Some(tr), &time, &m.global_sequences, 0.0));
                        let d = target - pos;
                        let len = d.length().max(1.0);
                        self.camera.target = target;
                        self.camera.distance = len;
                        self.camera.angle_of_attack = (d.z / len).asin().to_degrees();
                        self.camera.rotation = d.y.atan2(d.x).to_degrees();
                        self.camera.roll = roll.to_degrees();
                        self.camera.fov = cam.fov.to_degrees();
                        self.camera.far = cam.far_clip.max(self.camera.far);
                        self.camera.eye_offset = Vec3::ZERO;
                        return;
                    }
                }
            }
            // Camera noise shakes the target and the eye.
            let t = noise_offset(c.target_noise, self.time);
            self.camera.target += Vec3::from_array(t);
            self.camera.eye_offset = Vec3::from_array(noise_offset(c.source_noise, self.time + 17.0));
            // The smoothing factor eases the target toward where it should be.
            let wanted = self.camera.target;
            if let Some(last) = self.smoothed_target {
                let fps = if self.frame_dt > 0.0 { 1.0 / self.frame_dt } else { 0.0 };
                let share = smoothing_share(c.smoothing, (wanted - last).length(), fps);
                self.camera.target = last + (wanted - last) * share;
            }
            self.smoothed_target = Some(self.camera.target);
        } else {
            self.camera.target.z = self.camera_ground(self.camera.target.x, self.camera.target.y);
        }
    }

    /// SetDoodadAnimation(Rect): the doodads of a type within the area (or only the
    /// nearest) play the animation, a random variant of it when asked.
    fn doodad_animation(&mut self, id: [u8; 4], area: &w3_game::world::DoodadArea, anim: &str, random: bool) {
        use w3_game::world::DoodadArea as A;
        let pos = |inst: &ModelInstance| (inst.transform.w_axis.x, inst.transform.w_axis.y);
        let mut hits: Vec<(usize, f32)> = self
            .instances
            .iter()
            .enumerate()
            .filter(|(k, _)| self.doodad_ids.get(*k) == Some(&id))
            .filter_map(|(k, inst)| {
                let (x, y) = pos(inst);
                match *area {
                    A::Circle { x: cx, y: cy, radius, .. } => {
                        let d = (x - cx).hypot(y - cy);
                        (d <= radius).then_some((k, d))
                    }
                    A::Rect { min_x, min_y, max_x, max_y } => (x >= min_x && x <= max_x && y >= min_y && y <= max_y).then_some((k, 0.0)),
                }
            })
            .collect();
        if matches!(area, A::Circle { nearest: true, .. }) {
            hits.sort_by(|a, b| a.1.total_cmp(&b.1));
            hits.truncate(1);
        }
        for (k, _) in hits {
            let inst = &mut self.instances[k];
            let seqs = w3_render::anim::sequences_named(&inst.model.mdx, anim);
            let s = if random && !seqs.is_empty() { seqs.get((self.time * 1000.0) as usize % seqs.len()).copied() } else { seqs.first().copied().or_else(|| w3_render::anim::find_sequence(&inst.model.mdx, anim)) };
            if s.is_some() {
                inst.set_sequence(s);
            }
        }
    }

    /// Brings the interface up to date with the game (done each frame before drawing).
    pub fn sync_hud(&mut self, db: &w3_ui::FrameDb, aspect: f32) {
        let recipient = self.chat_recipient();
        let (Some(hud), Some(g)) = (self.hud.as_mut(), self.game.as_ref()) else { return };
        hud.update(db, &g.world);
        hud.sync_dialog(db, &g.world, g.world.local_player, aspect);
        if std::mem::take(&mut self.chat_dialog_requested) {
            hud.chat_dialog = crate::chatdialog::ChatDialog::open(db, aspect, &g.world, &hud.chat_history, recipient);
        }
        if std::mem::take(&mut self.alliance_requested) {
            hud.alliance = crate::alliance::AllianceDialog::new(db, &g.world, aspect);
        }
        // The waiting dialogs of a network game.
        let (lagging, silent) = &self.net_waiting;
        if self.net.is_none() {
        } else if !lagging.is_empty() {
            if hud.net_dialog.as_ref().is_none_or(|d| d.players() != *lagging) {
                let players: Vec<(u8, String, i32)> = lagging.iter().map(|&p| (p, g.world.players[p as usize].name.clone(), g.world.players[p as usize].color)).collect();
                hud.net_dialog = crate::netdialog::NetDialog::suspend(db, aspect, &players);
            }
        } else if *silent {
            if hud.net_dialog.is_none() {
                hud.net_dialog = crate::netdialog::NetDialog::unresponsive(db, aspect);
            }
        } else {
            hud.net_dialog = None;
        }
        if std::mem::take(&mut self.menu_requested) && hud.menu.is_none() {
            hud.quests = None;
            hud.log_view = None;
            hud.open_menu(db, aspect);
        }
        Self::update_quests(&mut self.quests_requested, &mut self.log_requested, hud, db, &g.world, &self.info, aspect);
        hud.sync_save_panel(db);
        if let Some(m) = hud.menu.as_mut() {
            // Network games cannot be restarted, saved, or loaded here.
            let net = g.world.network;
            m.set_enabled("RestartButton", !g.world.restart_disabled && !net);
            m.set_enabled("SaveGameButton", !net);
            m.set_enabled("LoadGameButton", !net);
        }
        hud.show_unit(db, &g.world, self.selection.first().copied());
        let w = &g.world;
        let page = hud.page;
        hud.card = self.selection.first().map_or_else(Vec::new, |&h| card_of(&self.commands, db, w, h, page));
        hud.show_group(&g.world, &self.selection);
        hud.sync_cinematic(db, &g.world, aspect);
        hud.sync_heroes(&g.world);
        hud.sync_idle_workers(&g.world);
        crate::boards::sync(&mut hud.boards, db, &g.world, &self.team_rgb, aspect);
    }

    /// Opens a requested quest log and shows a changed choice in it.
    fn update_quests(requested: &mut bool, log: &mut bool, hud: &mut crate::hud::Hud, db: &w3_ui::FrameDb, world: &w3_game::world::World, info: &MapInfo, aspect: f32) {
        if std::mem::take(log) && hud.menu.is_none() {
            hud.quests = None;
            hud.log_view = crate::msglog::LogView::open(db, aspect, &hud.log);
        }
        if std::mem::take(requested) && hud.menu.is_none() {
            hud.log_view = None;
            hud.quest_indicator = None;
            hud.quests = crate::quests::QuestView::open(db, aspect, world, (&info.loading_title, &info.loading_subtitle));
        }
        if let Some(q) = hud.quests.as_mut().filter(|q| q.dirty) {
            q.dirty = false;
            q.sync(db, world);
        }
    }

    /// Opens a requested game menu and updates its save panel (done each frame).
    pub fn update_menu(&mut self, db: &w3_ui::FrameDb, aspect: f32) {
        let Some(hud) = self.hud.as_mut() else { return };
        if let Some(g) = self.game.as_ref() {
            Self::update_quests(&mut self.quests_requested, &mut self.log_requested, hud, db, &g.world, &self.info, aspect);
        }
        if std::mem::take(&mut self.menu_requested) && hud.menu.is_none() {
            hud.open_menu(db, aspect);
        }
        if std::mem::take(&mut self.load_dialog_requested) && hud.menu.is_some() {
            self.menu_button("LoadGameButton");
        }
        let Some(hud) = self.hud.as_mut() else { return };
        hud.sync_save_panel(db);
        let Some(m) = hud.menu.as_mut() else { return };
        if std::mem::take(&mut self.options_fill) {
            let g = self.game.as_ref();
            let c = crate::options::panel::Context {
                speed: g.map_or(2, |g| g.world.game_speed.clamp(0, 2) as usize),
                difficulty: g.filter(|_| self.campaign).map(|g| g.world.difficulty),
                resolution: self.screen,
            };
            crate::options::panel::fill(m, db, &self.options, &c);
        }
        // The game speed's name follows its slider.
        if self.options_page == "GameplayPanel" {
            let speed = m.value("GameSpeedSlider").round() as usize;
            m.set_text("GameSpeedValue", &crate::options::panel::speed_name(db, speed));
        }
    }

    /// The selected unit's command card, as the interface would show it.
    pub fn card_now(&self, db: &w3_ui::FrameDb) -> Vec<crate::card::Button> {
        let (Some(g), Some(&h)) = (self.game.as_ref(), self.selection.first()) else { return Vec::new() };
        card_of(&self.commands, db, &g.world, h, crate::card::Page::Main)
    }

    /// The game as a saved game.
    pub fn save_game(&self) -> anyhow::Result<Vec<u8>> {
        let g = self.game.as_ref().context("no game")?;
        g.save(&self.map_path)
    }

    /// The ground height the camera looks at: the terrain 500 units around the target
    /// on a 3 by 3 grid, weighted 2 at the corners and center and 3 at the edges.
    fn camera_ground(&self, x: f32, y: f32) -> f32 {
        let (mut sum, mut total) = (0.0, 0.0);
        for k in 0..9 {
            let (dx, dy) = ((1 - k / 3) as f32 * 500.0, (1 - k % 3) as f32 * 500.0);
            let w = if k % 2 == 1 { 3.0 } else { 2.0 };
            sum += w * self.terrain.height_at(x + dx, y + dy);
            total += w;
        }
        sum / total
    }

    /// Scrolling the view (keys and screen edges).
    pub fn scroll(&mut self, dx: f32, dy: f32) {
        if !self.user_control() {
            return;
        }
        if let Some(g) = self.game.as_mut() {
            let (x, y) = (g.world.camera.x + dx, g.world.camera.y + dy);
            let (x, y) = g.world.clamp_to_camera_bounds(x, y);
            g.world.camera.pan_to(x, y, 0.0);
        } else {
            self.camera.target.x += dx;
            self.camera.target.y += dy;
        }
    }

    /// Zooming with the wheel.
    pub fn zoom(&mut self, amount: f32) {
        // Over the game menu, the wheel scrolls its text areas a line at a time.
        if let Some(h) = self.hud.as_mut() {
            if let Some(m) = h.menu.as_mut() {
                let (px, py) = h.pointer;
                if let Some(i) = (0..m.frames.len()).find(|&i| m.frames[i].def.kind == "TEXTAREA" && m.is_visible(i) && m.frames[i].rect.is_some_and(|r| r.contains(px, py))) {
                    let f = &mut m.frames[i];
                    f.selected = if amount > 0.0 { f.selected.saturating_sub(1) } else { f.selected + 1 };
                }
                return;
            }
        }
        if !self.user_control() {
            return;
        }
        if let Some(g) = self.game.as_mut() {
            let c = &mut g.world.camera;
            let d = (c.fields[w3_game::camera::FIELD_TARGET_DISTANCE] - amount).clamp(500.0, 5000.0);
            c.set_field(w3_game::camera::FIELD_TARGET_DISTANCE, d, 0.0);
        } else {
            self.camera.distance = (self.camera.distance - amount).clamp(500.0, 5000.0);
        }
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
        if let Some(g) = self.game.as_ref() {
            self.shadows.update(&g.world, &mut self.terrain, device, queue);
        }
        let view_proj = self.camera.view_proj(aspect);
        let eye = self.camera.eye();
        let rot = self.camera.billboard_rotation();
        let mut globals = default_globals(view_proj, eye);
        let light = self.light;
        globals.light_dir = [light.x, light.y, light.z, 0.0];
        if let Some(g) = &self.game {
            if g.world.blight_serial != self.blight_serial {
                self.blight_serial = g.world.blight_serial;
                self.terrain.set_blight(device, &g.world.blight);
            }
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
        // Terrain fog, from the camera's distance: game.dll hands the style (modulo 3: linear,
        // exponential, squared exponential), range, and density to Direct3D unchanged.
        if let Some((style, start, end, density, color)) = self.game.as_ref().and_then(|g| g.world.terrain_fog) {
            globals.fog = [start, end, style.rem_euclid(3) as f32 + 1.0, density];
            globals.fog_color = [color[0], color[1], color[2], 1.0];
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
                // w3i flag 0x10: masked areas partially visible.
                let dark = self.info.flags & 0x10 != 0;
                globals.fow_masked_terrain = argb(if dark { "DarkMaskedTerrain" } else { "BlackMaskedTerrain" });
                globals.fow_masked_object = argb(if dark { "DarkMaskedObject" } else { "BlackMaskedObject" });
                globals.water_tint = g.world.water_color.map(|v| v as f32 / 255.0);
            }
        }
        // The sky, centered on the eye, at the time of day through its first sequence.
        let sky_path = self.game.as_ref().and_then(|g| g.world.sky_model.clone());
        if self.sky.as_ref().map(|s| &s.0) != sky_path.as_ref() {
            self.sky = sky_path.and_then(|p| assets.model(device, queue, &p).map(|m| (p, ModelInstance::new(m))));
        }
        let hour = self.game.as_ref().map_or(12.0, |g| g.world.time_of_day);
        let day = self.game.as_ref().map_or(24.0, |g| g.world.misc_f32("DayHours", 24.0)).max(1.0);
        if let Some((_, sky)) = self.sky.as_mut() {
            if let Some(seq) = sky.model.mdx.sequences.first() {
                sky.sequence = Some(0);
                sky.frame = (seq.end - seq.start) as f32 * (hour / day).rem_euclid(1.0);
            }
            sky.transform = Mat4::from_translation(eye);
            self.sky_renderer.submit(sky, eye, rot);
        }
        // The model lights' cells cover the map (see w3_render::lights).
        let grid = LightGrid::new(&self.terrain);
        let cell = |inst: &ModelInstance| grid.cell(inst.transform.w_axis.truncate());
        for inst in &mut self.instances {
            // Cull instances well outside the view.
            let p = inst.transform.w_axis.truncate();
            if p.distance(self.camera.target) > self.camera.far * 1.5 {
                continue;
            }
            inst.light_cell = cell(inst);
            self.models.submit(inst, eye, rot);
        }
        for v in self.units.values_mut() {
            v.inst.light_cell = cell(&v.inst);
            for (_, c) in &mut v.children {
                c.light_cell = v.inst.light_cell;
            }
            self.models.submit(&mut v.inst, eye, rot);
            submit_children(&mut self.models, v, eye, rot);
        }
        let far = self.camera.far * 1.5;
        let target = self.camera.target;
        for (inst, _) in self.dests.values_mut() {
            if inst.transform.w_axis.truncate().distance(target) > far {
                continue;
            }
            inst.light_cell = cell(inst);
            self.models.submit(inst, eye, rot);
        }
        for v in self.effects.values_mut().filter(|v| v.inst.visible) {
            v.inst.light_cell = cell(&v.inst);
            self.models.submit(&mut v.inst, eye, rot);
        }
        for v in self.missiles.values_mut().filter(|v| v.inst.visible) {
            v.inst.light_cell = cell(&v.inst);
            self.models.submit(&mut v.inst, eye, rot);
        }
        for inst in self.weather.instances() {
            self.models.submit(inst, eye, rot);
        }
        if let Some(g) = self.game.as_ref() {
            self.lightning.draw(assets, device, queue, &g.world, self.camera.view(), self.time, &mut self.models);
        }
        for inst in self.items.values_mut() {
            inst.light_cell = cell(inst);
            self.models.submit(inst, eye, rot);
        }
        // The structure being placed, at the snapped point under the pointer.
        match self.placing {
            Some(id) => {
                let spot = self.ground_at(self.cursor.0, self.cursor.1, self.screen);
                if let (Some((x, y)), Some(g)) = (spot, self.game.as_ref()) {
                    let (x, y) = g.world.snap_placement(x, y);
                    if self.ghost.as_ref().is_none_or(|(k, _)| *k != id) {
                        let file = g.world.objects.units.get(id).map(|o| o.str("file")).unwrap_or_default();
                        self.ghost = assets.model(device, queue, &file).map(|m| {
                            let mut i = ModelInstance::new(m);
                            i.play("Stand");
                            i.team = g.world.players.get(g.world.local_player as usize).map_or(0, |p| p.color as u8);
                            (id, i)
                        });
                    }
                    if let Some((_, inst)) = self.ghost.as_mut() {
                        let scale = g.world.objects.units.get(id).map_or(1.0, |o| o.real("modelScale").max(0.01));
                        let facing = g.world.misc_f32("BuildingAngle", 270.0).to_radians();
                        inst.transform = Mat4::from_scale_rotation_translation(Vec3::splat(scale), Quat::from_rotation_z(facing), Vec3::new(x, y, self.terrain.height_at(x, y)));
                        self.models.submit(inst, eye, rot);
                    }
                }
            }
            None => self.ghost = None,
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
        // The lights the posed models give off, and each nearby cell's choice of them.
        let mut lights = Vec::new();
        {
            let mut add = |i: &ModelInstance| w3_render::lights::instance_lights(i, &mut lights);
            self.instances.iter().filter(|i| i.transform.w_axis.truncate().distance(target) <= far).for_each(&mut add);
            for v in self.units.values() {
                add(&v.inst);
                v.children.iter().for_each(|(_, c)| add(c));
            }
            self.dests.values().map(|(i, _)| i).filter(|i| i.transform.w_axis.truncate().distance(target) <= far).for_each(&mut add);
            self.effects.values().for_each(|v| add(&v.inst));
            self.missiles.values().for_each(|v| add(&v.inst));
            self.items.values().for_each(&mut add);
        }
        // Lights per object by the Lights option: 1, 4, or 8 (FUN_6f1206b0).
        let n = [1, 4, 8][self.options.lights.min(2) as usize];
        let sun = self.dnc.1.as_ref().and_then(|m| w3_render::dnc::light_at(m, self.game.as_ref().map_or(12.0, |g| g.world.time_of_day))).map_or(1.0, |l| l.intensity + l.ambient_intensity);
        let cells = grid.choose(&self.terrain, target, far, sun, &lights, n);
        let gpu: Vec<w3_render::lights::GpuLight> = lights.iter().map(|l| l.gpu()).collect();
        self.models.lights.write(queue, &gpu, &cells);
        globals.light_grid = grid.uniform();
        queue.write_buffer(&self.models.globals_buf, 0, bytemuck::bytes_of(&globals));
        // The world shows inside the same frame in cinematics; the panel covers the rest.
        if self.hud.is_some() {
            // The world shows only inside the game interface's world frame, which CGameUI
            // places from 0.13 to 0.58 of the 0.6-tall screen (game.dll 0x6f367710).
            let top = ((0.6 - 0.58) / 0.6 * size.1 as f32) as u32;
            let bottom = ((0.6 - 0.13) / 0.6 * size.1 as f32) as u32;
            pass.set_scissor_rect(0, top, size.0, bottom - top);
        }
        if self.sky.is_some() {
            // Unfogged and unhidden by the fog of war.
            let sky_globals = w3_render::model::Globals { fow_extent: [0.0; 4], ..globals };
            self.sky_renderer.render(device, queue, &mut pass, &sky_globals);
        }
        self.terrain.render_ground(&mut pass);
        let mut decals = self.ubersplat_decals(device, queue, assets);
        decals.extend(self.image_decals(device, queue, assets));
        decals.extend(self.shadow_decals(device, queue, assets));
        decals.extend(self.selection_decals(device, queue, assets));
        decals.extend(self.area_cursor_decal(device, queue, assets));
        decals.extend(self.placement_decals(device, queue, assets));
        let terrain = &self.terrain;
        self.decals.render(device, queue, &mut pass, &decals, &|x, y| terrain.height_at(x, y));
        self.models.render(device, queue, &mut pass, &globals);
        self.terrain.render_water(&mut pass, self.time);
        drop(pass);
        let mut tag_draws = self.health_bar_draws(&assets.frames, size);
        tag_draws.extend(self.text_tag_draws(&assets.frames, size));
        self.screen = size;
        self.sync_hud(&assets.frames, size.0 as f32 / size.1.max(1) as f32);
        if let (Some(hud), Some(_)) = (self.hud.as_mut(), self.game.as_ref()) {
            // Text tags lie under the interface.
            let mut draws = tag_draws;
            draws.extend(hud.draws(&assets.frames));
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
            let cine_rect = if hud.cinematic { hud.cine_portrait_rect() } else { None };
            let rect = hud.portrait_rect();
            let cinematic = hud.cinematic;
            let (width, root) = (hud.tree.width, hud.tree.frames[0].rect.unwrap_or_default());
            let tod = self.game.as_ref().map_or((0.0, 24.0, 60.0), |g| (g.world.time_of_day, g.world.misc_f32("DayHours", 24.0), g.world.misc_f32("ScaledAnimTime", 60.0)));
            if cinematic {
                if let Some(r) = cine_rect {
                    self.render_portrait(device, queue, encoder, color, depth, size, r, true);
                }
            } else {
                self.render_time_indicator(device, queue, encoder, color, depth, assets, width, root, tod);
                self.render_portrait(device, queue, encoder, color, depth, size, rect, false);
            }
        }
    }

    /// CTimeOfDayIndicator: the TimeOfDayIndicator model with its origin on the
    /// interface's lower left, its Stand animation at (time of day / DayHours) x
    /// ScaledAnimTime seconds (FUN_6f3d35f0), drawn as CSpriteFrame draws.
    #[allow(clippy::too_many_arguments)]
    fn render_time_indicator(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, color: &wgpu::TextureView, depth: &wgpu::TextureView, assets: &mut Assets, width: f32, root: w3_ui::Rect, (tod, hours, scaled): (f32, f32, f32)) {
        if self.time_indicator.is_none() {
            let path = assets.frames.decorate("TimeOfDayIndicator");
            let Some(m) = assets.model(device, queue, &path) else { return };
            let mut inst = ModelInstance::new(m);
            inst.sequence = w3_render::anim::find_sequence(&inst.model.mdx, "Stand");
            self.time_indicator = Some(inst);
        }
        let Some(inst) = self.time_indicator.as_mut() else { return };
        if let Some(s) = inst.sequence.and_then(|s| inst.model.mdx.sequences.get(s)) {
            let len = (s.end - s.start) as f32;
            inst.frame = ((tod / hours.max(1e-3)) * scaled * 1000.0).rem_euclid(len.max(1.0));
            inst.finished = false;
            inst.looped = false;
        }
        inst.transform = Mat4::from_translation(Vec3::new(root.x0, root.y0, 0.0));
        let ortho = Mat4::orthographic_rh(0.0, width, 0.0, w3_ui::UI_HEIGHT, -500.0, 500.0);
        let mut globals = default_globals(ortho, Vec3::new(0.0, 0.0, 50.0));
        globals.light_dir = [0.0, 0.0, -1.0, 0.0];
        globals.ambient = [1.0, 1.0, 1.0, 1.0];
        self.hud_sprites.submit(inst, Vec3::new(0.0, 0.0, 50.0), Quat::IDENTITY);
        // The cursor (war3skins Cursor): Scroll with the edge scrolling, Target while
        // a target is awaited (TargetSelect over a unit), Select over a unit, Normal
        // otherwise; its origin is the pointer.
        let pointer = self.hud.as_ref().map(|h| h.pointer);
        if let Some((px, py)) = pointer.filter(|p| p.0 >= 0.0) {
            if self.cursor_model.is_none() {
                let path = assets.frames.skin_value("Cursor").unwrap_or_default();
                if let Some(m) = assets.model(device, queue, path.trim_matches('"')) {
                    self.cursor_model = Some(ModelInstance::new(m));
                }
            }
            let over_unit = self.screen.0 > 0 && self.pick_unit(self.cursor.0, self.cursor.1, self.screen).is_some();
            let targeting = self.targeting.is_some() || self.item_target.is_some();
            let held_icon = self.held_item.and_then(|(u, slot)| {
                let g = self.game.as_ref()?;
                let item = g.world.unit(u)?.inventory.get(slot).copied()?;
                let t = g.world.item(item)?.type_id;
                g.world.objects.items.get(t).map(|o| o.str("Art"))
            });
            let seq = match self.scroll_dir {
                (0, 1) => "Scroll Up",
                (0, -1) => "Scroll Down",
                (-1, 0) => "Scroll Left",
                (1, 0) => "Scroll Right",
                (-1, 1) => "Scroll Up Left",
                (1, 1) => "Scroll Up Right",
                (-1, -1) => "Scroll Down Left",
                (1, -1) => "Scroll Down Right",
                _ if held_icon.is_some() => "HoldItem",
                _ if targeting && over_unit => "TargetSelect",
                _ if targeting => "Target",
                _ if over_unit => "Select",
                _ => "Normal",
            };
            let icon = held_icon.map(|p| assets.textures.get_or_white(device, queue, &p));
            if let Some(c) = self.cursor_model.as_mut() {
                // The held item's icon is the cursor's replaceable texture 21.
                match icon {
                    Some(t) => {
                        c.replaceable.insert(21, t);
                    }
                    None => {
                        c.replaceable.remove(&21);
                    }
                }
                if self.cursor_sequence != seq {
                    self.cursor_sequence = seq.to_string();
                    c.play(seq);
                }
                c.transform = Mat4::from_translation(Vec3::new(px, py, 0.0));
            }
        }
        // Minimap pings: MinimapIndicator at each point on the minimap.
        let now = self.time;
        self.pings.retain(|p| p.2 > now);
        let path = assets.frames.skin_value("MinimapIndicator").unwrap_or_default();
        for k in 0..self.pings.len() {
            let (x, y, _, color, _) = self.pings[k];
            let Some((mx, my)) = self.hud.as_ref().and_then(|h| h.world_to_minimap(x, y)) else { continue };
            if self.pings[k].4.is_none() {
                if let Some(m) = assets.model(device, queue, path.trim_matches('"')) {
                    let mut fx = ModelInstance::new(m);
                    fx.play("stand");
                    self.pings[k].4 = Some(fx);
                }
            }
            if let Some(fx) = self.pings[k].4.as_mut() {
                fx.transform = Mat4::from_translation(Vec3::new(mx, my, 0.0));
                fx.tint = glam::Vec4::from_array(color);
                self.hud_sprites.submit(fx, Vec3::new(0.0, 0.0, 50.0), Quat::IDENTITY);
            }
        }
        // QuestChangedParticles over the Quests button while it shows.
        let quest_button = self.hud.as_ref().filter(|h| h.quest_indicator.is_some_and(|t| h.now < t)).and_then(|h| h.tree.get("UpperButtonBarQuestsButton").and_then(|i| h.tree.frames[i].rect));
        match quest_button {
            Some(r) => {
                if self.quest_fx.is_none() {
                    let path = assets.frames.skin_value("QuestChangedParticles").unwrap_or_default();
                    if let Some(m) = assets.model(device, queue, path.trim_matches('"')) {
                        let mut fx = ModelInstance::new(m);
                        fx.play("stand");
                        self.quest_fx = Some(fx);
                    }
                }
                if let Some(fx) = self.quest_fx.as_mut() {
                    fx.transform = Mat4::from_translation(Vec3::new(r.x0, r.y0, 0.0));
                    self.hud_sprites.submit(fx, Vec3::new(0.0, 0.0, 50.0), Quat::IDENTITY);
                }
            }
            None => self.quest_fx = None,
        }
        // CommandButtonAutocast over each button whose autocast is on, from its lower left.
        let slots = self.hud.as_ref().map(|h| h.autocast_slots()).unwrap_or_default();
        self.autocast_fx.retain(|k, _| slots.iter().any(|s| (s.0, s.1) == *k));
        for (sx, sy, r) in slots {
            if !self.autocast_fx.contains_key(&(sx, sy)) {
                let path = assets.frames.skin_value("CommandButtonAutocast").unwrap_or_default();
                let Some(m) = assets.model(device, queue, path.trim_matches('"')) else { continue };
                let mut fx = ModelInstance::new(m);
                fx.play("stand");
                self.autocast_fx.insert((sx, sy), fx);
            }
            let fx = self.autocast_fx.get_mut(&(sx, sy)).unwrap();
            fx.transform = Mat4::from_translation(Vec3::new(r.x0, r.y0, 0.0));
            self.hud_sprites.submit(fx, Vec3::new(0.0, 0.0, 50.0), Quat::IDENTITY);
        }
        // The cursor over everything else.
        if let Some(c) = self.cursor_model.as_mut() {
            self.hud_sprites.submit(c, Vec3::new(0.0, 0.0, 50.0), Quat::IDENTITY);
        }
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("console sprites"),
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
        self.hud_sprites.render(device, queue, &mut pass, &globals);
    }

    /// Draws the portrait model through its own camera in the console's portrait slot.
    #[allow(clippy::too_many_arguments)]
    fn render_portrait(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, color: &wgpu::TextureView, depth: &wgpu::TextureView, size: (u32, u32), rect: w3_ui::Rect, cine: bool) {
        let width = self.hud.as_ref().map_or(w3_ui::UI_WIDTH, |h| if cine { h.cine.as_ref().map_or(h.tree.width, |c| c.width) } else { h.tree.width });
        let view = if cine { self.cine_portrait.as_mut().map(|p| &mut p.1) } else { self.portrait.as_mut().map(|p| &mut p.1) };
        let Some(v) = view else { return };
        let Some(cam) = v.inst.model.mdx.cameras.first().cloned() else { return };
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
        // The model camera's field of view spans the viewport's diagonal (matched against
        // the original's portraits), like the game camera's.
        let half_h = (cam.fov / 2.0).tan() / (1.0 + aspect * aspect).sqrt();
        let proj = Mat4::perspective_rh(2.0 * half_h.atan(), aspect, cam.near_clip.max(1.0), cam.far_clip.max(100.0));
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

/// UI\\TipStrings.txt [General]: TipCount tips, Tip1 on.
fn load_tips(chain: &w3_mpq::Chain) -> Vec<String> {
    let p = w3_formats::profile::Profile::parse(&chain.read_string("UI\\TipStrings.txt").unwrap_or_default());
    let n: usize = p.get_str("General", "TipCount").and_then(|v| v.trim().parse().ok()).unwrap_or(0);
    (1..=n).filter_map(|k| p.get_str("General", &format!("Tip{k}"))).map(|t| t.trim_matches('"').to_string()).collect()
}

/// The map's cells for the model lights (w3_render::lights): the first cell's indices
/// and the count across and up.
struct LightGrid {
    first: (i32, i32),
    size: (i32, i32),
}

impl LightGrid {
    fn new(t: &TerrainRenderer) -> LightGrid {
        use w3_render::lights::cell_of;
        let (x0, y0) = (t.offset[0], t.offset[1]);
        let (x1, y1) = (x0 + t.width.saturating_sub(1) as f32 * 128.0, y0 + t.height.saturating_sub(1) as f32 * 128.0);
        let first = (cell_of(x0), cell_of(y0));
        let size = (cell_of(x1) - first.0 + 1, cell_of(y1) - first.1 + 1);
        if (size.0 * size.1) as usize > w3_render::lights::MAX_CELLS {
            return LightGrid { first, size: (0, 0) };
        }
        LightGrid { first, size }
    }

    fn index(&self, i: i32, j: i32) -> Option<u32> {
        let (a, b) = (i - self.first.0, j - self.first.1);
        (a >= 0 && b >= 0 && a < self.size.0 && b < self.size.1).then(|| (b * self.size.0 + a) as u32)
    }

    fn cell(&self, p: Vec3) -> u32 {
        use w3_render::lights::{NO_LIGHT, cell_of};
        self.index(cell_of(p.x), cell_of(p.y)).unwrap_or(NO_LIGHT)
    }

    fn uniform(&self) -> [f32; 4] {
        [self.first.0 as f32, self.first.1 as f32, self.size.0 as f32, self.size.1 as f32]
    }

    /// Every cell's lights: those within `far` of `target` chosen at their points (on
    /// the ground), the rest the day/night light alone.
    fn choose(&self, t: &TerrainRenderer, target: Vec3, far: f32, sun: f32, lights: &[w3_render::lights::ModelLight], n: usize) -> Vec<[u32; w3_render::lights::PER_CELL]> {
        use w3_render::lights::{CELL, NO_LIGHT, PER_CELL, SUN, choose};
        let mut only_sun = [NO_LIGHT; PER_CELL];
        only_sun[0] = SUN;
        let mut out = vec![only_sun; (self.size.0 * self.size.1).max(0) as usize];
        if lights.is_empty() {
            return out;
        }
        for b in 0..self.size.1 {
            for a in 0..self.size.0 {
                let (x, y) = ((self.first.0 + a) as f32 * CELL, (self.first.1 + b) as f32 * CELL);
                if Vec3::new(x, y, target.z).distance(target) > far + CELL {
                    continue;
                }
                let p = Vec3::new(x, y, t.height_at(x, y));
                out[(b * self.size.0 + a) as usize] = choose(p, sun, lights, n);
            }
        }
        out
    }
}
