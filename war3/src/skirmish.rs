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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryKind {
    Up,
    Folder,
    Map,
}

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
        }
    }

    pub fn load_strings(&mut self, chain: &w3_mpq::Chain, expansion: bool) {
        self.expansion = expansion;
        let mut p = w3_formats::profile::Profile::default();
        for f in ["UI\\WorldEditData.txt", "UI\\WorldEditStrings.txt"] {
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
            self.entries.push(Entry { name: String::new(), path: self.dir.parent().unwrap_or(&self.root).to_path_buf(), kind: EntryKind::Up });
        }
        let Ok(rd) = std::fs::read_dir(&self.dir) else { return };
        let mut folders = Vec::new();
        let mut maps = Vec::new();
        for e in rd.flatten() {
            let p = e.path();
            let name = e.file_name().to_string_lossy().to_string();
            if p.is_dir() {
                folders.push(Entry { name, path: p, kind: EntryKind::Folder });
            } else {
                let ext = p.extension().map(|x| x.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
                if ext == "w3m" || ext == "w3x" && self.expansion {
                    let stem = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or(name);
                    maps.push(Entry { name: stem, path: p, kind: EntryKind::Map });
                }
            }
        }
        folders.sort_by_key(|e| e.name.to_ascii_lowercase());
        maps.sort_by_key(|e| e.name.to_ascii_lowercase());
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
        let Ok(ar) = w3_mpq::Archive::open(&e.path) else { return };
        let Ok(info) = ar.read("war3map.w3i").map_err(|e| e.to_string()).and_then(|b| MapInfo::parse(&b).map_err(|e| e.to_string())) else { return };
        let strings = ar.read("war3map.wts").ok().map(|b| w3_formats::map::parse_wts(&String::from_utf8_lossy(&b))).unwrap_or_default();
        let preview = ar.read("war3mapMap.blp").ok().and_then(|b| w3_formats::blp::decode_texture("war3mapMap.blp", &b).ok()).and_then(|m| m.into_iter().next());
        self.summary = Some(MapSummary { info, strings, preview });
        self.preview_dirty = true;
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

    pub fn button(&mut self, _tree: &mut FrameTree, _db: &FrameDb, name: &str) {
        log::debug!("skirmish button {name} has no handler yet");
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
            let tr = Rect { x0: ir.x1 + 0.008, x1: row.x1, y0: row.y0, y1: row.y1 };
            let color = if self.selected == Some(k) { [1.0, 0.8, 0.0, 1.0] } else { [1.0; 4] };
            out.push(Draw::Text { font: font.clone(), size: ROW_FONT, rect: tr, text: e.name.clone(), color, justify_h: Justify::Start, justify_v: Justify::Middle, shadow: None });
        }
        // The preview goes where MinimapImage is.
        if let (Some(_), Some(r)) = (self.summary.as_ref().and_then(|s| s.preview.as_ref()), tree.get("MinimapImage").and_then(|i| tree.frames[i].rect)) {
            out.push(Draw::Image { texture: MAP_PREVIEW.into(), rect: r, uv: [0.0, 0.0, 1.0, 1.0], color: [1.0; 4], blend: w3_ui::Blend::Alpha, rotated: false });
        }
    }
}

/// The run-time texture holding the selected map's preview.
pub const MAP_PREVIEW: &str = "@mappreview";
