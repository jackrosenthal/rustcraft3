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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryKind {
    Up,
    Folder,
    Map,
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
    world_edit: w3_formats::profile::Profile,
    pub preview_dirty: bool,
    pub slots: Vec<SlotChoice>,
    /// The screen's frames need rebuilding (the slot count changed).
    pub rebuild: bool,
    /// The local player's name for its slot.
    pub player_name: String,
}

/// Height of a map list row and its text, in interface units.
const ROW: f32 = 0.0195;
const ROW_FONT: f32 = 0.012;

impl Skirmish {
    pub fn new(game_dir: &Path) -> Skirmish {
        let root = game_dir.join("Maps");
        Skirmish {
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
        }
    }

    pub fn load_strings(&mut self, chain: &w3_mpq::Chain, expansion: bool) {
        self.expansion = expansion;
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
            self.entries.push(Entry { name: String::new(), path: self.dir.parent().unwrap_or(&self.root).to_path_buf(), kind: EntryKind::Up, players: 0 });
        }
        let Ok(rd) = std::fs::read_dir(&self.dir) else { return };
        let mut folders = Vec::new();
        let mut maps = Vec::new();
        for e in rd.flatten() {
            let p = e.path();
            let name = e.file_name().to_string_lossy().to_string();
            if p.is_dir() {
                folders.push(Entry { name, path: p, kind: EntryKind::Folder, players: 0 });
            } else {
                let ext = p.extension().map(|x| x.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
                if ext == "w3m" || ext == "w3x" && self.expansion {
                    // Maps list under their own names, by player count.
                    let Some(s) = summarize(&p, false) else { continue };
                    let title = s.text(&s.info.name);
                    maps.push(Entry { name: if title.is_empty() { name } else { title }, path: p, kind: EntryKind::Map, players: s.user_slots() });
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
        let Some(e) = self.entries.get(i) else { return };
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

    /// WorldEditData [LoadingScreens] entry `i`: its background model.
    pub fn loading_screen(&self, i: i32) -> Option<String> {
        let v = self.world_edit.get_list("LoadingScreens", &format!("{i:02}"));
        v.get(3).map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
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
            let open = matches!(s.controller, Controller::Open | Controller::Closed);
            for f in ["RaceMenu", "TeamButton", "ColorButton", "HandicapMenu"] {
                tree.set_visible(&format!("{f}{scope}"), !open);
            }
        }
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
        tree.set_visible("AdvancedOptionsPanel", false);
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
        use w3_ui::FramePoint as P;
        let pane = "MapInfoPane";
        let Some(w) = tree.get("MapInfoPaneContainer").and_then(|i| tree.frames[i].width) else { return };
        tree.set_point("MaxPlayersIcon", P::TopLeft, pane, P::TopLeft, 0.075, -0.019);
        tree.set_point("MinimapImage", P::Top, pane, P::Top, 0.0, -0.06);
        let rows = [("SuggestedPlayersLabel", -0.186), ("MapSizeLabel", -0.2007), ("MapTilesetLabel", -0.2153), ("MapDescLabel", -0.2353)];
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
        let Some(s) = &self.summary else {
            for f in ["MapNameValue", "SuggestedPlayersValue", "MapSizeValue", "MapTilesetValue", "MapDescValue", "MaxPlayersValue"] {
                tree.set_text(f, "");
            }
            return;
        };
        tree.set_text("MapNameValue", &s.text(&s.info.name));
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

    /// A click in the map list: selects a row; a click on the selected folder opens it.
    pub fn click(&mut self, tree: &mut FrameTree, db: &FrameDb, x: f32, y: f32) {
        let Some(r) = self.list else { return };
        if !r.contains(x, y) {
            return;
        }
        let row = ((r.y1 - y) / ROW) as usize + self.scroll;
        if row >= self.entries.len() {
            return;
        }
        let e = self.entries[row].clone();
        match e.kind {
            EntryKind::Up | EntryKind::Folder if self.selected == Some(row) => {
                self.dir = e.path;
                self.scan();
            }
            _ => self.select(row),
        }
        self.fill_info(tree, db);
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
                s.team = (s.team + 1) % n.max(1);
                let team = db.string("TEAM_FORMAT").replace("%d", &(s.team + 1).to_string());
                tree.set_text(&format!("TeamButtonTitle#{k}"), &team);
            }
            "ColorButton" => s.color = (s.color + 1) % 12,
            _ => {}
        }
    }

    /// The map list rows and the map preview.
    pub fn draws(&mut self, tree: &FrameTree, db: &FrameDb, out: &mut Vec<Draw>) {
        let Some(container) = tree.get("MapListContainer").and_then(|i| tree.frames[i].rect) else { return };
        // Rows sit inside the list's border, left of its scroll bar.
        let inner = Rect { x0: container.x0 + 0.012, x1: container.x1 - 0.03, y0: container.y0 + 0.008, y1: container.y1 - 0.008 };
        self.list = Some(inner);
        let rows = (inner.h() / ROW) as usize;
        let font = db.decorate("MasterFont");
        for (k, e) in self.entries.iter().enumerate().skip(self.scroll).take(rows) {
            let y1 = inner.y1 - (k - self.scroll) as f32 * ROW;
            let row = Rect { x0: inner.x0, x1: inner.x1, y0: y1 - ROW, y1 };
            if self.selected == Some(k) {
                out.push(Draw::Image { texture: String::new(), rect: row, uv: [0.0, 0.0, 1.0, 1.0], color: [0.0, 0.0, 0.8, 0.8], blend: w3_ui::Blend::Alpha, rotated: false });
            }
            let icon = match e.kind {
                EntryKind::Up => "ui\\widgets\\glues\\icon-folder-up.blp",
                EntryKind::Folder => "ui\\widgets\\glues\\icon-folder.blp",
                EntryKind::Map => "ui\\widgets\\glues\\icon-file-melee.blp",
            };
            let ir = Rect { x0: row.x0 + 0.004, x1: row.x0 + 0.004 + ROW * 0.9 * 0.75, y0: row.y0 + ROW * 0.05, y1: row.y1 - ROW * 0.05 };
            out.push(Draw::Image { texture: icon.into(), rect: ir, uv: [0.0, 0.0, 1.0, 1.0], color: [1.0; 4], blend: w3_ui::Blend::Alpha, rotated: false });
            if e.kind == EntryKind::Map {
                out.push(Draw::Text { font: font.clone(), size: 0.009, rect: ir, text: e.players.to_string(), color: [1.0; 4], justify_h: Justify::Middle, justify_v: Justify::Middle, shadow: None });
            }
            let tr = Rect { x0: ir.x1 + 0.008, x1: row.x1, y0: row.y0, y1: row.y1 };
            let color = if self.selected == Some(k) { [1.0, 0.8, 0.0, 1.0] } else { [1.0; 4] };
            out.push(Draw::Text { font: font.clone(), size: ROW_FONT, rect: tr, text: e.name.clone(), color, justify_h: Justify::Start, justify_v: Justify::Middle, shadow: None });
        }
        // Each slot's color button shows the player color.
        for (k, s) in self.slots.iter().enumerate() {
            if let Some(r) = tree.get(&format!("ColorButtonValue#{k}")).filter(|&i| tree.is_visible(i)).and_then(|i| tree.frames[i].rect) {
                out.push(Draw::Image { texture: format!("ReplaceableTextures\\TeamColor\\TeamColor{:02}.blp", s.color), rect: r, uv: [0.0, 0.0, 1.0, 1.0], color: [1.0; 4], blend: w3_ui::Blend::Alpha, rotated: false });
            }
        }
        // The preview goes where MinimapImage is.
        if let (Some(_), Some(r)) = (self.summary.as_ref().and_then(|s| s.preview.as_ref()), tree.get("MinimapImage").and_then(|i| tree.frames[i].rect)) {
            out.push(Draw::Image { texture: MAP_PREVIEW.into(), rect: r, uv: [0.0, 0.0, 1.0, 1.0], color: [1.0; 4], blend: w3_ui::Blend::Alpha, rotated: false });
        }
    }
}

/// Reads a map's w3i, trigger strings, and optionally its preview image.
fn summarize(path: &Path, preview: bool) -> Option<MapSummary> {
    let ar = w3_mpq::Archive::open(path).ok()?;
    let info = MapInfo::parse(&ar.read("war3map.w3i").ok()?).ok()?;
    let strings = ar.read("war3map.wts").ok().map(|b| w3_formats::map::parse_wts(&String::from_utf8_lossy(&b))).unwrap_or_default();
    let preview = if preview { ar.read("war3mapMap.blp").ok().and_then(|b| w3_formats::blp::decode_texture("war3mapMap.blp", &b).ok()).and_then(|m| m.into_iter().next()) } else { None };
    Some(MapSummary { info, strings, preview })
}

/// The run-time texture holding the selected map's preview.
pub const MAP_PREVIEW: &str = "@mappreview";
