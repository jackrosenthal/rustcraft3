//! The game menu's Save Game and Load Game panels (EscMenuSaveGamePanel): the saved
//! games listed newest first in FileListFrame, the name being typed in the edit box,
//! and the overwrite and delete confirmations.

use std::path::{Path, PathBuf};

use w3_ui::{Draw, FrameDb, FrameTree, Justify, Rect};

/// Extension of saved games.
pub const EXTENSION: &str = "w3z";

/// Height of a list row and its text, as the map list's (the same list widget).
const ROW: f32 = 0.0195;
/// Measured from the original's list.
const ROW_FONT: f32 = 0.0135;

/// Why a saved game chosen in the game cannot load, as the game says it in red for
/// ten seconds (FUN_6f3b0120): the file does not read as a save, or its map is gone.
pub fn load_error(data: &crate::data::GameData, db: &FrameDb, file: &Path) -> Option<String> {
    let save = std::fs::read(file).ok().and_then(|b| w3_game::save::Load::parse(&b).ok());
    let Some(save) = save.filter(|s| !s.map().is_empty()) else { return Some(db.string("SAVE_ERROR_BAD_SAVE")) };
    if data.map_of_save(&save).is_file() || data.chain.contains(save.map()) {
        return None;
    }
    let name = save.map().rsplit(['\\', '/']).next().unwrap_or_default();
    Some(db.string("SAVE_ERROR_MISSING_MAP").replacen("%s", name, 1))
}

pub enum Confirm {
    Overwrite(PathBuf),
    Delete(PathBuf),
}

pub struct SavePanel {
    /// Loading rather than saving.
    pub load: bool,
    pub dir: PathBuf,
    /// Names and files, newest first.
    pub entries: Vec<(String, PathBuf)>,
    pub hover: Option<usize>,
    pub selected: Option<usize>,
    pub text: String,
    pub confirm: Option<Confirm>,
    list: Option<Rect>,
}

/// The saved games in `dir`, newest first.
pub fn saved_games(dir: &Path) -> Vec<(String, PathBuf)> {
    let mut v: Vec<(std::time::SystemTime, String, PathBuf)> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case(EXTENSION)))
        .filter_map(|p| {
            let t = p.metadata().and_then(|m| m.modified()).ok()?;
            Some((t, p.file_stem()?.to_string_lossy().to_string(), p))
        })
        .collect();
    v.sort_by(|a, b| b.0.cmp(&a.0));
    v.into_iter().map(|(_, n, p)| (n, p)).collect()
}

impl SavePanel {
    pub fn new(load: bool, dir: PathBuf) -> SavePanel {
        let entries = saved_games(&dir);
        SavePanel { load, dir, entries, hover: None, selected: None, text: String::new(), confirm: None, list: None }
    }

    pub fn refresh(&mut self) {
        self.entries = saved_games(&self.dir);
        self.selected = None;
        self.hover = None;
    }

    /// The file the typed name saves to.
    pub fn target(&self) -> Option<PathBuf> {
        let name = self.text.trim();
        (!name.is_empty()).then(|| self.dir.join(format!("{name}.{EXTENSION}")))
    }

    pub fn selected_path(&self) -> Option<PathBuf> {
        self.selected.and_then(|i| self.entries.get(i)).map(|e| e.1.clone())
    }

    fn row_at(&self, x: f32, y: f32) -> Option<usize> {
        let r = self.list?;
        if !r.contains(x, y) {
            return None;
        }
        let i = ((r.y1 - y) / ROW) as usize;
        (i < self.entries.len()).then_some(i)
    }

    pub fn pointer_move(&mut self, x: f32, y: f32) {
        self.hover = self.row_at(x, y);
    }

    /// A click on a row selects it; saving takes its name.
    pub fn click(&mut self, x: f32, y: f32) -> bool {
        let Some(i) = self.row_at(x, y) else { return false };
        self.selected = Some(i);
        if !self.load {
            self.text = self.entries[i].0.clone();
        }
        true
    }

    /// Shows the panel's state in the menu's frames.
    pub fn sync(&mut self, tree: &mut FrameTree, db: &FrameDb) {
        let confirm = self.confirm.is_some();
        tree.set_visible("EscMenuSaveLoadContainer", !confirm);
        tree.set_visible("LoadOnly", self.load && !confirm);
        tree.set_visible("SaveOnly", !self.load && !confirm);
        tree.set_visible("EscMenuOverwriteContainer", matches!(self.confirm, Some(Confirm::Overwrite(_))));
        tree.set_visible("EscMenuDeleteContainer", matches!(self.confirm, Some(Confirm::Delete(_))));
        // The menu's backdrop takes the shown container's size.
        let (w, h) = if confirm { (0.336, 0.192) } else { (0.432, 0.288) };
        tree.set_size("EscMenuBackdrop", Some(w), Some(h));
        let name = |p: &PathBuf| p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        match &self.confirm {
            Some(Confirm::Overwrite(p)) => tree.set_text("OverwriteMessageText", &db.string("OVERWRITE_MESSAGE").replace("%s", &name(p))),
            Some(Confirm::Delete(p)) => tree.set_text("DeleteMessageText", &db.string("DELETE_MESSAGE").replace("%s", &name(p))),
            None => {}
        }
        tree.set_text("SaveGameFileEditBoxText", &format!("{}|", self.text));
        tree.set_enabled("SaveGameSaveButton", !self.text.trim().is_empty());
        tree.set_enabled("SaveGameDeleteButton", self.selected.is_some());
        tree.set_enabled("LoadGameLoadButton", self.selected.is_some());
        tree.bind_points();
        tree.layout();
        // Rows sit inside the list's border, left of its scroll bar.
        self.list = tree
            .get("FileListFrame")
            .and_then(|i| tree.frames[i].rect)
            .filter(|_| !confirm)
            .map(|r| Rect { x0: r.x0 + 0.012, x1: r.x1 - 0.0125, y0: r.y0 + 0.008, y1: r.y1 - 0.01 });
    }

    /// The list's rows.
    pub fn draws(&self, db: &FrameDb, out: &mut Vec<Draw>) {
        let Some(inner) = self.list else { return };
        let rows = (inner.h() / ROW) as usize;
        let font = db.decorate("MasterFont");
        for (k, (name, _)) in self.entries.iter().enumerate().take(rows) {
            let y1 = inner.y1 - k as f32 * ROW;
            let row = Rect { x0: inner.x0, x1: inner.x1, y0: y1 - ROW, y1 };
            if self.selected == Some(k) || self.hover == Some(k) {
                // Measured from the original: a dim blue band.
                out.push(Draw::Image { texture: String::new(), rect: row, uv: [0.0, 0.0, 1.0, 1.0], color: [0.075, 0.086, 0.153, 1.0], blend: w3_ui::Blend::Add, rotated: false });
            }
            let ir = Rect { x0: row.x0 + 0.004, x1: row.x0 + 0.004 + ROW, y0: row.y0, y1: row.y1 };
            out.push(Draw::Image { texture: "UI\\Widgets\\Glues\\icon-file.blp".into(), rect: ir, uv: [0.0, 0.0, 1.0, 1.0], color: [1.0; 4], blend: w3_ui::Blend::Alpha, rotated: false });
            let tr = Rect { x0: ir.x1 + 0.006, x1: row.x1, y0: row.y0, y1: row.y1 };
            out.push(Draw::Text { font: font.clone(), size: ROW_FONT, rect: tr, text: name.clone(), color: [1.0; 4], justify_h: Justify::Start, justify_v: Justify::Middle, shadow: None });
        }
    }
}
