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
    /// Start a game on a map file with the setup screen's choices and advanced options.
    Play(std::path::PathBuf, Vec<w3_game::LobbySlot>, u32),
    /// A campaign mission, by its path in the game's archives.
    Campaign(String),
    /// A saved game file.
    LoadSave(std::path::PathBuf),
    /// The same map again with the same setup.
    Restart(std::path::PathBuf, Option<Vec<w3_game::LobbySlot>>),
    /// A replay file to watch.
    WatchReplay(std::path::PathBuf),
    /// A replay to watch (again).
    PlayReplay(Box<w3_game::replay::Replay>),
    /// A network game: its map, this computer's setup, and how it plays.
    PlayNet(std::path::PathBuf, Vec<w3_game::LobbySlot>, std::rc::Rc<std::cell::RefCell<Option<crate::net::Role>>>, u8, u32),
    /// A loaded network game: its map, saved state, role, speed, and this computer's
    /// player.
    PlayNetSave(std::path::PathBuf, Vec<u8>, std::rc::Rc<std::cell::RefCell<Option<crate::net::Role>>>, u8, u8),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    MainMenu,
    SinglePlayer,
    Skirmish,
    /// CCampaignMenu: choosing a campaign and mission.
    Campaign,
    /// CLoadSavedGameScreen: the saved games and the chosen one's map.
    LoadSaved,
    /// The loading screen shown while a map loads.
    Loading,
    /// CScoreScreen: the players' scores after a game.
    Score,
    /// COptionsMenu: the player's options.
    Options,
    /// CCustomCampaignMenu: choosing a custom campaign from the Campaigns folder.
    CustomCampaign,
    /// CViewReplayScreen: the kept replays.
    ViewReplay,
    /// CLocalMultiplayerJoin, CLocalMultiplayerCreate, and CGameChatroom: the local
    /// network's games, hosting one, and a game's setup.
    LanJoin,
    LanCreate,
    LanLobby,
    /// CLocalMultiplayerLoad: a saved network game to host.
    LanLoad,
    /// CModelCinematic: a model played as a cinematic after a level (PlayModelCinematic).
    ModelCinematic,
}

impl Screen {
    /// The animation word the glue sprite layers use for the screen (game.dll's glue
    /// token list: mainmenu, singleplayer, singleplayerskirmish, ...).
    fn token(self) -> &'static str {
        match self {
            Screen::MainMenu => "MainMenu",
            Screen::SinglePlayer => "SinglePlayer",
            Screen::Skirmish => "SinglePlayerSkirmish",
            // The panels' layout for the saved games screen matches the original's with
            // these sequences (compared against a capture).
            Screen::LoadSaved => "BattlenetCustomCreate",
            Screen::Campaign => "Campaign",
            Screen::Options => "Options",
            // Laid out as the saved games screen is.
            Screen::CustomCampaign | Screen::ViewReplay => "BattlenetCustomCreate",
            Screen::LanJoin => "BattlenetCustom",
            Screen::LanCreate | Screen::LanLoad => "BattlenetCustomCreate",
            Screen::LanLobby => "MultiplayerPreGameChat",
            Screen::Loading | Screen::Score | Screen::ModelCinematic => "",
        }
    }

    /// The frame each screen shows (from its .fdf).
    fn frame(self) -> &'static str {
        match self {
            Screen::MainMenu => "MainMenuFrame",
            Screen::SinglePlayer => "SinglePlayerMenu",
            Screen::Skirmish => "Skirmish",
            Screen::LoadSaved => "LoadSavedGameScreen",
            Screen::Campaign => "CampaignMenu",
            Screen::Loading => "Loading",
            Screen::Score => "ScoreScreenFrame",
            Screen::Options => "OptionsMenu",
            Screen::CustomCampaign => "CustomCampaignMenu",
            Screen::ViewReplay => "ViewReplayScreen",
            Screen::LanJoin => "LocalMultiplayerJoin",
            Screen::LanCreate => "LocalMultiplayerCreate",
            Screen::LanLobby => "GameChatroom",
            Screen::LanLoad => "LocalMultiplayerLoad",
            Screen::ModelCinematic => "",
        }
    }
}

/// The score screen's state: the scores, the tab, each tab's sorting column, whether
/// teams are grouped, the map that follows (ChangeLevel), and the rows' bar models after
/// the first (whose bar is the FDF's ScoreScreenBarTop).
pub struct ScoreState {
    pub data: crate::scorescreen::ScoreData,
    pub tab: usize,
    pub sort: [usize; 4],
    pub teams: bool,
    pub ally_colors: bool,
    pub next: Option<String>,
    /// The screen to go back to.
    prev: Screen,
    bars: Vec<(String, ModelInstance)>,
}

/// Where a screen change is: the old screen's panels leave, then the new ones arrive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Birth,
    Stand,
    Death(Screen),
}

pub struct Glue {
    /// Drawn over the menus (the screen capture's message).
    pub overlay: Vec<w3_ui::Draw>,
    pub screen: Screen,
    pub tree: FrameTree,
    phase: Phase,
    /// The options' panel: whether it has come out (a page was opened), and the sequence
    /// that follows its morph once that is done.
    options_panel: bool,
    panel_next: Option<String>,
    background: Option<ModelInstance>,
    /// GlueSpriteLayerTopLeft and GlueSpriteLayerTopRight: the panel models.
    layers: Vec<ModelInstance>,
    /// [MenuZFog] for the background: start, end, and color.
    fog: (f32, f32, [f32; 4]),
    background_renderer: ModelRenderer,
    sprite_renderer: ModelRenderer,
    /// The cursor (war3skins Cursor), drawn over the interface.
    cursor_renderer: ModelRenderer,
    cursor: Option<ModelInstance>,
    cursor_clock: std::time::Instant,
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
    /// Where saved games are kept.
    pub saves_dir: std::path::PathBuf,
    pub score: Option<ScoreState>,
    /// Sequences SPRITE and MODEL frames play in place of Birth and Stand, by frame name.
    sprite_sequences: HashMap<String, String>,
    /// The player's options, as the options screen shows them while it is open (its
    /// sound settings take effect at once; Cancel puts back the stored ones).
    pub options: crate::options::Options,
    /// The display's size in pixels, which the options screen lists as its resolution.
    pub display: (u32, u32),
    /// The game's archives, where a custom campaign is mounted while it is open.
    chain: std::sync::Arc<w3_mpq::Chain>,
    /// The Campaigns folder.
    campaigns_dir: std::path::PathBuf,
    /// The Single Player menu's profiles.
    pub profile: crate::profiles::ProfilePanel,
    /// What a DialogWar3 message's Yes does, while one shows.
    dialog: Option<DialogAction>,
    /// The score screen's Save Replay panel.
    save_replay: crate::savereplay::SaveReplayPanel,
    /// The local network screens.
    pub lan: crate::lan::Lan,
    /// A network game's players (name and race) for its loading screen.
    lan_players: Option<Vec<(String, usize)>>,
    /// A model cinematic playing, or waiting for its model to load.
    cinematic: Option<Cinematic>,
    /// Sound labels the glue asks the application to play.
    pub sounds: Vec<String>,
    /// Stop the cinematic scene's sound (a new scene, or the cinematic's end).
    pub stop_scene_sound: bool,
    /// UI\MiscData.txt, for a cinematic's fog and sounds.
    misc: w3_formats::profile::Profile,
    version: crate::data::Version,
}

/// A model cinematic (CModelCinematic, FUN_6f30b790): its sequences in order, each
/// through the camera of the same number, the glue's background put aside; fog from
/// UI\MiscData.txt [<name>ZFog]; [<name>Sounds] SceneN as sequence N begins
/// (FUN_6f306950); letterboxed 0.11875 above and below (FUN_6f305170).  A key or a
/// click ends it, as does its last sequence; then what the level's end asked for.
struct Cinematic {
    path: String,
    loaded: bool,
    sequence: usize,
    /// The score screen and the next level after it, or nothing (back to the menus).
    after: (Option<crate::scorescreen::ScoreData>, Option<String>),
}

/// A loading screen row's race for an observer and for a referee.
const OBSERVER_ROW: usize = usize::MAX;
const REFEREE_ROW: usize = usize::MAX - 1;

/// What a glue message's Yes (or OK) does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DialogAction {
    DeleteProfile,
    /// Nothing: the message only tells.
    Notice,
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
        // The local player takes the profile's name (LOCAL_PLAYER without one).
        let local = assets.frames.string("LOCAL_PLAYER");
        let profile = crate::profiles::ProfilePanel::new(&local);
        skirmish.player_name = profile.name().unwrap_or(local);
        let lan = crate::lan::Lan::new(&data.dir, skirmish.player_name.clone());
        let mut g = Glue {
            overlay: Vec::new(),
            screen: Screen::MainMenu,
            tree: FrameTree::new(4.0 / 3.0),
            phase: Phase::Birth,
            options_panel: false,
            panel_next: None,
            background,
            layers,
            fog,
            background_renderer: ModelRenderer::new(device, queue, &mut assets.textures, format),
            sprite_renderer: ModelRenderer::new(device, queue, &mut assets.textures, format),
            cursor_renderer: ModelRenderer::new(device, queue, &mut assets.textures, format),
            cursor: None,
            cursor_clock: std::time::Instant::now(),
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
            saves_dir: crate::data::save_dir(data.version).unwrap_or_default(),
            score: None,
            sprite_sequences: HashMap::new(),
            options: crate::options::Options::load(),
            display: (800, 600),
            chain: data.chain.clone(),
            campaigns_dir: data.dir.join("Campaigns"),
            profile,
            dialog: None,
            save_replay: Default::default(),
            lan,
            lan_players: None,
            cinematic: None,
            sounds: Vec::new(),
            stop_scene_sound: false,
            misc: data.misc.clone(),
            version: data.version,
        };
        g.build(Screen::MainMenu, assets);
        g.play_layers(&format!("{} Birth", Screen::MainMenu.token()));
        g
    }

    /// Plays a sequence on the top-left panel alone (the glue's layer 1, FUN_6f306dd0).
    fn play_top_left(&mut self, name: &str) {
        if let Some(l) = self.layers.first_mut() {
            if let Some(s) = w3_render::anim::find_sequence(&l.model.mdx, name) {
                l.sequence = Some(s);
                l.frame = 0.0;
                l.finished = false;
                l.looped = false;
            }
        }
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
        self.options_panel = false;
        self.panel_next = None;
        self.tree = FrameTree::new(4.0 / 3.0);
        self.sprites.clear();
        let db = &assets.frames;
        if !screen.frame().is_empty() && self.tree.create(db, screen.frame(), 0).is_none() {
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
                // CSinglePlayerMenu's vf55: Reign of Chaos has no custom campaigns, and
                // Custom Game takes their place under View Replay.
                if self.version == crate::data::Version::ReignOfChaos {
                    self.tree.set_visible("CustomCampaignBackdrop", false);
                    use w3_ui::FramePoint as P;
                    self.tree.set_point("SkirmishBackdrop", P::TopRight, "ViewReplayBackdrop", P::BottomRight, 0.0, 0.014);
                }
                self.profile.attach(&mut self.tree, db);
            }
            Screen::Skirmish => {
                self.skirmish.detach_saves();
                self.skirmish.attach(&mut self.tree, db);
                crate::advanced::attach(&mut self.tree, db, self.skirmish.flags, true);
            }
            Screen::LoadSaved => {
                let dir = self.saves_dir.clone();
                self.skirmish.attach_saves(&mut self.tree, db, &dir);
            }
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
            Screen::ViewReplay => {
                self.skirmish.detach_saves();
                let dir = crate::data::replay_dir().unwrap_or_default();
                self.skirmish.attach_replays(&mut self.tree, db, &dir);
            }
            Screen::CustomCampaign => {
                self.skirmish.detach_saves();
                let dir = self.campaigns_dir.clone();
                self.skirmish.attach_campaigns(&mut self.tree, db, &dir);
            }
            Screen::LanJoin => {
                let port = crate::net::port(&self.options);
                self.lan.attach_join(&mut self.tree, db, port);
                self.skirmish.attach_pane(&mut self.tree, db, None);
            }
            Screen::LanCreate => {
                self.skirmish.detach_saves();
                self.skirmish.attach(&mut self.tree, db);
                crate::advanced::attach(&mut self.tree, db, self.skirmish.flags, false);
                self.tree.set_value("GameSpeedSlider", 2.0);
                self.tree.set_text("GameSpeedValue", &db.string("FAST"));
            }
            Screen::LanLoad => {
                let dir = crate::data::multiplayer_save_dir().unwrap_or_default();
                self.skirmish.attach_saves(&mut self.tree, db, &dir);
            }
            Screen::LanLobby => {
                self.lan.rebuild = false;
                self.lan.attach_lobby(&mut self.tree, db);
                let map = self.lan.map();
                self.skirmish.attach_pane(&mut self.tree, db, Some(&map));
            }
            Screen::Options => {
                self.options = crate::options::Options::load();
                crate::options::menu::fill(&mut self.tree, &assets.frames, &self.options, self.display);
            }
            Screen::Loading | Screen::ModelCinematic => {}
            Screen::Score => {
                // Save Replay keeps the game just played (Replay\LastReplay.w3g).
                let last = crate::replays::last_replay().is_some_and(|p| p.is_file());
                self.tree.set_enabled("ScoreScreenSaveReplayButton", last);
                let kept = self.score.as_ref().is_none_or(|s| s.data.replay);
                self.tree.set_visible("ScoreScreenSaveReplayButton", kept);
                self.tree.set_visible("ScoreScreenReplaySavedButton", false);
                self.save_replay = Default::default();
                self.sync_score(db);
            }
        }
        self.tree.bind_points();
        self.tree.fit_text_widths();
        self.tree.layout();
    }

    /// Plays a model cinematic, then shows `score` (going on to `next`), starts `next`,
    /// or goes back to the menus.
    pub fn play_cinematic(&mut self, path: String, score: Option<crate::scorescreen::ScoreData>, next: Option<String>, assets: &mut Assets) {
        self.cinematic = Some(Cinematic { path, loaded: false, sequence: 0, after: (score, next) });
        self.build(Screen::ModelCinematic, assets);
        self.phase = Phase::Stand;
    }

    /// The cinematic's name in UI\MiscData.txt's sections: its file name.
    fn cinematic_name(path: &str) -> String {
        let file = path.rsplit(['\\', '/']).next().unwrap_or(path);
        file.rsplit_once('.').map_or(file, |(n, _)| n).to_string()
    }

    /// Starts sequence `k` with its camera and its scene sound.
    fn cinematic_sequence(&mut self, k: usize) {
        let Some(c) = self.cinematic.as_mut() else { return };
        c.sequence = k;
        let name = Self::cinematic_name(&c.path);
        if let Some(b) = self.background.as_mut() {
            b.sequence = Some(k);
            b.frame = 0.0;
            b.finished = false;
            b.looped = false;
        }
        if let Some(label) = self.misc.get_str(&format!("{name}Sounds"), &format!("Scene{k}")) {
            self.stop_scene_sound = true;
            self.sounds.push(label.trim().trim_matches('"').to_string());
        }
    }

    /// The cinematic is over: the glue's background back, then what follows.
    fn end_cinematic(&mut self, assets: &mut Assets) {
        let Some(c) = self.cinematic.take() else { return };
        self.stop_scene_sound = true;
        if let Some((b, fog)) = self.saved_background.take() {
            self.background = b;
            self.fog = fog;
        }
        match c.after {
            (Some(score), next) => self.show_score(score, next, assets),
            (None, Some(map)) => self.action = Some(GlueAction::Campaign(map)),
            (None, None) => {
                self.build(Screen::MainMenu, assets);
                self.play_layers(&format!("{} Birth", Screen::MainMenu.token()));
                self.phase = Phase::Birth;
            }
        }
    }

    /// Shows the score screen for a finished game; OK goes on to `next` (a ChangeLevel
    /// map) or back to the menus.
    pub fn show_score(&mut self, data: crate::scorescreen::ScoreData, next: Option<String>, assets: &mut Assets) {
        let prev = if self.screen == Screen::Score { Screen::MainMenu } else { self.screen };
        self.score = Some(ScoreState { data, tab: 0, sort: crate::scorescreen::DEFAULT_SORT, teams: false, ally_colors: false, next, prev, bars: Vec::new() });
        self.build(Screen::Score, assets);
        self.phase = Phase::Stand;
    }

    /// Puts the score screen's tab, columns, and art in place.
    fn sync_score(&mut self, db: &w3_ui::FrameDb) {
        use crate::scorescreen::*;
        let Some(s) = self.score.as_ref() else { return };
        let (tab, col) = (s.tab, s.sort[s.tab]);
        let four = tab == 0;
        self.tree.set_visible("ScoreScreen4ColumnFrame", four);
        self.tree.set_visible("ScoreScreen5ColumnFrame", !four);
        let n = if four { 4 } else { 5 };
        for k in 0..n {
            let prefix = if four { "ScoreScreen4Column" } else { "ScoreScreen5Column" };
            let text = db.string(&format!("{}{k}", HEADERS[tab]));
            self.tree.set_text(&format!("{prefix}Header{k}"), &text);
            self.tree.set_checked(&format!("{prefix}Button{k}"), k == col);
        }
        self.tree.set_checked("ScoreScreenTeamsButton", s.teams);
        // The ally color button shows the minimap's ally button for its state
        // (FUN_6f3075e0).
        if let Some(b) = self.tree.get("ScoreScreenTeamColorButton") {
            let state = if s.ally_colors { "Active" } else { "Inactive" };
            let kids: Vec<usize> = self.tree.frames[b].children.clone();
            for c in kids {
                let key = match self.tree.frames[c].def.name.as_str() {
                    "ScoreScreenButtonBackdrop" => "Enabled",
                    "ScoreScreenButtonPushedBackdrop" => "Pushed",
                    "ScoreScreenButtonDisabledBackdrop" => "Disabled",
                    _ => continue,
                };
                self.tree.frames[c].texture = Some(db.decorate(&format!("MiniMapAllyButton{state}{key}")));
            }
        }
        // The chosen tab's name shows larger in its place (ScoreScreenTabTextSelected).
        for k in 0..4 {
            self.tree.set_visible(&format!("ScoreScreenTabText{k}"), k != tab);
        }
        let label = db.string(&format!("SCORESCREEN_TAB{tab}"));
        self.tree.set_text("ScoreScreenTabTextSelected", &label);
        let at = format!("ScoreScreenTabText{tab}");
        self.tree.clear_points("ScoreScreenTabTextSelected");
        self.tree.set_point("ScoreScreenTabTextSelected", w3_ui::FramePoint::Center, &at, w3_ui::FramePoint::Center, 0.0, 0.0);
        self.tree.set_text("ScoreScreenElapsedTimeValue", &elapsed_text(s.data.elapsed));
        // The race's victory art, or the defeat art (FUN_6f3024c0).
        let art = if s.data.victory {
            let race = race_skin(s.data.local_race);
            db.skins.get_str(race, &format!("ScoreScreenVictory_V{}", db.version)).or_else(|| db.skins.get_str(race, "ScoreScreenVictory"))
        } else {
            db.skin_value("ScoreScreenDefeat")
        };
        if let Some(a) = art {
            self.tree.set_model("ScoreScreenRaceArt", &a);
        }
        // The page's tab and column layout, and the first row's team color.
        self.sprite_sequences.insert("ScoreScreenPageTab".into(), format!("Tab{}", tab + 1));
        self.sprite_sequences.insert("ScoreScreenBackground".into(), if four { "Scorescreen-4Button".into() } else { "Scorescreen-5Button".into() });
        let order = s.data.order(tab, col, s.teams);
        // The bars' sequences are taken by the player's color index: their names do not
        // follow the colors (sequence 0, "TeamColor02", is red).
        if let Some(r) = order.first().map(|&k| &s.data.rows[k]) {
            self.sprite_sequences.insert("ScoreScreenBarTop".into(), format!("#{}", r.bar_color(s.ally_colors)));
        }
        self.tree.bind_points();
        self.tree.layout();
    }

    /// The score screen's rows: banner, race icon, team number, name, and values.
    fn score_draws(&self, db: &w3_ui::FrameDb, draws: &mut Vec<Draw>) {
        use crate::scorescreen::*;
        use w3_ui::{Blend, Justify, Rect};
        let Some(s) = self.score.as_ref() else { return };
        let root = self.tree.frames[0].rect.unwrap_or_default();
        let (tab, col) = (s.tab, s.sort[s.tab]);
        let order = s.data.order(tab, col, s.teams);
        let bars = row_bars(order.len(), root.y1);
        let font = db.decorate("MasterFont");
        let shadow = Some(([0.0, 0.0, 0.0, 1.0], 0.002, -0.002));
        let height = |k: &str, d: f32| db.misc_ui.get_str("FontHeights", k).and_then(|v| v.split("//").next().and_then(|v| v.trim().parse().ok())).unwrap_or(d);
        let (normal, large, team_h) = (height("ScoreScreenNormal", 0.011), height("ScoreScreenLarge", 0.011), height("ScoreScreenTeam", 0.009));
        let columns: Vec<f32> = if tab == 0 { COLUMNS_4.to_vec() } else { COLUMNS_5.to_vec() };
        // The highest value of each column shows larger and gold.
        let best: Vec<i32> = (0..columns.len()).map(|c| s.data.rows.iter().map(|r| r.values[tab][c]).max().unwrap_or(0)).collect();
        let image = |texture: String, rect: Rect| Draw::Image { texture, rect, uv: [0.0, 0.0, 1.0, 1.0], color: [1.0; 4], blend: Blend::Alpha, rotated: false };
        for (k, &ri) in order.iter().enumerate() {
            let r = &s.data.rows[ri];
            let (_, top, bottom) = bars[k];
            let (x0, y) = (root.x0, (top + bottom) / 2.0 + row_offset(k, order.len()));
            let banner = if r.won { "ScoreScreenBannerWin" } else { "ScoreScreenBannerLoss" };
            if let Some(t) = db.skin_value(banner) {
                draws.push(image(t, Rect { x0, x1: x0 + 0.2, y0: y - 0.025, y1: y + 0.025 }));
            }
            if let Some(t) = db.skin_value(&format!("ScoreScreenPlayer{}", race_word(r.race))) {
                let cy = y + 0.0015;
                draws.push(image(t, Rect { x0, x1: x0 + 0.03, y0: cy - 0.015, y1: cy + 0.015 }));
            }
            let text = |text: String, rect: Rect, size: f32, color: [f32; 4], jh: Justify| Draw::Text { font: font.clone(), size, rect, text, color, justify_h: jh, justify_v: Justify::Middle, shadow };
            draws.push(text((r.team + 1).to_string(), Rect { x0: x0 + 0.005, x1: x0 + 0.025, y0: y - 0.01, y1: y + 0.01 }, team_h, [1.0; 4], Justify::End));
            // Winners' names are gold, others white; names are cut to 0.11625.
            let name_color = if r.won { [0xee as f32 / 255.0, 0xcc as f32 / 255.0, 0x48 as f32 / 255.0, 1.0] } else { [1.0; 4] };
            let mut name = r.name.clone();
            while name.len() > 1 && db.text_width(&font, normal, &name) > 0.11625 {
                name.pop();
            }
            draws.push(text(name, Rect { x0: x0 + 0.03, x1: x0 + 0.03 + 0.11625, y0: y - 0.01, y1: y + 0.01 }, normal, name_color, Justify::Start));
            for (c, &cx) in columns.iter().enumerate() {
                let center = root.x0 + 0.4 + cx;
                // The heroes column shows their icons and levels.
                if tab == 2 && c == 0 {
                    let xs: &[f32] = match r.heroes.len().min(3) {
                        0 => &[],
                        1 => &[0.2],
                        2 => &[0.1825, 0.2175],
                        _ => &[0.165, 0.2, 0.235],
                    };
                    for (h, &hx) in r.heroes.iter().zip(xs) {
                        let ix = x0 + hx;
                        let cy = y + 0.0015;
                        draws.push(image(h.0.clone(), Rect { x0: ix, x1: ix + 0.03, y0: cy - 0.015, y1: cy + 0.015 }));
                        draws.push(text(h.1.to_string(), Rect { x0: ix + 0.03 - 0.012, x1: ix + 0.06 - 0.012, y0: cy - 0.015, y1: cy + 0.015 }, normal, [1.0; 4], Justify::Start));
                    }
                    continue;
                }
                let v = r.values[tab][c];
                let value = if tab == 3 && c == 2 { format!("{}/{}", r.traded.0, r.traded.1) } else { v.to_string() };
                let top_value = best[c] != 0 && v == best[c] && !(tab == 3 && c == 2);
                let (size, color) = if top_value { (large, [1.0, 0xdb as f32 / 255.0, 0x47 as f32 / 255.0, 1.0]) } else { (normal, [1.0; 4]) };
                draws.push(text(value, Rect { x0: center - 0.05, x1: center + 0.05, y0: y - size / 2.0, y1: y + size / 2.0 }, size, color, Justify::Middle));
            }
        }
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
        // While loading the bar reads LOADING_LOADING (CLoading state 0, FUN_6f304a20).
        self.tree.set_text("LoadingBarText", &db.string("LOADING_LOADING"));
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
            // A network game's players, or the Custom Game screen's.
            let playing: Vec<(String, usize)> = match self.lan_players.take() {
                Some(p) => p,
                None => self
                    .skirmish
                    .slots
                    .iter()
                    .filter_map(|s| {
                        let name = match s.controller {
                            crate::skirmish::Controller::Local => self.skirmish.player_name.clone(),
                            crate::skirmish::Controller::Computer(d) => db.string(names[2 + d.clamp(0, 2) as usize]),
                            _ => return None,
                        };
                        Some((name, s.race))
                    })
                    .collect(),
            };
            for (k, (name, race)) in playing.iter().enumerate() {
                let scope = format!("#{k}");
                if self.tree.create_scoped(db, "LoadingPlayerSlot", container, &scope).is_none() {
                    continue;
                }
                use w3_ui::FramePoint as P;
                self.tree.set_point(&format!("LoadingPlayerSlot{scope}"), P::TopLeft, "LoadingMeleePlayerContainer", P::TopLeft, 0.0, -(k as f32) * 0.032);
                self.tree.set_text(&format!("LoadingPlayerSlotName{scope}"), name);
                let race_text = match *race {
                    OBSERVER_ROW => db.string("OBSERVER"),
                    REFEREE_ROW => db.string("REFEREE"),
                    r => db.string(races[r.min(4)]),
                };
                self.tree.set_text(&format!("LoadingPlayerSlotRace{scope}"), &race_text);
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
        if self.cinematic.is_some() {
            self.end_cinematic(assets);
            return true;
        }
        let (x, y) = self.pointer;
        if self.tree.pointer_down(x, y).is_some() {
            return true;
        }
        if self.screen == Screen::Score {
            self.save_replay.click(&mut self.tree, x, y);
        }
        if self.screen == Screen::SinglePlayer && self.dialog.is_none() {
            if let Some(e) = self.profile.click(&mut self.tree, x, y) {
                self.profile_event(e, assets);
            }
        }
        if matches!(self.screen, Screen::Skirmish | Screen::LoadSaved | Screen::CustomCampaign | Screen::ViewReplay | Screen::LanCreate | Screen::LanLoad) {
            self.skirmish.click(&mut self.tree, &assets.frames, x, y);
        }
        if self.screen == Screen::LanJoin {
            self.lan.click(&mut self.tree, &assets.frames, x, y);
            if let Some(map) = self.lan.summary_due.take() {
                self.skirmish.show_map(&mut self.tree, &assets.frames, Some(&map));
            }
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

    /// Once saved, the score screen's Save Replay gives way to Replay Saved.
    fn replay_saved(&mut self) {
        self.tree.set_visible("ScoreScreenSaveReplayButton", false);
        self.tree.set_visible("ScoreScreenReplaySavedButton", true);
        self.tree.set_enabled("ScoreScreenReplaySavedButton", false);
        self.tree.set_visible("SaveReplayPanel", false);
    }

    /// What the local network screens ask.
    fn lan_event(&mut self, e: crate::lan::Event, assets: &mut Assets) {
        match e {
            crate::lan::Event::Lobby => self.show(Screen::LanLobby, assets),
            crate::lan::Event::Joining(message) => {
                self.show(Screen::LanJoin, assets);
                if let Some(key) = message {
                    let text = assets.frames.string(key);
                    self.show_dialog(&text, false, DialogAction::Notice, assets);
                }
            }
            crate::lan::Event::Start(map, lobby, role, speed, flags) => {
                self.lan.detach();
                let names = ["OPEN", "CLOSED", "COMPUTER_NEWBIE", "COMPUTER_NORMAL", "COMPUTER_INSANE"];
                self.lan_players = Some(
                    self.lan
                        .setup
                        .slots
                        .iter()
                        .filter_map(|s| match s.holder {
                            // An observer's race reads OBSERVER or REFEREE (FUN_6f2fb430).
                            crate::net::Holder::Human(_) if s.team == crate::lan::OBSERVERS => Some((s.name.clone(), if self.lan.setup.flags & 0x400000 != 0 { REFEREE_ROW } else { OBSERVER_ROW })),
                            crate::net::Holder::Human(_) => Some((s.name.clone(), s.race)),
                            crate::net::Holder::Computer(d) => Some((assets.frames.string(names[2 + d.clamp(0, 2) as usize]), s.race)),
                            _ => None,
                        })
                        .collect(),
                );
                self.action = Some(GlueAction::PlayNet(map, lobby, std::rc::Rc::new(std::cell::RefCell::new(Some(role))), speed, flags));
            }
            crate::lan::Event::StartSaved(map, save, role, speed, me) => {
                self.lan.detach();
                self.lan_players = Some(self.lan.setup.slots.iter().filter(|s| matches!(s.holder, crate::net::Holder::Human(_))).map(|s| (s.name.clone(), s.race)).collect());
                self.action = Some(GlueAction::PlayNetSave(map, save, std::rc::Rc::new(std::cell::RefCell::new(Some(role))), speed, me));
            }
        }
    }

    /// The profile panel's requests: another profile chosen (its progress and saved
    /// games become the game's), or Delete's confirmation.
    fn profile_event(&mut self, e: crate::profiles::Event, assets: &mut Assets) {
        match e {
            crate::profiles::Event::Chosen => {
                *self.persist.borrow_mut() = crate::data::load_persist(self.version);
                self.saves_dir = crate::data::save_dir(self.version).unwrap_or_default();
                self.skirmish.player_name = self.profile.name().unwrap_or_else(|| assets.frames.string("LOCAL_PLAYER"));
            }
            crate::profiles::Event::ConfirmDelete(name) => {
                let text = assets.frames.string("DELETE_PROFILE_MESSAGE").replacen("%s", &name, 1);
                self.show_dialog(&text, true, DialogAction::DeleteProfile, assets);
            }
        }
    }

    /// A DialogWar3 message over the screen (FUN_6f2bc510): OK, or Yes and No.
    fn show_dialog(&mut self, text: &str, yes_no: bool, action: DialogAction, assets: &mut Assets) {
        if self.tree.get("DialogWar3").is_none() {
            self.tree.create(&assets.frames, "DialogWar3", 0);
            self.tree.bind_points();
        }
        // CDialogWar3 sits in the middle of the screen.
        use w3_ui::FramePoint as P;
        self.tree.set_point("DialogWar3", P::Center, self.screen.frame(), P::Center, 0.0, 0.0);
        self.tree.set_visible("DialogWar3", true);
        self.tree.set_text("DialogText", text);
        for (n, show) in [("DialogButtonOK", !yes_no), ("DialogButtonYes", yes_no), ("DialogButtonNo", yes_no)] {
            self.tree.set_visible(&format!("{n}Backdrop"), show);
            self.tree.set_visible(n, show);
        }
        self.tree.layout();
        self.dialog = Some(action);
    }

    /// CCustomCampaignMenu's OK (FUN_6f2f7730): the chosen campaign's archive joins the
    /// game's, its difficulty (Normal unless it lets the player choose) is set, and the
    /// campaign screen shows its missions.
    fn open_custom_campaign(&mut self, assets: &mut Assets) {
        let (Some(path), Some(summary)) = (self.skirmish.selected_campaign(), self.skirmish.campaign.clone()) else { return };
        let archive = match w3_mpq::Archive::open(&path) {
            Ok(a) => std::sync::Arc::new(a),
            Err(e) => {
                log::error!("opening {}: {e}", path.display());
                return;
            }
        };
        let hard = summary.info.flags & 1 != 0 && self.tree.get("DifficultySelect").is_some_and(|i| self.tree.frames[i].selected == 1);
        self.persist.borrow_mut().difficulty = if hard { 2 } else { 1 };
        self.chain.unmount(crate::data::PRIORITY_CAMPAIGN);
        self.chain.mount(archive, None, crate::data::PRIORITY_CAMPAIGN);
        let world_edit = self.skirmish.world_edit.clone();
        self.campaigns.open_custom(&path, &summary, &world_edit);
        self.show(Screen::Campaign, assets);
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
        if self.cinematic.is_some() {
            if c == '\u{1b}' || c == ' ' || c == '\r' {
                self.end_cinematic(assets);
            }
            return;
        }
        if self.screen == Screen::Score && self.save_replay.open {
            if let Some(crate::savereplay::Event::Saved) = self.save_replay.key(c, &mut self.tree) {
                self.replay_saved();
            }
            return;
        }
        if self.screen == Screen::SinglePlayer && self.dialog.is_none() {
            if let Some(e) = self.profile.key(c, &mut self.tree) {
                if let Some(e) = e {
                    self.profile_event(e, assets);
                }
                return;
            }
        }
        if matches!(self.screen, Screen::LanJoin | Screen::LanLobby) && c != '\u{1b}' && self.lan.key(c, &mut self.tree, &assets.frames, self.screen == Screen::LanLobby) {
            return;
        }
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
        if let Some(action) = self.dialog {
            if matches!(name, "DialogButtonYes" | "DialogButtonOK" | "DialogButtonNo") {
                self.dialog = None;
                self.tree.set_visible("DialogWar3", false);
                if name != "DialogButtonNo" && action == DialogAction::DeleteProfile {
                    let e = self.profile.delete(&mut self.tree);
                    self.profile_event(e, assets);
                }
            }
            return;
        }
        if self.screen == Screen::SinglePlayer {
            if let Some(e) = self.profile.button(name, &mut self.tree) {
                self.profile_event(e, assets);
                return;
            }
        }
        match (self.screen, name) {
            (Screen::MainMenu, "SinglePlayerButton") => self.show(Screen::SinglePlayer, assets),
            (Screen::MainMenu, "ExitButton") => self.action = Some(GlueAction::Quit),
            (Screen::MainMenu, "OptionsButton") => self.show(Screen::Options, assets),
            // The credits are a map: UI\MiscUI.txt's CreditsFile_V<n> (FUN_6f2eb220).
            (Screen::MainMenu, "CreditsButton") => {
                let db = &assets.frames;
                if let Some(map) = db.misc_ui.get_str("Misc", &format!("CreditsFile_V{}", db.version)) {
                    self.action = Some(GlueAction::Campaign(map));
                }
            }
            (Screen::Options, "GameplayButton" | "VideoButton" | "SoundButton") => {
                // The first page opened brings the top-left panel out: its Options Morph,
                // then Options Stand Alternate (FUN_6f2aa3a0 with the glue's morph and
                // alternate tokens).
                if !self.options_panel {
                    self.options_panel = true;
                    self.play_top_left("Options Morph");
                    self.panel_next = Some("Options Stand Alternate".to_string());
                }
                let page = match name {
                    "GameplayButton" => 0,
                    "VideoButton" => 1,
                    _ => 2,
                };
                crate::options::menu::show(&mut self.tree, page);
            }
            (Screen::Options, "OKButton") => {
                crate::options::menu::read(&self.tree, &mut self.options);
                self.options.store();
                self.show(Screen::MainMenu, assets);
            }
            (Screen::Options, "CancelButton") => {
                self.options = crate::options::Options::load();
                self.show(Screen::MainMenu, assets);
            }
            (Screen::Options, _) => {
                // The sound settings are heard at once (COptionsMenu vf3 cases 6 to 9).
                crate::options::menu::sync(&mut self.tree);
                crate::options::menu::read(&self.tree, &mut self.options);
            }
            (Screen::SinglePlayer, "SkirmishButton") => self.show(Screen::Skirmish, assets),
            (Screen::SinglePlayer, "CancelButton") => self.show(Screen::MainMenu, assets),
            (Screen::Skirmish, "CancelButton") => self.show(Screen::SinglePlayer, assets),
            (Screen::Skirmish, "PlayGameButton") => {
                if let Some(p) = self.skirmish.selected_map() {
                    self.action = Some(GlueAction::Play(p, self.skirmish.lobby(), self.skirmish.flags));
                }
            }
            (Screen::Skirmish | Screen::LanCreate, "MapInfoButton") => crate::advanced::show(&mut self.tree, true),
            (Screen::Skirmish | Screen::LanCreate, "AdvancedOptionsButton") => crate::advanced::show(&mut self.tree, false),
            (Screen::Skirmish, other) => {
                self.skirmish.button(&mut self.tree, &assets.frames, other);
                self.skirmish.flags = crate::advanced::read(&self.tree);
            }
            (Screen::SinglePlayer, "CampaignButton") => self.show(Screen::Campaign, assets),
            (Screen::SinglePlayer, "LoadSavedButton") => self.show(Screen::LoadSaved, assets),
            (Screen::LoadSaved, "CancelButton") => self.show(Screen::SinglePlayer, assets),
            (Screen::LoadSaved, "OKButton") => {
                if let Some(path) = self.skirmish.selected_save() {
                    self.action = Some(GlueAction::LoadSave(path));
                }
            }
            (Screen::SinglePlayer, "CustomCampaignButton") => self.show(Screen::CustomCampaign, assets),
            (Screen::SinglePlayer, "ViewReplayButton") => self.show(Screen::ViewReplay, assets),
            (Screen::ViewReplay, "CancelButton") => self.show(Screen::SinglePlayer, assets),
            (Screen::ViewReplay, "OKButton") => {
                if let Some(path) = self.skirmish.selected_replay() {
                    self.action = Some(GlueAction::WatchReplay(path));
                }
            }
            (Screen::CustomCampaign, "CancelButton") => self.show(Screen::SinglePlayer, assets),
            (Screen::CustomCampaign, "OKButton") => self.open_custom_campaign(assets),
            (Screen::Campaign, "BackButton") if self.campaigns.custom.is_some() => {
                self.campaigns.close_custom();
                self.chain.unmount(crate::data::PRIORITY_CAMPAIGN);
                self.show(Screen::CustomCampaign, assets);
            }
            (Screen::Campaign, "BackButton") => {
                if self.campaigns.open.take().is_none() {
                    self.show(Screen::SinglePlayer, assets);
                }
            }
            (Screen::Campaign, "DifficultySelect") => {
                let sel = self.tree.get("DifficultySelect").map_or(0, |i| self.tree.frames[i].selected);
                self.persist.borrow_mut().difficulty = if sel == 1 { 2 } else { 1 };
            }
            (Screen::Score, _) => self.score_button(name, assets),
            (Screen::MainMenu, "LocalAreaNetworkButton") => self.show(Screen::LanJoin, assets),
            (Screen::LanJoin, "CancelButton") => {
                self.lan.detach();
                self.show(Screen::MainMenu, assets);
            }
            (Screen::LanJoin, "CreateButton") => self.show(Screen::LanCreate, assets),
            (Screen::LanJoin, "LoadButton") => self.show(Screen::LanLoad, assets),
            (Screen::LanLoad, "CancelButton") => self.show(Screen::LanJoin, assets),
            // A saved network game is hosted with its players' slots kept for them.
            (Screen::LanLoad, "OKButton") => {
                let Some(file) = self.skirmish.selected_save() else { return };
                match self.lan.host_saved(&file, self.version == crate::data::Version::FrozenThrone) {
                    Ok(()) => self.show(Screen::LanLobby, assets),
                    Err(key) => {
                        let text = assets.frames.string(key);
                        self.show_dialog(&text, false, DialogAction::Notice, assets);
                    }
                }
            }
            (Screen::LanJoin, "JoinButton") => {
                if let Err(key) = self.lan.join() {
                    let text = assets.frames.string(key);
                    self.show_dialog(&text, false, DialogAction::Notice, assets);
                }
            }
            (Screen::LanCreate, "CancelButton") => self.show(Screen::LanJoin, assets),
            (Screen::LanCreate, "PlayButton") => {
                let (Some(map), Some(summary)) = (self.skirmish.selected_map(), self.skirmish.summary.clone()) else { return };
                let game = assets.frames.string("LOCAL_GAME");
                let speed = self.tree.value("GameSpeedSlider").round().clamp(0.0, 2.0) as u8;
                match self.lan.host_game(&map, &summary, &game, self.version == crate::data::Version::FrozenThrone, speed, self.skirmish.flags) {
                    Ok(()) => self.show(Screen::LanLobby, assets),
                    Err(key) => {
                        let text = assets.frames.string(key);
                        self.show_dialog(&text, false, DialogAction::Notice, assets);
                    }
                }
            }
            (Screen::LanCreate, "GameSpeedSlider") => {
                let speed = self.tree.value("GameSpeedSlider").round().clamp(0.0, 2.0) as u8;
                self.tree.set_text("GameSpeedValue", &assets.frames.string(crate::lan::speed_key(speed)));
            }
            (Screen::LanCreate, other) => {
                self.skirmish.button(&mut self.tree, &assets.frames, other);
                self.skirmish.flags = crate::advanced::read(&self.tree);
            }
            (Screen::LanLobby, "CancelButton") => {
                self.lan.leave();
                self.show(Screen::LanJoin, assets);
            }
            (Screen::LanLobby, "StartGameButton") => {
                self.lan.begin_start();
                self.tree.set_enabled("StartGameButton", false);
            }
            (Screen::LanLobby, other) => self.lan.button(&mut self.tree, &assets.frames, other),
            _ => log::debug!("glue button {name} has no handler yet"),
        }
    }

    fn score_button(&mut self, name: &str, assets: &mut Assets) {
        if self.save_replay.open {
            if let Some(crate::savereplay::Event::Saved) = self.save_replay.button(name, &mut self.tree) {
                self.replay_saved();
            }
            return;
        }
        if name == "ScoreScreenSaveReplayButton" {
            self.save_replay.open(&mut self.tree, &assets.frames);
            return;
        }
        let Some(s) = self.score.as_mut() else { return };
        if let Some(k) = name.strip_prefix("ScoreScreenTabButton").and_then(|k| k.parse::<usize>().ok()) {
            s.tab = k.min(3);
        } else if let Some(k) = name.strip_prefix("ScoreScreen5ColumnButton").or_else(|| name.strip_prefix("ScoreScreen4ColumnButton")).and_then(|k| k.parse::<usize>().ok()) {
            s.sort[s.tab] = k;
        } else if name == "ScoreScreenTeamsButton" {
            s.teams = !s.teams;
        } else if name == "ScoreScreenTeamColorButton" {
            s.ally_colors = !s.ally_colors;
        } else if name == "ScoreScreenOkButton" {
            let (next, prev) = (s.next.take(), s.prev);
            self.score = None;
            self.sprite_sequences.clear();
            match next {
                Some(map) => self.action = Some(GlueAction::Campaign(map)),
                None => {
                    self.build(prev, assets);
                    self.play_layers(&format!("{} Birth", prev.token()));
                    self.phase = Phase::Birth;
                }
            }
            return;
        }
        self.sync_score(&assets.frames);
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
        // A model cinematic goes from sequence to sequence, and ends after the last.
        if let Some(c) = self.cinematic.as_ref().filter(|c| c.loaded) {
            let k = c.sequence;
            let (done, count) = self.background.as_ref().map_or((true, 0), |b| (b.finished || b.looped, b.model.mdx.sequences.len()));
            if done {
                if k + 1 < count {
                    self.cinematic_sequence(k + 1);
                } else {
                    self.end_cinematic(assets);
                }
            }
        }
        // The local network: games found, and the lobby's messages.
        if matches!(self.screen, Screen::LanJoin | Screen::LanLobby) {
            if self.screen == Screen::LanLobby {
                if let Some(e) = self.lan.tick_countdown(&assets.frames, dt) {
                    self.lan_event(e, assets);
                }
                self.sounds.append(&mut self.lan.sounds);
            }
            if let Some(e) = self.lan.update(&mut self.tree, &assets.frames) {
                self.lan_event(e, assets);
            }
            if self.screen == Screen::LanLobby && self.phase == Phase::Stand {
                if self.lan.rebuild {
                    self.build(Screen::LanLobby, assets);
                }
                self.lan.sync_chat(&mut self.tree);
            }
        }
        if self.phase == Phase::Stand && done {
            if let Some(next) = self.panel_next.take() {
                self.play_top_left(&next);
            }
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
        // A cinematic's sequence N is seen through its camera N.
        let k = self.cinematic.as_ref().map_or(0, |c| c.sequence);
        let cam = b.model.mdx.cameras.get(k).or(b.model.mdx.cameras.first())?;
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
    /// The options' explanations (COptionsMenu::vf55's FUN_6f2bb390 and kin) in the glue
    /// tooltip (CGlueToolTipWar3) over the control under the pointer: MasterFont at
    /// FontHeights GlueToolTip, at most 0.22 wide, 0.006 inside the Battle.net tooltip
    /// backdrop, its lower left on the control's upper right and kept on the screen
    /// (CGlueToolTipWar3::vf25).
    fn options_tip_draws(&self, db: &w3_ui::FrameDb, out: &mut Vec<Draw>) {
        use w3_ui::fdf::{Arg, Def, Prop};
        const TIPS: [(&str, &str); 7] = [
            ("GamePortEditBox", "GAMEPORT_INFO"),
            ("ChatSupportMenu", "CHAT_SUPPORT_INFO"),
            ("CustomKeysCheckBox", "CUSTOM_KEYS_INFO"),
            ("HealthBarsCheckBox", "HEALTH_BARS_INFO"),
            ("AutosaveReplayCheckBox", "AUTOSAVE_REPLAY_INFO"),
            ("FormationToggleCheckBox", "FORMATION_TOGGLE_INFO"),
            ("SpellFilterMenu", "SPELL_FILTER_INFO"),
        ];
        let (px, py) = self.pointer;
        let Some((owner, key)) = TIPS.iter().find_map(|&(name, key)| {
            let f = self.tree.get(name).filter(|&f| self.tree.is_visible(f))?;
            self.tree.frames[f].rect.filter(|r| r.contains(px, py)).map(|r| (r, key))
        }) else {
            return;
        };
        let text = db.string(key);
        let font = db.decorate("MasterFont");
        let size = db.misc_ui.get_str("FontHeights", "GlueToolTip").and_then(|v| v.split("//").next().and_then(|v| v.trim().parse().ok())).unwrap_or(0.011f32);
        let lines = db.wrap(&font, size, &text, 0.22);
        let width = lines.iter().map(|l| db.text_width(&font, size, l)).fold(0.0f32, f32::max).min(0.22);
        let (w, h) = (width + 0.012, lines.len() as f32 * size + 0.012);
        // Kept on the screen: moved left of the right edge and below the top.
        let screen_w = self.tree.width.max(w3_ui::UI_WIDTH);
        let x0 = owner.x1.min(screen_w - w);
        let y0 = owner.y1.min(w3_ui::UI_HEIGHT - h);
        let r = w3_ui::Rect { x0, x1: x0 + w, y0, y1: y0 + h };
        let prop = |key: &str, args: Vec<Arg>| Prop { key: key.into(), args };
        let s = |v: &str| Arg::Str(v.into());
        let def = Def {
            block: "Frame".into(),
            kind: "BACKDROP".into(),
            name: String::new(),
            inherits: None,
            with_children: false,
            props: vec![
                prop("BackdropBackground", vec![s("UI\\widgets\\BattleNet\\bnet-tooltip-background.blp")]),
                prop("BackdropTileBackground", vec![]),
                prop("BackdropCornerFlags", vec![s("UL|UR|BL|BR|T|L|B|R")]),
                prop("BackdropCornerSize", vec![Arg::Num(0.008)]),
                prop("BackdropBackgroundInsets", vec![Arg::Num(0.0019); 4]),
                prop("BackdropEdgeFile", vec![s("UI\\widgets\\BattleNet\\bnet-tooltip-border.blp")]),
            ],
            children: vec![],
        };
        self.tree.backdrop(db, &def, r, out);
        out.push(Draw::Text { font, size, rect: w3_ui::Rect { x0: r.x0 + 0.006, x1: r.x1 - 0.006 + 0.001, y0: r.y0 + 0.006, y1: r.y1 - 0.006 }, text: lines.join("\n"), color: [1.0; 4], justify_h: w3_ui::Justify::Start, justify_v: w3_ui::Justify::Start, shadow: None });
    }

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
                let path = c.model.clone().or_else(|| assets.frames.skin_value(&c.background));
                let model = path.and_then(|p| assets.model(device, queue, &p));
                if let Some(m) = model {
                    let mut i = ModelInstance::new(m);
                    i.sequence = w3_render::anim::find_sequence(&i.model.mdx, "Stand");
                    let (start, end, _, color) = c.fog;
                    self.saved_background = Some((self.background.replace(i), self.fog));
                    self.fog = (start, end, color);
                }
            }
        } else if self.screen != Screen::Campaign && self.screen != Screen::ModelCinematic {
            if let Some((b, fog)) = self.saved_background.take() {
                self.background = b;
                self.fog = fog;
            }
        }
        // A model cinematic's model takes the background's place once it loads, with its
        // fog.
        if let Some(path) = self.cinematic.as_ref().filter(|c| !c.loaded).map(|c| c.path.clone()) {
            let name = Self::cinematic_name(&path);
            if let Some(m) = assets.model(device, queue, &path) {
                let inst = ModelInstance::new(m);
                let section = format!("{name}ZFog");
                let num = |k: &str| self.misc.get_str(&section, k).and_then(|v| v.trim().parse::<f32>().ok());
                let color: Vec<f32> = self.misc.get_list(&section, "Color").iter().filter_map(|v| v.trim().parse().ok()).collect();
                let fog = match (num("Start"), num("End")) {
                    (Some(s), Some(e)) if color.len() == 4 => (s, e, [color[1] / 255.0, color[2] / 255.0, color[3] / 255.0, color[0] / 255.0]),
                    _ => (0.0, 0.0, [0.0; 4]),
                };
                self.saved_background = Some((self.background.replace(inst), self.fog));
                self.fog = fog;
            }
            if let Some(c) = self.cinematic.as_mut() {
                c.loaded = true;
            }
            self.cinematic_sequence(0);
        }
        // Background through the model's camera.
        let mut bg_globals = None;
        let loading = matches!(self.screen, Screen::Loading | Screen::Score) || (self.screen == Screen::ModelCinematic && self.cinematic.is_none());
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
            // View space looks down -z; billboards in MDX face +x (as the game camera's
            // billboard_rotation turns them).
            let rot = Quat::from_mat4(&view.inverse()) * Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2) * Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2);
            if let Some(b) = self.background.as_mut() {
                b.transform = Mat4::IDENTITY;
                b.light_cell = 0;
                self.background_renderer.submit(b, eye, rot);
                // The scene is lit by its model's own lights (w3_render::lights), the best
                // eight at its cell; there is no day/night light.
                let mut lights = Vec::new();
                w3_render::lights::instance_lights(b, &mut lights);
                use w3_render::lights::{CELL, cell_of, choose};
                let at = Vec3::new(cell_of(0.0) as f32 * CELL, cell_of(0.0) as f32 * CELL, 0.0);
                let cells = [choose(at, f32::NEG_INFINITY, &lights, 8)];
                let gpu: Vec<_> = lights.iter().map(|l| l.gpu()).collect();
                self.background_renderer.lights.write(queue, &gpu, &cells);
                g.light_grid = [cell_of(0.0) as f32, cell_of(0.0) as f32, 1.0, 1.0];
                g.light_color = [0.0; 4];
                g.ambient = [0.0; 4];
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
        let cinematic = self.screen == Screen::ModelCinematic;
        for l in self.layers.iter_mut().filter(|_| !loading && !campaign && !cinematic) {
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
                // A sequence the screen chose for the frame (the score screen's tab,
                // columns, and team colors).
                if let Some(seq) = self.sprite_sequences.get(&self.tree.frames[*i].name) {
                    // "#n" is the sequence at index n.
                    let want = match seq.strip_prefix('#').and_then(|k| k.parse::<usize>().ok()) {
                        Some(k) => (k < inst.model.mdx.sequences.len()).then_some(k),
                        None => w3_render::anim::find_sequence(&inst.model.mdx, seq),
                    };
                    if want.is_some() && inst.sequence != want {
                        inst.sequence = want;
                        inst.frame = 0.0;
                        inst.finished = false;
                        inst.looped = false;
                    }
                }
                inst.transform = Mat4::from_translation(Vec3::new(*x, *y, 0.0));
                self.sprite_renderer.submit(inst, Vec3::new(0.0, 0.0, 50.0), Quat::IDENTITY);
            }
        }
        // The score screen's rows after the first: CenterBar and BottomBar models in each
        // player's team color, their origin at the row's lower left.
        if let Some(s) = self.score.as_mut().filter(|_| self.screen == Screen::Score) {
            let root = self.tree.frames[0].rect.unwrap_or_default();
            let order = s.data.order(s.tab, s.sort[s.tab], s.teams);
            let bars = crate::scorescreen::row_bars(order.len(), root.y1);
            s.bars.truncate(bars.len().saturating_sub(1));
            for (k, &(path, _, bottom)) in bars.iter().enumerate().skip(1) {
                if s.bars.get(k - 1).is_none_or(|b| b.0 != path) {
                    let Some(m) = assets.model(device, queue, path) else { continue };
                    s.bars.truncate(k - 1);
                    s.bars.push((path.to_string(), ModelInstance::new(m)));
                }
                let color = s.data.rows[order[k]].bar_color(s.ally_colors);
                let inst = &mut s.bars[k - 1].1;
                let want = usize::try_from(color).ok().filter(|&k| k < inst.model.mdx.sequences.len());
                if inst.sequence != want {
                    inst.sequence = want;
                    inst.frame = 0.0;
                }
                inst.transform = Mat4::from_translation(Vec3::new(root.x0, bottom, 0.0));
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
        // A cinematic's black bars, 0.11875 above and below.
        if self.screen == Screen::ModelCinematic {
            let w = self.tree.width.max(w3_ui::UI_WIDTH);
            for (y0, y1) in [(0.0, 0.11875), (w3_ui::UI_HEIGHT - 0.11875, w3_ui::UI_HEIGHT)] {
                draws.push(Draw::Image { texture: String::new(), rect: w3_ui::Rect { x0: -1.0, x1: w + 1.0, y0, y1 }, uv: [0.0, 0.0, 1.0, 1.0], color: [0.0, 0.0, 0.0, 1.0], blend: w3_ui::Blend::Alpha, rotated: false });
            }
        }
        if self.screen == Screen::Loading {
            if let Some(r) = self.tree.get("MinimapImage").filter(|&i| self.tree.is_visible(i)).and_then(|i| self.tree.frames[i].rect) {
                draws.push(Draw::Image { texture: crate::skirmish::MAP_PREVIEW.into(), rect: r, uv: [0.0, 0.0, 1.0, 1.0], color: [1.0; 4], blend: w3_ui::Blend::Alpha, rotated: false });
            }
        }
        if self.screen == Screen::Score {
            self.score_draws(&assets.frames, &mut draws);
            self.save_replay.draws(&self.tree, &assets.frames, &mut draws);
        }
        if matches!(self.screen, Screen::Skirmish | Screen::LoadSaved | Screen::CustomCampaign | Screen::ViewReplay | Screen::LanCreate | Screen::LanLoad) && self.phase == Phase::Stand {
            self.skirmish.draws(&self.tree, &assets.frames, &mut draws);
        }
        if matches!(self.screen, Screen::LanJoin | Screen::LanLobby) && self.phase == Phase::Stand {
            self.skirmish.preview_draws(&self.tree, &mut draws);
            self.lan.draws(&self.tree, &assets.frames, &mut draws);
        }
        if self.screen == Screen::SinglePlayer && self.phase == Phase::Stand && self.dialog.is_none() {
            self.profile.draws(&self.tree, &assets.frames, &mut draws);
        }
        if self.screen == Screen::Campaign && self.phase == Phase::Stand {
            let root = self.tree.frames[0].rect.unwrap_or_default();
            let p = self.persist.borrow();
            let hover = self.campaigns.row_at(&p, root, self.pointer.0, self.pointer.1);
            self.campaigns.draws(&p, &assets.frames, root, hover.as_ref(), &mut draws);
        }
        if self.screen == Screen::Options && self.phase == Phase::Stand {
            self.options_tip_draws(&assets.frames, &mut draws);
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
        draws.extend(std::mem::take(&mut self.overlay));
        self.ui.render(device, queue, &mut assets.textures, &mut pass, &draws, self.tree.width, size);
        drop(pass);
        // The cursor's Normal sequence at the pointer, over everything.
        if self.cursor.is_none() {
            let path = assets.frames.skin_value("Cursor").unwrap_or_default();
            if let Some(m) = assets.model(device, queue, path.trim_matches('"')) {
                let mut c = ModelInstance::new(m);
                c.play("Normal");
                self.cursor = Some(c);
            }
        }
        if let Some(c) = self.cursor.as_mut() {
            let elapsed = self.cursor_clock.elapsed().as_secs_f32();
            self.cursor_clock = std::time::Instant::now();
            c.advance(elapsed * 1000.0);
            c.transform = Mat4::from_translation(Vec3::new(self.pointer.0, self.pointer.1, 0.0));
            self.cursor_renderer.submit(c, Vec3::new(0.0, 0.0, 50.0), Quat::IDENTITY);
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("glue cursor"),
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
            self.cursor_renderer.render(device, queue, &mut pass, &sprite_globals);
        }
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
