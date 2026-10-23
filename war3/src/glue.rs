//! The glue screens: the menus shown outside a game.  Each screen is a frame from the
//! UI\FrameDef\Glue definitions over the 3D menu background, with its buttons wired as
//! game.dll's glue classes wire them (CMainMenu, CSinglePlayerMenu, CSkirmish).

use std::collections::HashMap;

use glam::{Mat4, Quat, Vec3};
use w3_render::model::{ModelInstance, ModelRenderer, default_globals};
use w3_ui::{Draw, FrameTree};

use crate::data::{GameData, Version};
use crate::scene::Assets;

/// What the glue asks the application to do.
#[derive(Clone, Debug)]
pub enum GlueAction {
    Quit,
    /// Start a game on a map file with the setup screen's choices.
    Play(std::path::PathBuf, Vec<w3_game::LobbySlot>),
    /// A campaign mission, by its path in the game's archives.
    Campaign(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    MainMenu,
    SinglePlayer,
    Skirmish,
    /// CCampaignMenu: choosing a campaign and mission.
    Campaign,
    /// The loading screen shown while a map loads.
    Loading,
}

impl Screen {
    /// The animation word the glue sprite layers use for the screen (game.dll's glue
    /// token list: mainmenu, singleplayer, singleplayerskirmish, ...).
    fn token(self) -> &'static str {
        match self {
            Screen::MainMenu => "MainMenu",
            Screen::SinglePlayer => "SinglePlayer",
            Screen::Skirmish => "SinglePlayerSkirmish",
            Screen::Campaign => "Campaign",
            Screen::Loading => "",
        }
    }

    /// The frame each screen shows (from its .fdf).
    fn frame(self) -> &'static str {
        match self {
            Screen::MainMenu => "MainMenuFrame",
            Screen::SinglePlayer => "SinglePlayerMenu",
            Screen::Skirmish => "Skirmish",
            Screen::Campaign => "CampaignMenu",
            Screen::Loading => "Loading",
        }
    }
}

/// Where a screen change is: the old screen's panels leave, then the new ones arrive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Birth,
    Stand,
    Death(Screen),
}

pub struct Glue {
    pub screen: Screen,
    pub tree: FrameTree,
    phase: Phase,
    background: Option<ModelInstance>,
    /// GlueSpriteLayerTopLeft and GlueSpriteLayerTopRight: the panel models.
    layers: Vec<ModelInstance>,
    /// [MenuZFog] for the background: start, end, and color.
    fog: (f32, f32, [f32; 4]),
    background_renderer: ModelRenderer,
    sprite_renderer: ModelRenderer,
    /// Instances for SPRITE frames, with the model name each was made from.
    sprites: HashMap<usize, (String, ModelInstance)>,
    ui: w3_render::ui::UiRenderer,
    pub action: Option<GlueAction>,
    /// Interface-space pointer position.
    pointer: (f32, f32),
    pub skirmish: crate::skirmish::Skirmish,
    /// Loading progress from 0 to 1, shown by the bar's Birth animation.
    pub progress: f32,
    pub campaigns: crate::campaign::Campaigns,
    pub persist: std::rc::Rc<std::cell::RefCell<w3_game::persist::Persist>>,
    /// The glue's own background and fog, kept while the campaign screen shows a
    /// campaign's backdrop.
    saved_background: Option<(Option<ModelInstance>, (f32, f32, [f32; 4]))>,
    /// The loading screen waits for a key (LOADING_PRESS_A_KEY) once loading is done.
    pub press_a_key: bool,
    /// The loading background's sequence, when its [LoadingScreens] entry names one.
    loading_sequence: Option<usize>,
}

impl Glue {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat, data: &GameData, assets: &mut Assets) -> Glue {
        // CGlueMgr's sprite layers: war3skins.txt GlueSpriteLayerBackground_V<n> (the 3D
        // scene, through its own camera) and the TopLeft and TopRight panel models.
        let layer = |assets: &mut Assets, key: &str| {
            let path = assets.frames.skin_value(key)?;
            let m = assets.model(device, queue, &path)?;
            let mut i = ModelInstance::new(m);
            i.sequence = w3_render::anim::find_sequence(&i.model.mdx, "Stand");
            Some(i)
        };
        let background = layer(assets, "GlueSpriteLayerBackground");
        let layers: Vec<ModelInstance> = ["GlueSpriteLayerTopLeft", "GlueSpriteLayerTopRight"].iter().filter_map(|k| layer(assets, k)).collect();
        // UI\MiscData.txt [MenuZFog]: one group of values per data version.
        let v = data.version as usize;
        let pick = |key: &str, n: usize| -> Vec<f32> {
            let all: Vec<f32> = data.misc.get_list("MenuZFog", key).iter().filter_map(|s| s.trim().parse().ok()).collect();
            all.chunks(n).nth(v).or(all.chunks(n).next()).map(|c| c.to_vec()).unwrap_or_default()
        };
        let (start, end, color) = (pick("Start", 1), pick("End", 1), pick("Color", 4));
        let fog = (
            start.first().copied().unwrap_or(0.0),
            end.first().copied().unwrap_or(0.0),
            if color.len() == 4 { [color[1] / 255.0, color[2] / 255.0, color[3] / 255.0, color[0] / 255.0] } else { [0.0; 4] },
        );
        let mut skirmish = crate::skirmish::Skirmish::new(&data.dir);
        skirmish.load_strings(&data.chain, data.version == Version::FrozenThrone);
        // Without a profile the local player takes LOCAL_PLAYER, as game.dll does.
        skirmish.player_name = assets.frames.string("LOCAL_PLAYER");
        let mut g = Glue {
            screen: Screen::MainMenu,
            tree: FrameTree::new(4.0 / 3.0),
            phase: Phase::Birth,
            background,
            layers,
            fog,
            background_renderer: ModelRenderer::new(device, queue, &mut assets.textures, format),
            sprite_renderer: ModelRenderer::new(device, queue, &mut assets.textures, format),
            sprites: HashMap::new(),
            ui: w3_render::ui::UiRenderer::new(device, format),
            action: None,
            pointer: (0.0, 0.0),
            skirmish,
            progress: 0.0,
            campaigns: crate::campaign::Campaigns::load(&data.chain, &assets.frames),
            persist: data.persist.clone(),
            saved_background: None,
            press_a_key: false,
            loading_sequence: None,
        };
        g.build(Screen::MainMenu, assets);
        g.play_layers(&format!("{} Birth", Screen::MainMenu.token()));
        g
    }

    fn play_layers(&mut self, name: &str) {
        for l in &mut self.layers {
            if let Some(s) = w3_render::anim::find_sequence(&l.model.mdx, name) {
                l.sequence = Some(s);
                l.frame = 0.0;
                l.finished = false;
                l.looped = false;
            }
        }
    }

    /// Changes screens: the current panels play their Death, then the new screen's
    /// panels their Birth.
    pub fn show(&mut self, screen: Screen, _assets: &mut Assets) {
        if self.screen == screen && self.phase != Phase::Stand {
            return;
        }
        self.play_layers(&format!("{} Death", self.screen.token()));
        self.phase = Phase::Death(screen);
        for f in &mut self.tree.frames {
            f.hovered = false;
            f.pushed = false;
        }
    }

    /// Builds a screen's frames.
    fn build(&mut self, screen: Screen, assets: &mut Assets) {
        self.screen = screen;
        self.tree = FrameTree::new(4.0 / 3.0);
        self.sprites.clear();
        let db = &assets.frames;
        if self.tree.create(db, screen.frame(), 0).is_none() {
            log::warn!("glue frame {} is not defined", screen.frame());
        }
        match screen {
            Screen::MainMenu => {
                // CMainMenu: the logo is MainMenuLogo_V<n> from war3skins.txt; the realm
                // selector stays hidden until Battle.net is chosen.
                if let Some(logo) = db.skin_value("MainMenuLogo") {
                    self.tree.set_model("WarCraftIIILogo", &logo);
                }
                self.tree.set_visible("RealmSelect", false);
            }
            Screen::SinglePlayer => {
                // CSinglePlayerMenu shows the profile panel only from its profile button.
                self.tree.set_visible("ProfilePanel", false);
                let name = self.skirmish.player_name.clone();
                self.tree.set_text("ProfileNameText", &name);
            }
            Screen::Skirmish => self.skirmish.attach(&mut self.tree, db),
            Screen::Campaign => {
                self.tree.set_visible("MissionSelectFrame", false);
                // SlidingDoors (Campaign-Fade) fades to black when a mission starts.
                self.tree.set_visible("SlidingDoors", false);
                if let Some(logo) = db.skin_value("MainMenuLogo") {
                    self.tree.set_model("WarCraftIIILogo", &logo);
                }
                // CCampaignMenu::vf55: Normal and Hard, from the profile's difficulty.
                let hard = self.persist.borrow().difficulty == 2;
                self.tree.set_items("DifficultySelect", vec![db.string("NORMAL"), db.string("HARD")], hard as usize);
            }
            Screen::Loading => {}
        }
        self.tree.bind_points();
        self.tree.layout();
    }

    /// Loading is done (CLoading state 1): the bar asks for a key.
    pub fn loading_done(&mut self, assets: &mut Assets) {
        self.press_a_key = true;
        self.progress = 1.0;
        let text = assets.frames.string("LOADING_PRESS_A_KEY");
        self.tree.set_text("LoadingBarText", &text);
    }

    /// Shows the loading screen for the selected map (CLoading): a melee map shows the
    /// local race's LoadingMeleeBackground with the map, game type, and players; other
    /// maps show their w3i loading screen and texts.
    pub fn show_loading(&mut self, assets: &mut Assets) {
        self.screen = Screen::Loading;
        self.phase = Phase::Stand;
        self.loading_sequence = None;
        self.tree = FrameTree::new(4.0 / 3.0);
        self.sprites.clear();
        let db = &assets.frames;
        if self.tree.create(db, "Loading", 0).is_none() {
            return;
        }
        let summary = self.skirmish.summary.clone().unwrap_or_default();
        let info = &summary.info;
        let text = |s: &str| match s.strip_prefix("TRIGSTR_").and_then(|n| n.parse::<u32>().ok()) {
            Some(n) => summary.strings.get(&n).cloned().unwrap_or_default(),
            None => s.to_string(),
        };
        let melee = info.flags & 0x4 != 0;
        self.tree.set_visible("LoadingMeleePanel", melee);
        self.tree.set_visible("LoadingCustomPanel", !melee);
        if let Some(bar) = db.skin_value("LoadingProgressBar") {
            self.tree.set_model("LoadingBar", &bar);
        }
        let background = if melee {
            // The local player's race picks the skin section.
            let race = self.skirmish.slots.iter().find(|s| s.controller == crate::skirmish::Controller::Local).map_or(0, |s| s.race);
            let section = ["Default", "Human", "Orc", "Undead", "NightElf"][race.min(4)];
            db.skins.get_str(section, "LoadingMeleeBackground").or_else(|| db.skin_value("LoadingMeleeBackground"))
        } else if !info.loading_model.is_empty() {
            Some(text(&info.loading_model))
        } else if info.loading_screen >= 0 {
            self.skirmish.loading_screen(info.loading_screen).map(|(m, seq)| {
                self.loading_sequence = Some(seq);
                m
            })
        } else {
            Some("UI\\Glues\\Loading\\Load-Generic\\Load-Generic.mdl".to_string())
        };
        if let Some(b) = background {
            self.tree.set_model("LoadingBackground", &b);
        }
        self.tree.set_text("LoadingMeleeMapName", &text(&info.name));
        self.tree.set_text("LoadingMeleeGameTypeValue", &db.string("MELEE"));
        self.tree.set_text("LoadingTitleText", &text(&info.loading_title));
        self.tree.set_text("LoadingSubtitleText", &text(&info.loading_subtitle));
        self.tree.set_text("LoadingText", &text(&info.loading_text));
        // One LoadingPlayerSlot per player, stacked in the container.
        if let Some(container) = self.tree.get("LoadingMeleePlayerContainer").filter(|_| melee) {
            let races = ["RANDOM", "HUMAN", "ORC", "UNDEAD", "NIGHT_ELF"];
            let names = ["OPEN", "CLOSED", "COMPUTER_NEWBIE", "COMPUTER_NORMAL", "COMPUTER_INSANE"];
            let playing: Vec<&crate::skirmish::SlotChoice> = self.skirmish.slots.iter().filter(|s| !matches!(s.controller, crate::skirmish::Controller::Open | crate::skirmish::Controller::Closed)).collect();
            for (k, s) in playing.iter().enumerate() {
                let scope = format!("#{k}");
                if self.tree.create_scoped(db, "LoadingPlayerSlot", container, &scope).is_none() {
                    continue;
                }
                use w3_ui::FramePoint as P;
                self.tree.set_point(&format!("LoadingPlayerSlot{scope}"), P::TopLeft, "LoadingMeleePlayerContainer", P::TopLeft, 0.0, -(k as f32) * 0.032);
                let name = match s.controller {
                    crate::skirmish::Controller::Local => self.skirmish.player_name.clone(),
                    crate::skirmish::Controller::Computer(d) => db.string(names[2 + d.clamp(0, 2) as usize]),
                    _ => String::new(),
                };
                self.tree.set_text(&format!("LoadingPlayerSlotName{scope}"), &name);
                self.tree.set_text(&format!("LoadingPlayerSlotRace{scope}"), &db.string(races[s.race.min(4)]));
                self.tree.set_visible(&format!("LoadingPlayerSlotReadyHighlight{scope}"), false);
            }
        }
        self.tree.bind_points();
        self.tree.layout();
    }

    /// Mouse movement in window pixels.
    pub fn pointer_move(&mut self, x: f32, y: f32, size: (u32, u32)) {
        self.pointer = self.to_ui(x, y, size);
        self.tree.pointer_move(self.pointer.0, self.pointer.1);
    }

    pub fn pointer_down(&mut self, assets: &mut Assets) -> bool {
        let (x, y) = self.pointer;
        if self.tree.pointer_down(x, y).is_some() {
            return true;
        }
        if self.screen == Screen::Skirmish {
            self.skirmish.click(&mut self.tree, &assets.frames, x, y);
        }
        if self.screen == Screen::Campaign && self.phase == Phase::Stand {
            let root = self.tree.frames[0].rect.unwrap_or_default();
            let row = self.campaigns.row_at(&self.persist.borrow(), root, x, y);
            match row {
                // A campaign with only its first mission open starts it; otherwise its
                // open missions are listed.
                Some(crate::campaign::Row::Campaign(c)) => {
                    let p = self.persist.borrow();
                    let more = (1..self.campaigns.list[c].as_ref().map_or(0, |e| e.missions.len())).any(|m| self.campaigns.mission_available(&p, c, m));
                    drop(p);
                    if more {
                        self.campaigns.open = Some(c);
                    } else {
                        self.start_mission(c, 0);
                    }
                    return true;
                }
                Some(crate::campaign::Row::Mission(c, m)) => {
                    self.start_mission(c, m);
                    return true;
                }
                _ => {}
            }
        }
        false
    }

    fn start_mission(&mut self, c: usize, m: usize) {
        if let Some(map) = self.campaigns.list.get(c).and_then(|e| e.as_ref()).and_then(|e| e.missions.get(m)) {
            self.action = Some(GlueAction::Campaign(map.map.clone()));
        }
    }

    pub fn pointer_up(&mut self, assets: &mut Assets) -> Option<String> {
        let (x, y) = self.pointer;
        let clicked = self.tree.pointer_up(x, y)?;
        let name = self.tree.frames[clicked].name.clone();
        self.button(&name, assets);
        Some(name)
    }

    /// A key press: control shortcut keys and Escape for Cancel.
    pub fn key(&mut self, c: char, assets: &mut Assets) {
        if c == '\u{1b}' {
            if let Some(i) = self.tree.get("CancelButton").filter(|&i| self.tree.is_visible(i)) {
                let name = self.tree.frames[i].name.clone();
                self.button(&name, assets);
            }
            return;
        }
        if let Some(i) = self.tree.shortcut(c) {
            let name = self.tree.frames[i].name.clone();
            self.button(&name, assets);
        }
    }

    /// Button handlers, by frame name, following the glue classes.
    fn button(&mut self, name: &str, assets: &mut Assets) {
        match (self.screen, name) {
            (Screen::MainMenu, "SinglePlayerButton") => self.show(Screen::SinglePlayer, assets),
            (Screen::MainMenu, "ExitButton") => self.action = Some(GlueAction::Quit),
            (Screen::SinglePlayer, "SkirmishButton") => self.show(Screen::Skirmish, assets),
            (Screen::SinglePlayer, "CancelButton") => self.show(Screen::MainMenu, assets),
            (Screen::Skirmish, "CancelButton") => self.show(Screen::SinglePlayer, assets),
            (Screen::Skirmish, "PlayGameButton") => {
                if let Some(p) = self.skirmish.selected_map() {
                    self.action = Some(GlueAction::Play(p, self.skirmish.lobby()));
                }
            }
            (Screen::Skirmish, other) => self.skirmish.button(&mut self.tree, &assets.frames, other),
            (Screen::SinglePlayer, "CampaignButton") => self.show(Screen::Campaign, assets),
            (Screen::Campaign, "BackButton") => {
                if self.campaigns.open.take().is_none() {
                    self.show(Screen::SinglePlayer, assets);
                }
            }
            (Screen::Campaign, "DifficultySelect") => {
                let sel = self.tree.get("DifficultySelect").map_or(0, |i| self.tree.frames[i].selected);
                self.persist.borrow_mut().difficulty = if sel == 1 { 2 } else { 1 };
            }
            _ => log::debug!("glue button {name} has no handler yet"),
        }
    }

    fn to_ui(&self, x: f32, y: f32, size: (u32, u32)) -> (f32, f32) {
        (x / size.0 as f32 * self.tree.width, (1.0 - y / size.1 as f32) * w3_ui::UI_HEIGHT)
    }

    pub fn update(&mut self, dt: f32, assets: &mut Assets) {
        if let Some(b) = self.background.as_mut() {
            b.advance(dt * 1000.0);
        }
        for (_, s) in self.sprites.values_mut() {
            s.advance(dt * 1000.0);
        }
        let mut done = false;
        for l in &mut self.layers {
            l.advance(dt * 1000.0);
            done |= l.finished || l.looped;
        }
        if self.skirmish.rebuild && self.screen == Screen::Skirmish && self.phase == Phase::Stand {
            self.skirmish.rebuild = false;
            self.build(Screen::Skirmish, assets);
        }
        // Birth and Death sequences run once; then the screen stands or the next arrives.
        match self.phase {
            Phase::Birth if done || self.layers.is_empty() => {
                self.play_layers(&format!("{} Stand", self.screen.token()));
                self.phase = Phase::Stand;
            }
            Phase::Death(next) if done || self.layers.is_empty() => {
                self.build(next, assets);
                self.play_layers(&format!("{} Birth", next.token()));
                self.phase = Phase::Birth;
            }
            _ => {}
        }
    }

    /// The interface takes input only while its screen stands.
    pub fn interactive(&self) -> bool {
        self.phase == Phase::Stand
    }

    /// The background model's camera at its current time: eye, target, roll, and fov.
    fn background_camera(&self) -> Option<(Vec3, Vec3, f32, f32, f32, f32)> {
        let b = self.background.as_ref()?;
        let cam = b.model.mdx.cameras.first()?;
        let time = b.time();
        let globals = &b.model.mdx.global_sequences;
        let t = w3_render::anim::sample(cam.tracks.vec3(b"KCTR"), &time, globals, [0.0; 3]);
        let tt = w3_render::anim::sample(cam.tracks.vec3(b"KTTR"), &time, globals, [0.0; 3]);
        let roll = w3_render::anim::sample(cam.tracks.f32(b"KCRL"), &time, globals, 0.0);
        let eye = Vec3::from_array(cam.position) + Vec3::from_array(t);
        let target = Vec3::from_array(cam.target) + Vec3::from_array(tt);
        Some((eye, target, roll, cam.fov, cam.near_clip, cam.far_clip))
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
        let aspect = size.0 as f32 / size.1.max(1) as f32;
        // The campaign screen shows a campaign's backdrop and fog in place of the glue's.
        if self.screen == Screen::Campaign && self.saved_background.is_none() {
            let pick = self.campaigns.backdrop_campaign(&self.persist.borrow()).and_then(|c| self.campaigns.list[c].clone());
            if let Some(c) = pick {
                let path = assets.frames.skin_value(&c.background);
                let model = path.and_then(|p| assets.model(device, queue, &p));
                if let Some(m) = model {
                    let mut i = ModelInstance::new(m);
                    i.sequence = w3_render::anim::find_sequence(&i.model.mdx, "Stand");
                    let (start, end, _, color) = c.fog;
                    self.saved_background = Some((self.background.replace(i), self.fog));
                    self.fog = (start, end, color);
                }
            }
        } else if self.screen != Screen::Campaign {
            if let Some((b, fog)) = self.saved_background.take() {
                self.background = b;
                self.fog = fog;
            }
        }
        // Background through the model's camera.
        let mut bg_globals = None;
        let loading = self.screen == Screen::Loading;
        if let Some((eye, target, roll, fov, near, far)) = self.background_camera().filter(|_| !loading) {
            let fwd = (target - eye).normalize_or_zero();
            let up = Quat::from_axis_angle(fwd, roll) * Vec3::Z;
            let view = Mat4::look_at_rh(eye, target, up);
            // The field of view spans the diagonal of the 4:3 interface, which is
            // stretched over the window like the rest of the glue.
            let _ = aspect;
            let fov_y = 2.0 * ((fov / 2.0).tan() * 0.6).atan();
            let proj = Mat4::perspective_rh(fov_y, 4.0 / 3.0, near.max(1.0), far.max(100.0));
            let mut g = default_globals(proj * view, eye);
            g.light_dir = [0.3, 0.3, -1.0, 0.0];
            let (fs, fe, fc) = self.fog;
            if fe > fs {
                g.fog = [fs, fe, 1.0, 0.0];
                g.fog_color = fc;
            }
            let rot = Quat::from_mat4(&view.inverse());
            if let Some(b) = self.background.as_mut() {
                b.transform = Mat4::IDENTITY;
                self.background_renderer.submit(b, eye, rot);
            }
            bg_globals = Some(g);
        }
        // SPRITE frames: models in interface units with their origin on the frame's lower
        // left, over the whole window.
        let mut relayout = false;
        let sprites = self.tree.sprites();
        let live: Vec<usize> = sprites.iter().map(|s| s.0).collect();
        self.sprites.retain(|k, _| live.contains(k));
        for (i, path, _, _) in &sprites {
            if self.sprites.get(i).is_some_and(|s| s.0.eq_ignore_ascii_case(path)) {
                continue;
            }
            if let Some(m) = assets.model(device, queue, path) {
                // A sprite frame without a size takes its model's bounds (FUN_6f0f7080,
                // FUN_6f0f6fc0).
                let e = &m.mdx.extent;
                let f = &mut self.tree.frames[*i];
                if f.width.is_none() {
                    f.width = Some(e.max[0] - e.min[0]);
                }
                if f.height.is_none() {
                    f.height = Some(e.max[1] - e.min[1]);
                }
                relayout = true;
                let mut inst = ModelInstance::new(m);
                inst.sequence = w3_render::anim::find_sequence(&inst.model.mdx, "Birth").or_else(|| w3_render::anim::find_sequence(&inst.model.mdx, "Stand"));
                if self.tree.frames[*i].name == "LoadingBackground" {
                    if let Some(k) = self.loading_sequence.filter(|&k| k < inst.model.mdx.sequences.len()) {
                        inst.sequence = Some(k);
                    }
                }
                self.sprites.insert(*i, (path.clone(), inst));
            }
        }
        // CSpriteFrame's default projection: orthographic over the frame with depth from
        // -500 to 500 (FUN_6f1add30: a left-handed projection with z negated, which is
        // the right-handed one), so higher model z is nearer.
        let ortho = Mat4::orthographic_rh(0.0, self.tree.width, 0.0, w3_ui::UI_HEIGHT, -500.0, 500.0);
        let mut sprite_globals = default_globals(ortho, Vec3::new(0.0, 0.0, 50.0));
        sprite_globals.light_dir = [0.0, 0.0, -1.0, 0.0];
        sprite_globals.ambient = [1.0, 1.0, 1.0, 1.0];
        if relayout {
            self.tree.layout();
        }
        let sprites = self.tree.sprites();
        // The panel layers cover the glue root with their origin at its lower left.
        let campaign = self.screen == Screen::Campaign && self.phase != Phase::Death(Screen::SinglePlayer);
        for l in self.layers.iter_mut().filter(|_| !loading && !campaign) {
            l.transform = Mat4::IDENTITY;
            self.sprite_renderer.submit(l, Vec3::new(0.0, 0.0, 50.0), Quat::IDENTITY);
        }
        for (i, _, x, y) in &sprites {
            if let Some((_, inst)) = self.sprites.get_mut(i) {
                // A finished Birth gives way to Stand.
                if inst.finished || inst.looped && inst.model.mdx.sequences.get(inst.sequence.unwrap_or(0)).is_some_and(|s| s.name.starts_with("Birth")) {
                    inst.sequence = w3_render::anim::find_sequence(&inst.model.mdx, "Stand").or(inst.sequence);
                    inst.frame = 0.0;
                    inst.finished = false;
                    inst.looped = false;
                }
                // CLoading steps the bar through its Birth animation as loading goes.
                if self.tree.frames[*i].name == "LoadingBar" {
                    if let Some(s) = w3_render::anim::find_sequence(&inst.model.mdx, "Birth") {
                        let len = inst.model.mdx.sequences[s].end - inst.model.mdx.sequences[s].start;
                        inst.sequence = Some(s);
                        inst.frame = self.progress.clamp(0.0, 1.0) * len as f32;
                        inst.finished = false;
                        inst.looped = false;
                    }
                }
                inst.transform = Mat4::from_translation(Vec3::new(*x, *y, 0.0));
                self.sprite_renderer.submit(inst, Vec3::new(0.0, 0.0, 50.0), Quat::IDENTITY);
            }
        }
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("glue background"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: color,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: depth,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
            if let Some(g) = bg_globals {
                self.background_renderer.render(device, queue, &mut pass, &g);
            }
        }
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("glue sprites"),
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
            self.sprite_renderer.render(device, queue, &mut pass, &sprite_globals);
        }
        if self.skirmish.preview_dirty {
            self.skirmish.preview_dirty = false;
            if let Some(img) = self.skirmish.summary.as_ref().and_then(|s| s.preview.as_ref()) {
                assets.textures.set_dynamic(device, queue, crate::skirmish::MAP_PREVIEW, img.width, img.height, &img.rgba);
            }
        }
        let mut draws: Vec<Draw> = if self.phase == Phase::Stand { self.tree.draw(&assets.frames) } else { Vec::new() };
        if self.screen == Screen::MainMenu && self.phase == Phase::Stand {
            draws.push(self.version_text(assets));
        }
        if self.screen == Screen::Loading {
            if let Some(r) = self.tree.get("MinimapImage").filter(|&i| self.tree.is_visible(i)).and_then(|i| self.tree.frames[i].rect) {
                draws.push(Draw::Image { texture: crate::skirmish::MAP_PREVIEW.into(), rect: r, uv: [0.0, 0.0, 1.0, 1.0], color: [1.0; 4], blend: w3_ui::Blend::Alpha, rotated: false });
            }
        }
        if self.screen == Screen::Skirmish && self.phase == Phase::Stand {
            self.skirmish.draws(&self.tree, &assets.frames, &mut draws);
        }
        if self.screen == Screen::Campaign && self.phase == Phase::Stand {
            let root = self.tree.frames[0].rect.unwrap_or_default();
            let p = self.persist.borrow();
            let hover = self.campaigns.row_at(&p, root, self.pointer.0, self.pointer.1);
            self.campaigns.draws(&p, &assets.frames, root, hover.as_ref(), &mut draws);
        }
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("glue interface"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: color,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: None,
            ..Default::default()
        });
        self.ui.render(device, queue, &mut assets.textures, &mut pass, &draws, self.tree.width, size);
    }

    /// The build number at the lower right, as CMainMenu formats it ("%d.%d.%d.%d":
    /// 1.27.1.7085), 0.013 in from the right edge.
    fn version_text(&self, assets: &Assets) -> Draw {
        let w = self.tree.width;
        Draw::Text {
            font: assets.frames.decorate("MasterFont"),
            size: 0.011,
            rect: w3_ui::Rect { x0: w - 0.3, x1: w - 0.013, y0: 0.0, y1: 0.02 },
            text: "1.27.1.7085".into(),
            color: [1.0; 4],
            justify_h: w3_ui::Justify::End,
            justify_v: w3_ui::Justify::Start,
            shadow: Some(([0.0, 0.0, 0.0, 1.0], 0.001, -0.001)),
        }
    }
}
