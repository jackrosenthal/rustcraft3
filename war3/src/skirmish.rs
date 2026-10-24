//! The Custom Game screen (CSkirmish): browsing the Maps folder, the selected map's
//! information, and the player slots.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use w3_formats::map::MapInfo;
use w3_ui::{Draw, FrameDb, FrameTree, Justify, Rect};

/// One row of the map list.
#[derive(Clone, Debug)]
pub struct Entry {
    pub name: String,
    pub path: PathBuf,
    pub kind: EntryKind,
    /// Player slots of a map, shown on its icon.
    pub players: usize,
    /// A melee map (w3i flag 4), shown with the melee icon rather than the custom one.
    pub melee: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryKind {
    Up,
    Folder,
    Map,
    /// A saved game (the Load Saved Game screen).
    Save,
    /// A custom campaign (the Custom Campaign screen).
    Campaign,
    /// A replay (the View Replay screen).
    Replay,
}

/// What the Custom Campaign screen shows about the selected campaign.
#[derive(Clone, Debug, Default)]
pub struct CampaignSummary {
    pub info: w3_formats::campaign::CampaignInfo,
    pub strings: HashMap<u32, String>,
}

impl CampaignSummary {
    pub fn read(path: &Path) -> Option<CampaignSummary> {
        let ar = w3_mpq::Archive::open(path).ok()?;
        let info = w3_formats::campaign::CampaignInfo::parse(&ar.read("war3campaign.w3f").ok()?).ok()?;
        let strings = ar.read("war3campaign.wts").ok().map(|b| w3_formats::map::parse_wts(&String::from_utf8_lossy(&b))).unwrap_or_default();
        Some(CampaignSummary { info, strings })
    }

    pub fn text(&self, s: &str) -> String {
        match s.strip_prefix("TRIGSTR_").and_then(|n| n.parse::<u32>().ok()) {
            Some(n) => self.strings.get(&n).cloned().unwrap_or_default(),
            None => s.to_string(),
        }
    }
}

/// Who plays a slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Controller {
    Local,
    Open,
    Closed,
    /// AI_DIFFICULTY_NEWBIE (0), _NORMAL (1), or _INSANE (2).
    Computer(i32),
}

/// One row of the team setup.
#[derive(Clone, Debug)]
pub struct SlotChoice {
    pub player: u8,
    pub controller: Controller,
    /// Index into RaceMenu: random, human, orc, undead, night elf.
    pub race: usize,
    pub team: i32,
    pub color: i32,
    /// Index into HandicapMenu: 100% down to 50%.
    pub handicap: usize,
}

/// NameMenu entries for slots other than the local player's, in order.
const NAME_KEYS: [&str; 5] = ["OPEN", "CLOSED", "COMPUTER_NEWBIE", "COMPUTER_NORMAL", "COMPUTER_INSANE"];

/// What the info pane shows about the selected map.
#[derive(Clone, Debug, Default)]
pub struct MapSummary {
    pub info: MapInfo,
    pub strings: HashMap<u32, String>,
    pub preview: Option<w3_formats::blp::Image>,
    /// The map carries a strong signature (the 'NGIS' block after the archive, as
    /// Blizzard's maps do), for the pane's AuthIcon.  The original also checks the
    /// signature against Blizzard's key; this takes its presence.
    pub signed: bool,
}

impl MapSummary {
    fn text(&self, s: &str) -> String {
        match s.strip_prefix("TRIGSTR_").and_then(|n| n.parse::<u32>().ok()) {
            Some(n) => self.strings.get(&n).cloned().unwrap_or_default(),
            None => s.to_string(),
        }
    }

    /// Human-controllable slots (the number on the map icon).
    pub fn user_slots(&self) -> usize {
        self.info.players.iter().filter(|p| p.kind == 1).count()
    }
}

pub struct Skirmish {
    /// The game's archives, for maps inside them (saved campaign games).
    chain: Option<std::sync::Arc<w3_mpq::Chain>>,
    /// The frame holding the list: MapListContainer, or FileListFrame on the Load
    /// Saved Game screen.
    list_frame: &'static str,
    root: PathBuf,
    dir: PathBuf,
    /// The expansion's .w3x maps are listed only with its data.
    expansion: bool,
    pub entries: Vec<Entry>,
    pub selected: Option<usize>,
    scroll: usize,
    pub summary: Option<MapSummary>,
    /// Map list rows' rectangle, from MapListContainer.
    list: Option<Rect>,
    /// WorldEditData [MapSizes] and [TileSets], and WorldEditStrings, for the info pane.
    pub world_edit: w3_formats::profile::Profile,
    pub preview_dirty: bool,
    pub slots: Vec<SlotChoice>,
    /// The screen's frames need rebuilding (the slot count changed).
    pub rebuild: bool,
    /// The local player's name for its slot.
    pub player_name: String,
    /// The Maps folder, which the Custom Game screen browses.
    maps_root: PathBuf,
    /// The selected custom campaign.
    pub campaign: Option<CampaignSummary>,
    /// The selected replay's players.
    replay_players: Option<usize>,
    /// The file name of a map asked for that could not be read.
    unknown_map: Option<String>,
    /// The game setup's advanced options (crate::advanced).
    pub flags: u32,
}

/// Each team setup row's color button shows its player color.
pub fn color_draws(tree: &FrameTree, colors: impl Iterator<Item = i32>, out: &mut Vec<Draw>) {
    for (k, color) in colors.enumerate() {
        if let Some(r) = tree.get(&format!("ColorButtonValue#{k}")).filter(|&i| tree.is_visible(i)).and_then(|i| tree.frames[i].rect) {
            out.push(Draw::Image { texture: format!("ReplaceableTextures\\TeamColor\\TeamColor{color:02}.blp"), rect: r, uv: [0.0, 0.0, 1.0, 1.0], color: [1.0; 4], blend: w3_ui::Blend::Alpha, rotated: false });
        }
    }
}

/// The Custom Campaign screen's list frame (as on the Load Saved Game screen, whose
/// list is also FileListFrame, the mode is told apart by this name).
const CAMPAIGN_LIST: &str = "FileListFrame#campaigns";

/// Height of a map list row and its text, in interface units.
const ROW: f32 = 0.0195;
/// CMapList's margins and row height (its +0x200..+0x20c and +0x230).
const MAP_LIST_INSET: f32 = 0.013125;
const MAP_ROW: f32 = 0.02;
const ROW_FONT: f32 = 0.012;

impl Skirmish {
    pub fn new(game_dir: &Path) -> Skirmish {
        let root = game_dir.join("Maps");
        Skirmish {
            chain: None,
            list_frame: "MapListContainer",
            dir: root.clone(),
            root,
            expansion: true,
            entries: Vec::new(),
            selected: None,
            scroll: 0,
            summary: None,
            list: None,
            world_edit: w3_formats::profile::Profile::default(),
            preview_dirty: false,
            slots: Vec::new(),
            rebuild: false,
            player_name: String::new(),
            maps_root: game_dir.join("Maps"),
            campaign: None,
            replay_players: None,
            unknown_map: None,
            flags: crate::advanced::DEFAULT_FLAGS,
        }
    }

    pub fn load_strings(&mut self, chain: &std::sync::Arc<w3_mpq::Chain>, expansion: bool) {
        self.expansion = expansion;
        self.chain = Some(chain.clone());
        let mut p = w3_formats::profile::Profile::default();
        for f in ["UI\\WorldEditData.txt", "UI\\WorldEditStrings.txt", "UI\\WorldEditGameStrings.txt"] {
            if let Ok(s) = chain.read_string(f) {
                p.merge(&s);
            }
        }
        self.world_edit = p;
    }

    /// Lists the current folder: ".." when below the root, then folders, then maps.
    fn scan(&mut self) {
        self.entries.clear();
        if self.dir != self.root {
            self.entries.push(Entry { name: String::new(), path: self.dir.parent().unwrap_or(&self.root).to_path_buf(), kind: EntryKind::Up, players: 0, melee: false });
        }
        let Ok(rd) = std::fs::read_dir(&self.dir) else { return };
        let mut folders = Vec::new();
        let mut maps = Vec::new();
        for e in rd.flatten() {
            let p = e.path();
            let name = e.file_name().to_string_lossy().to_string();
            if p.is_dir() {
                folders.push(Entry { name, path: p, kind: EntryKind::Folder, players: 0, melee: false });
            } else {
                let ext = p.extension().map(|x| x.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
                if self.list_frame == CAMPAIGN_LIST {
                    // Custom campaigns list under their own names.
                    if ext == "w3n" {
                        let title = CampaignSummary::read(&p).map(|s| s.text(&s.info.name)).unwrap_or_default();
                        maps.push(Entry { name: if title.is_empty() { name } else { title }, path: p, kind: EntryKind::Campaign, players: 0, melee: false });
                    }
                } else if ext == "w3m" || ext == "w3x" && self.expansion {
                    // Maps list under their own names, by player count.
                    let Some(s) = summarize(&p, false) else { continue };
                    let title = s.text(&s.info.name);
                    maps.push(Entry { name: if title.is_empty() { name } else { title }, path: p, kind: EntryKind::Map, players: s.user_slots(), melee: s.info.flags & 0x4 != 0 });
                }
            }
        }
        folders.sort_by_key(|e| e.name.to_ascii_lowercase());
        maps.sort_by_key(|e| (e.players, e.name.to_ascii_lowercase()));
        self.entries.extend(folders);
        self.entries.extend(maps);
        self.scroll = 0;
        self.selected = None;
    }

    fn select(&mut self, i: usize) {
        self.selected = Some(i);
        self.summary = None;
        self.unknown_map = None;
        self.campaign = None;
        let Some(e) = self.entries.get(i) else { return };
        if e.kind == EntryKind::Campaign {
            self.campaign = CampaignSummary::read(&e.path);
            return;
        }
        if e.kind == EntryKind::Replay {
            // The replay's map, as a file or in the game's archives.
            let replay = std::fs::read(&e.path).ok().and_then(|b| w3_game::replay::Replay::parse(&b).ok());
            self.replay_players = replay.as_ref().and_then(|r| r.lobby.as_ref()).map(|l| l.iter().filter(|s| !matches!(s.fill, w3_game::SlotFill::Empty)).count());
            let map = replay.map(|r| r.map);
            self.summary = map.and_then(|m| {
                let p = PathBuf::from(&m);
                if p.is_file() { summarize(&p, true) } else { self.chain.as_ref().and_then(|c| summarize_from(c, &m)) }
            });
            self.preview_dirty = true;
            return;
        }
        if e.kind == EntryKind::Save {
            // The saved game's map: a file, or one in the game's archives.
            let map = std::fs::read(&e.path).ok().and_then(|b| w3_game::save::SaveGame::peek_map(&b));
            self.summary = map.and_then(|m| {
                let p = PathBuf::from(&m);
                // Saved games show the unknown map image, not the map's preview.
                if p.is_file() { summarize(&p, false) } else { self.chain.as_ref().and_then(|c| summarize_from(c, &m)) }
            });
            self.preview_dirty = true;
            return;
        }
        if e.kind != EntryKind::Map {
            return;
        }
        self.summary = summarize(&e.path, true);
        self.preview_dirty = true;
        // The map's user slots: the local player first, computers in the rest.
        let old = self.slots.len();
        self.slots = self
            .summary
            .iter()
            .flat_map(|s| s.info.players.iter().filter(|p| p.kind == 1))
            .enumerate()
            .map(|(k, p)| SlotChoice {
                player: p.id as u8,
                controller: if k == 0 { Controller::Local } else { Controller::Computer(1) },
                race: 0,
                team: k as i32,
                color: p.id as i32,
                handicap: 0,
            })
            .collect();
        self.rebuild |= self.slots.len() != old;
    }

    /// WorldEditData [LoadingScreens] entry `i`: its background model and which of the
    /// model's sequences to play (the map's place on it).
    pub fn loading_screen(&self, i: i32) -> Option<(String, usize)> {
        let v = self.world_edit.get_list("LoadingScreens", &format!("{i:02}"));
        let model = v.get(3).map(|s| s.trim().to_string()).filter(|s| !s.is_empty())?;
        Some((model, v.get(2).and_then(|s| s.trim().parse().ok()).unwrap_or(0)))
    }

    /// The game setup for the selected map.
    pub fn lobby(&self) -> Vec<w3_game::LobbySlot> {
        self.slots
            .iter()
            .map(|s| w3_game::LobbySlot {
                player: s.player,
                fill: match s.controller {
                    Controller::Local => w3_game::SlotFill::Local,
                    Controller::Computer(_) => w3_game::SlotFill::Computer,
                    Controller::Open | Controller::Closed => w3_game::SlotFill::Empty,
                },
                name: (s.controller == Controller::Local).then(|| self.player_name.clone()),
                race: (s.race > 0).then_some(s.race as i32),
                team: s.team,
                color: s.color,
                handicap: 100.0 - 10.0 * s.handicap as f32,
                difficulty: match s.controller {
                    Controller::Computer(d) => d,
                    _ => 1,
                },
            })
            .collect()
    }

    /// Creates a PlayerSlot row per slot in TeamSetup and shows each slot's choices.
    fn attach_slots(&self, tree: &mut FrameTree, db: &FrameDb) {
        use w3_ui::FramePoint as P;
        let Some(team_setup) = tree.get("TeamSetup") else { return };
        let width = tree.get("TeamSetupContainer").and_then(|i| tree.frames[i].width);
        for (k, s) in self.slots.iter().enumerate() {
            let scope = format!("#{k}");
            if tree.create_scoped(db, "PlayerSlot", team_setup, &scope).is_none() {
                continue;
            }
            let row = format!("PlayerSlot{scope}");
            tree.set_point(&row, P::TopLeft, "TeamSetup", P::TopLeft, 0.0, -(k as f32) * 0.025);
            tree.set_size(&row, width, None);
            let name_menu = format!("NameMenu{scope}");
            match s.controller {
                Controller::Local => {
                    tree.set_items(&name_menu, vec![self.player_name.clone()], 0);
                    tree.set_enabled(&name_menu, false);
                }
                c => {
                    let items = NAME_KEYS.iter().map(|k| db.string(k)).collect();
                    let sel = match c {
                        Controller::Open => 0,
                        Controller::Closed => 1,
                        Controller::Computer(d) => 2 + d.clamp(0, 2) as usize,
                        Controller::Local => 0,
                    };
                    tree.set_items(&name_menu, items, sel);
                }
            }
            tree.set_selected(&format!("RaceMenu{scope}"), s.race);
            tree.set_selected(&format!("HandicapMenu{scope}"), s.handicap);
            let team = db.string("TEAM_FORMAT").replace("%d", &(s.team + 1).to_string());
            tree.set_text(&format!("TeamButtonTitle{scope}"), &team);
            // The team button opens the list of teams, "TEAM %u" (FUN_6f2bc6e0).
            let teams = (1..=self.slots.len()).map(|t| format!("{} {t}", db.string("TEAM"))).collect();
            tree.set_pulldown(&format!("TeamButton{scope}"), &format!("RacePopupMenuMenu{scope}"), teams, s.team.max(0) as usize);
            // The color button opens the player colors' swatches (FUN_6f2bc6e0).
            let swatches = (0..12).map(|c| format!("ReplaceableTextures\\TeamColor\\TeamColor{c:02}.blp")).collect();
            tree.set_pulldown_images(&format!("ColorButton{scope}"), &format!("RacePopupMenuMenu{scope}"), swatches, s.color.clamp(0, 11) as usize);
            let open = matches!(s.controller, Controller::Open | Controller::Closed);
            for f in ["RaceMenu", "TeamButton", "ColorButton", "HandicapMenu"] {
                tree.set_visible(&format!("{f}{scope}"), !open);
            }
        }
    }

    /// Lists the saved games in `dir` for the Load Saved Game screen, newest first, and
    /// selects the first.
    pub fn attach_saves(&mut self, tree: &mut FrameTree, db: &FrameDb, dir: &Path) {
        self.list_frame = "FileListFrame";
        for (template, container) in [("MapListBox", "FileListFrame"), ("MapInfoPane", "MapInfoPaneContainer")] {
            if let (Some(parent), true) = (tree.get(container), db.templates.contains_key(template)) {
                if let Some(i) = tree.create(db, template, parent) {
                    tree.frames[i].all_points = true;
                }
            }
        }
        // Measured from the original's Load Saved Game screen.
        Self::layout_info_at(tree, [0.001, -0.033, -0.172, -0.2233]);
        self.entries = crate::saveload::saved_games(dir).into_iter().map(|(name, path)| Entry { name, path, kind: EntryKind::Save, players: 0, melee: false }).collect();
        self.scroll = 0;
        self.selected = None;
        self.summary = None;
        self.unknown_map = None;
        if !self.entries.is_empty() {
            self.select(0);
        }
        self.fill_info(tree, db);
    }

    /// Lists the custom campaigns in `dir` (the Campaigns folder) for the Custom
    /// Campaign screen (CCustomCampaignMenu), and selects the first.
    pub fn attach_campaigns(&mut self, tree: &mut FrameTree, db: &FrameDb, dir: &Path) {
        self.list_frame = CAMPAIGN_LIST;
        if let (Some(parent), true) = (tree.get("FileListFrame"), db.templates.contains_key("MapListBox")) {
            if let Some(i) = tree.create(db, "MapListBox", parent) {
                tree.frames[i].all_points = true;
            }
        }
        self.root = dir.to_path_buf();
        self.dir = self.root.clone();
        self.scan();
        if let Some(i) = self.entries.iter().position(|e| e.kind == EntryKind::Campaign) {
            self.select(i);
        }
        self.fill_campaign(tree, db);
    }

    /// The selected campaign's details in CampaignInfoPane, and its difficulty choice
    /// (Normal or Hard, when the campaign lets the player choose).
    fn fill_campaign(&self, tree: &mut FrameTree, db: &FrameDb) {
        let Some(s) = &self.campaign else {
            for f in ["CampaignNameValue", "AuthorValue", "DifficultyValue", "NumMissionsValue", "MapDescValue"] {
                tree.set_text(f, "");
            }
            tree.set_enabled("DifficultySelect", false);
            return;
        };
        tree.set_text("CampaignNameValue", &s.text(&s.info.name));
        tree.set_text("AuthorValue", &s.text(&s.info.author));
        tree.set_text("DifficultyValue", &s.text(&s.info.difficulty));
        tree.set_text("NumMissionsValue", &s.info.missions.len().to_string());
        tree.set_text("MapDescValue", &s.text(&s.info.description));
        let sel = tree.get("DifficultySelect").map_or(0, |i| tree.frames[i].selected);
        tree.set_items("DifficultySelect", vec![db.string("NORMAL"), db.string("HARD")], sel.min(1));
        tree.set_enabled("DifficultySelect", s.info.flags & 1 != 0);
    }

    pub fn selected_campaign(&self) -> Option<PathBuf> {
        let e = self.entries.get(self.selected?)?;
        (e.kind == EntryKind::Campaign).then(|| e.path.clone())
    }

    /// Lists the replays in `dir` (the Replay folder, its folders first) for the View
    /// Replay screen, and selects the first.
    pub fn attach_replays(&mut self, tree: &mut FrameTree, db: &FrameDb, dir: &Path) {
        self.list_frame = "FileListFrame";
        for (template, container) in [("MapListBox", "FileListFrame"), ("MapInfoPane", "MapInfoPaneContainer")] {
            if let (Some(parent), true) = (tree.get(container), db.templates.contains_key(template)) {
                if let Some(i) = tree.create(db, template, parent) {
                    tree.frames[i].all_points = true;
                }
            }
        }
        Self::layout_info_at(tree, [0.001, -0.033, -0.172, -0.2233]);
        // CViewReplayScreen clears the pane's description row (+0x194 bit 0x80).
        tree.set_visible("MapDescLabel", false);
        tree.set_visible("MapDescValue", false);
        let mut entries = Vec::new();
        if let Ok(rd) = std::fs::read_dir(dir) {
            let mut files: Vec<_> = rd.flatten().map(|e| e.path()).collect();
            files.sort();
            for p in files {
                let name = p.file_stem().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                if p.is_dir() {
                    entries.insert(0, Entry { name, path: p, kind: EntryKind::Folder, players: 0, melee: false });
                } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("w3g")) {
                    entries.push(Entry { name, path: p, kind: EntryKind::Replay, players: 0, melee: false });
                }
            }
        }
        self.entries = entries;
        self.scroll = 0;
        self.selected = None;
        self.summary = None;
        self.unknown_map = None;
        if let Some(i) = self.entries.iter().position(|e| e.kind == EntryKind::Replay) {
            self.select(i);
        }
        self.fill_info(tree, db);
    }

    pub fn selected_replay(&self) -> Option<PathBuf> {
        let e = self.entries.get(self.selected?)?;
        (e.kind == EntryKind::Replay).then(|| e.path.clone())
    }

    /// MapInfoPane in a screen's MapInfoPaneContainer, showing a map, laid out as the
    /// game list and game setup screens have it (+0x3a0 1, the description row cleared
    /// from +0x194, as CLocalMultiplayerJoin's and CGameChatroom's vf55 set it).
    pub fn attach_pane(&mut self, tree: &mut FrameTree, db: &FrameDb, map: Option<&Path>) {
        if let (Some(parent), true) = (tree.get("MapInfoPaneContainer"), db.templates.contains_key("MapInfoPane")) {
            if let Some(i) = tree.create(db, "MapInfoPane", parent) {
                tree.frames[i].all_points = true;
            }
        }
        Self::layout_info_at(tree, [0.001, -0.033, -0.172, -0.2233]);
        tree.set_visible("MapDescLabel", false);
        tree.set_visible("MapDescValue", false);
        self.show_map(tree, db, map);
    }

    /// The pane shows a map (a file or one in the game's archives), or nothing.
    pub fn show_map(&mut self, tree: &mut FrameTree, db: &FrameDb, map: Option<&Path>) {
        self.campaign = None;
        self.replay_players = None;
        self.summary = map.and_then(|p| if p.is_file() { summarize(p, true) } else { self.chain.as_ref().and_then(|c| summarize_from(c, &p.to_string_lossy())) });
        self.unknown_map = map.filter(|_| self.summary.is_none()).map(|p| p.to_string_lossy().rsplit(['\\', '/']).next().unwrap_or_default().to_string());
        self.preview_dirty = true;
        self.fill_info(tree, db);
    }

    /// Back to the Custom Game screen's map list.
    pub fn detach_saves(&mut self) {
        if self.list_frame != "MapListContainer" {
            self.list_frame = "MapListContainer";
            self.root = self.maps_root.clone();
            self.dir = self.root.clone();
            self.entries.clear();
            self.selected = None;
            self.summary = None;
            self.unknown_map = None;
            self.campaign = None;
        }
    }

    pub fn selected_save(&self) -> Option<PathBuf> {
        let e = self.entries.get(self.selected?)?;
        (e.kind == EntryKind::Save).then(|| e.path.clone())
    }

    pub fn selected_map(&self) -> Option<PathBuf> {
        let e = self.entries.get(self.selected?)?;
        (e.kind == EntryKind::Map).then(|| e.path.clone())
    }

    /// Creates the screen's parts in their containers and fills in the first map.
    pub fn attach(&mut self, tree: &mut FrameTree, db: &FrameDb) {
        for (template, container) in [("MapListBox", "MapListContainer"), ("MapInfoPane", "MapInfoPaneContainer"), ("TeamSetup", "TeamSetupContainer")] {
            if let (Some(parent), true) = (tree.get(container), db.templates.contains_key(template)) {
                if let Some(i) = tree.create(db, template, parent) {
                    tree.frames[i].all_points = true;
                }
            }
        }
        // CMapList sets its scroll bar inside its margins, the list's height less them.
        use w3_ui::FramePoint as FP;
        tree.set_point("MapListScrollBar", FP::TopRight, "MapListBox", FP::TopRight, -MAP_LIST_INSET, -MAP_LIST_INSET);
        tree.set_point("MapListScrollBar", FP::BottomRight, "MapListBox", FP::BottomRight, -MAP_LIST_INSET, MAP_LIST_INSET);
        // It starts at the list's top (CMapList::vf55 sets 1.0).
        tree.set_value("MapListScrollBar", 1.0);
        Self::layout_info(tree);
        if self.entries.is_empty() {
            self.scan();
            // The first map in the folder starts selected.
            if let Some(i) = self.entries.iter().position(|e| e.kind == EntryKind::Map) {
                self.select(i);
            }
        }
        self.fill_info(tree, db);
        self.attach_slots(tree, db);
    }

    /// Places MapInfoPane's parts, which CMapInfoPane positions in code: the name row,
    /// the minimap below it, then the label rows and the description (offsets measured
    /// from the original at 1600x900).
    fn layout_info(tree: &mut FrameTree) {
        Self::layout_info_at(tree, [-0.019, -0.06, -0.186, -0.2353]);
    }

    /// The parts at these heights below the pane's top: the name row, the minimap, the
    /// first label row, and the description label.
    fn layout_info_at(tree: &mut FrameTree, [name, minimap, labels, desc]: [f32; 4]) {
        use w3_ui::FramePoint as P;
        let pane = "MapInfoPane";
        let Some(w) = tree.get("MapInfoPaneContainer").and_then(|i| tree.frames[i].width) else { return };
        tree.set_point("MaxPlayersIcon", P::TopLeft, pane, P::TopLeft, 0.075, name);
        tree.set_point("MinimapImage", P::Top, pane, P::Top, 0.0, minimap);
        let rows = [("SuggestedPlayersLabel", labels), ("MapSizeLabel", labels - 0.0147), ("MapTilesetLabel", labels - 0.0293), ("MapDescLabel", desc)];
        for (label, y) in rows {
            tree.set_point(label, P::TopLeft, pane, P::TopLeft, 0.0, y);
            tree.set_size(label, Some(w), Some(0.0146));
        }
        for v in ["SuggestedPlayersValue", "MapSizeValue", "MapTilesetValue"] {
            tree.set_size(v, Some(w), Some(0.0146));
        }
        tree.set_size("MapDescValue", Some(w), Some(0.06));
    }

    /// Writes the selected map's details into MapInfoPane's text frames.
    fn fill_info(&self, tree: &mut FrameTree, db: &FrameDb) {
        // The list's scroll bar shows only when its rows outgrow it (an empty saved
        // games list has none, as in the original).
        let rows = tree.get(self.list_frame.split('#').next().unwrap_or_default()).and_then(|i| tree.frames[i].rect).map(|r| ((r.h() - 2.0 * MAP_LIST_INSET) / self.row_h()) as usize);
        tree.set_visible("MapListScrollBar", rows.map_or(!self.entries.is_empty(), |n| self.entries.len() > n));
        // Its value runs from 1 at the list's top to 0 at its end (FUN_6f2ac2a0).
        if let Some(n) = rows.filter(|&n| self.entries.len() > n) {
            tree.set_value("MapListScrollBar", 1.0 - self.scroll as f32 / (self.entries.len() - n) as f32);
        }
        // The player count and the Blizzard logo show only with a map, the logo only
        // for a signed one.
        tree.set_visible("MaxPlayersIcon", self.summary.is_some());
        tree.set_visible("AuthIcon", self.summary.as_ref().is_some_and(|s| s.signed));
        let Some(s) = &self.summary else {
            for f in ["MapNameValue", "SuggestedPlayersValue", "MapSizeValue", "MapTilesetValue", "MapDescValue", "MaxPlayersValue"] {
                tree.set_text(f, "");
            }
            // A map that could not be read shows its file name and the UNKNOWNMAP texts
            // (CMapInfoPane, FUN_6f2d11d0).
            if let Some(name) = &self.unknown_map {
                tree.set_text("MapNameValue", name);
                tree.set_text("SuggestedPlayersValue", &db.string("UNKNOWNMAP_SUGGESTEDPLAYERS"));
                tree.set_text("MapSizeValue", &db.string("UNKNOWNMAP_MAPSIZE"));
                tree.set_text("MapTilesetValue", &db.string("UNKNOWNMAP_TILESET"));
                tree.set_text("MapDescValue", &db.string("UNKNOWNMAP_DESCRIPTION"));
            }
            return;
        };
        tree.set_text("MapNameValue", &s.text(&s.info.name));
        let players = self.replay_players.unwrap_or_else(|| s.info.players.len());
        tree.set_text("NumPlayersValue", &players.to_string());
        tree.set_text("MaxPlayersValue", &s.user_slots().to_string());
        tree.set_text("SuggestedPlayersValue", &s.text(&s.info.recommended_players));
        // [MapSizes]: the first size whose area bound is at least the playable area
        // (FUN_6f71f720).
        let area = s.info.playable_width as i64 * s.info.playable_height as i64;
        let n: i64 = self.world_edit.get_str("MapSizes", "NumSizes").and_then(|v| v.trim().parse().ok()).unwrap_or(0);
        let mut size = self.world_edit.get_str("WorldEditStrings", "WESTRING_UNKNOWN").unwrap_or_default();
        for i in 0..n {
            let v = self.world_edit.get_list("MapSizes", &format!("Size{i:02}"));
            let bound: i64 = v.first().and_then(|x| x.trim().parse().ok()).unwrap_or(0);
            if area <= bound {
                let key = v.get(1).cloned().unwrap_or_default();
                size = self.world_edit.get_str("WorldEditStrings", key.trim()).unwrap_or(key);
                break;
            }
        }
        tree.set_text("MapSizeValue", &size);
        let tileset_key = self.world_edit.get_list("TileSets", &(s.info.tileset as char).to_string()).first().cloned().unwrap_or_default();
        let tileset = self.world_edit.get_str("WorldEditStrings", tileset_key.trim()).unwrap_or(tileset_key);
        tree.set_text("MapTilesetValue", &tileset);
        tree.set_text("MapDescValue", &s.text(&s.info.description));
        let _ = db;
    }

    /// The list's row height: the map list's (CMapList), or the saved games list's.
    fn row_h(&self) -> f32 {
        if self.list_frame == "MapListContainer" { MAP_ROW } else { ROW }
    }

    /// A click in the map list: selects a row; a click on the selected folder opens it.
    pub fn click(&mut self, tree: &mut FrameTree, db: &FrameDb, x: f32, y: f32) {
        let Some(r) = self.list else { return };
        if !r.contains(x, y) {
            return;
        }
        let row = ((r.y1 - y) / self.row_h()) as usize + self.scroll;
        if row >= self.entries.len() {
            return;
        }
        let e = self.entries[row].clone();
        match e.kind {
            EntryKind::Up | EntryKind::Folder if self.selected == Some(row) => {
                self.dir = e.path;
                self.scan();
                // A folder opens with its "(up one level)" row chosen.
                if self.entries.first().is_some_and(|e| e.kind == EntryKind::Up) {
                    self.select(0);
                }
            }
            _ => self.select(row),
        }
        if self.list_frame == CAMPAIGN_LIST {
            self.fill_campaign(tree, db);
        } else {
            self.fill_info(tree, db);
        }
        tree.layout();
    }

    /// A slot control changed: popup menus report their new choice; the team and
    /// color buttons step to the next team or color.
    pub fn button(&mut self, tree: &mut FrameTree, db: &FrameDb, name: &str) {
        let Some((part, k)) = name.split_once('#') else { return };
        let Ok(k) = k.parse::<usize>() else { return };
        let n = self.slots.len() as i32;
        let chosen = tree.get(name).map_or(0, |i| tree.frames[i].selected);
        let Some(s) = self.slots.get_mut(k) else { return };
        match part {
            "NameMenu" => {
                s.controller = match chosen {
                    0 => Controller::Open,
                    1 => Controller::Closed,
                    d => Controller::Computer(d as i32 - 2),
                };
                self.rebuild = true;
            }
            "RaceMenu" => s.race = chosen,
            "HandicapMenu" => s.handicap = chosen,
            "TeamButton" => {
                s.team = (chosen as i32).min(n - 1).max(0);
                let team = db.string("TEAM_FORMAT").replace("%d", &(s.team + 1).to_string());
                tree.set_text(&format!("TeamButtonTitle#{k}"), &team);
            }
            "ColorButton" => s.color = chosen.min(11) as i32,
            _ => {}
        }
    }

    /// The map list rows and the map preview.
    pub fn draws(&mut self, tree: &FrameTree, db: &FrameDb, out: &mut Vec<Draw>) {
        self.list_draws(tree, db, out);
        self.preview_draws(tree, out);
    }

    fn list_draws(&mut self, tree: &FrameTree, db: &FrameDb, out: &mut Vec<Draw>) {
        let Some(container) = tree.get(self.list_frame.split('#').next().unwrap_or_default()).and_then(|i| tree.frames[i].rect) else { return };
        // Rows sit inside the list's border, left of its scroll bar; the saved games list
        // (measured from the original) runs nearly to the border, in larger text.
        let saves = self.list_frame != "MapListContainer";
        // The map list (CMapList) keeps 0.013125 inside its border on every side, its
        // rows 0.02 tall, and its scroll bar inside the same margins (FUN_6f2abb20).
        let inner = if saves {
            Rect { x0: container.x0 + 0.012, x1: container.x1 - 0.0125, y0: container.y0 + 0.008, y1: container.y1 - 0.008 }
        } else {
            Rect { x0: container.x0 + MAP_LIST_INSET, x1: container.x1 - MAP_LIST_INSET - 0.0165, y0: container.y0 + MAP_LIST_INSET, y1: container.y1 - MAP_LIST_INSET }
        };
        self.list = Some(inner);
        let row_h = if saves { ROW } else { MAP_ROW };
        let rows = (inner.h() / row_h) as usize;
        let row_font = if saves { 0.0135 } else { ROW_FONT };
        let font = db.decorate("MasterFont");
        for (k, e) in self.entries.iter().enumerate().skip(self.scroll).take(rows) {
            let y1 = inner.y1 - (k - self.scroll) as f32 * row_h;
            let row = Rect { x0: inner.x0, x1: inner.x1, y0: y1 - row_h, y1 };
            if self.selected == Some(k) {
                out.push(Draw::Image { texture: String::new(), rect: row, uv: [0.0, 0.0, 1.0, 1.0], color: [0.0, 0.0, 0.8, 0.8], blend: w3_ui::Blend::Alpha, rotated: false });
            }
            let icon = match e.kind {
                EntryKind::Up => "ui\\widgets\\glues\\icon-folder-up.blp",
                EntryKind::Folder => "ui\\widgets\\glues\\icon-folder.blp",
                // game.dll's file icons (0x6faa37c4): melee and custom maps, files.
                EntryKind::Map if e.melee => "ui\\widgets\\glues\\icon-file-melee.blp",
                EntryKind::Map => "ui\\widgets\\glues\\icon-file-ums.blp",
                EntryKind::Save | EntryKind::Campaign | EntryKind::Replay => "ui\\widgets\\glues\\icon-file.blp",
            };
            let ir = if saves { Rect { x0: row.x0 + 0.004, x1: row.x0 + 0.004 + ROW, y0: row.y0, y1: row.y1 } } else { Rect { x0: row.x0 + 0.004, x1: row.x0 + 0.004 + ROW * 0.9 * 0.75, y0: row.y0 + ROW * 0.05, y1: row.y1 - ROW * 0.05 } };
            out.push(Draw::Image { texture: icon.into(), rect: ir, uv: [0.0, 0.0, 1.0, 1.0], color: [1.0; 4], blend: w3_ui::Blend::Alpha, rotated: false });
            if e.kind == EntryKind::Map {
                out.push(Draw::Text { font: font.clone(), size: 0.009, rect: ir, text: e.players.to_string(), color: [1.0; 4], justify_h: Justify::Middle, justify_v: Justify::Middle, shadow: None });
            }
            let tr = Rect { x0: ir.x1 + 0.008, x1: row.x1, y0: row.y0, y1: row.y1 };
            let color = if self.selected == Some(k) { [1.0, 0.8, 0.0, 1.0] } else { [1.0; 4] };
            // The parent folder's row reads UP_ONE_LEVEL (CMapList, FUN_6f2bf120).
            let text = if e.kind == EntryKind::Up { db.string("UP_ONE_LEVEL") } else { e.name.clone() };
            out.push(Draw::Text { font: font.clone(), size: row_font, rect: tr, text, color, justify_h: Justify::Start, justify_v: Justify::Middle, shadow: None });
        }
        // Each slot's color button shows the player color.
        color_draws(tree, self.slots.iter().map(|s| s.color), out);
    }

    /// The map preview, where MinimapImage is.
    pub fn preview_draws(&self, tree: &FrameTree, out: &mut Vec<Draw>) {
        // The preview goes where MinimapImage is; a map without one (and a saved game)
        // shows Minimap-Unknown.
        // A campaign shows its own image.
        if let (Some(c), Some(r)) = (self.campaign.as_ref(), tree.get("MinimapImage").filter(|&i| tree.is_visible(i)).and_then(|i| tree.frames[i].rect)) {
            out.push(Draw::Image { texture: c.info.minimap_path.clone(), rect: r, uv: [0.0, 0.0, 1.0, 1.0], color: [1.0; 4], blend: w3_ui::Blend::Alpha, rotated: false });
        }
        if let (Some(s), Some(r)) = (self.summary.as_ref(), tree.get("MinimapImage").filter(|&i| tree.is_visible(i)).and_then(|i| tree.frames[i].rect)) {
            let texture = if s.preview.is_some() { MAP_PREVIEW.to_string() } else { "UI\\Widgets\\Glues\\Minimap-Unknown.blp".to_string() };
            out.push(Draw::Image { texture, rect: r, uv: [0.0, 0.0, 1.0, 1.0], color: [1.0; 4], blend: w3_ui::Blend::Alpha, rotated: false });
        }
    }
}

/// A map in the game's archives (a campaign mission), read for the loading screen.
pub fn summarize_from(chain: &w3_mpq::Chain, name: &str) -> Option<MapSummary> {
    let ar = w3_mpq::Archive::from_bytes(name, chain.read(name).ok()?).ok()?;
    summarize_archive(&ar, false)
}

/// A map file on disk, read for the loading screen.
pub fn summarize_path(path: &Path) -> Option<MapSummary> {
    summarize(path, false)
}

/// Reads a map's w3i, trigger strings, and optionally its preview image.
fn summarize(path: &Path, preview: bool) -> Option<MapSummary> {
    let ar = w3_mpq::Archive::open(path).ok()?;
    let mut s = summarize_archive(&ar, preview)?;
    s.signed = strongly_signed(path);
    Some(s)
}

/// Whether a file ends in a strong signature block: 'NGIS' and 256 bytes.
fn strongly_signed(path: &Path) -> bool {
    use std::io::{Read, Seek, SeekFrom};
    let Ok(mut f) = std::fs::File::open(path) else { return false };
    let mut tag = [0u8; 4];
    f.seek(SeekFrom::End(-260)).is_ok() && f.read_exact(&mut tag).is_ok() && &tag == b"NGIS"
}

fn summarize_archive(ar: &w3_mpq::Archive, preview: bool) -> Option<MapSummary> {
    let info = MapInfo::parse(&ar.read("war3map.w3i").ok()?).ok()?;
    let strings = ar.read("war3map.wts").ok().map(|b| w3_formats::map::parse_wts(&String::from_utf8_lossy(&b))).unwrap_or_default();
    let preview = if preview { ar.read("war3mapMap.blp").ok().and_then(|b| w3_formats::blp::decode_texture("war3mapMap.blp", &b).ok()).and_then(|m| m.into_iter().next()) } else { None };
    Some(MapSummary { info, strings, preview, signed: false })
}

/// The run-time texture holding the selected map's preview.
pub const MAP_PREVIEW: &str = "@mappreview";
