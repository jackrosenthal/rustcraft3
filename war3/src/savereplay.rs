//! The score screen's Save Replay panel (CSaveReplayPanel, ScoreScreen.fdf
//! SaveReplayPanel): the kept replays, a name to save the last game's replay under, and
//! Save, Delete, and Cancel.

use std::path::PathBuf;

use w3_ui::{Draw, FrameDb, FrameTree, Justify, Rect};

const ROW: f32 = 0.0195;

#[derive(Default)]
pub struct SaveReplayPanel {
    pub open: bool,
    pub text: String,
    /// The replays in the Replay folder, by name.
    names: Vec<String>,
    selected: Option<usize>,
    list: Option<Rect>,
    /// The delete or overwrite asked about (DELETE_REPLAY_MESSAGE,
    /// OVERWRITE_REPLAY_MESSAGE).
    pending: Option<Pending>,
}

/// What the panel did.
pub enum Event {
    /// The replay was saved.
    Saved,
    /// A question to ask first, Yes or No: the message's key and the file it names.
    Ask(&'static str, String),
}

/// What waits on the question's Yes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pending {
    Delete(usize),
    Overwrite,
}

fn dir() -> Option<PathBuf> {
    crate::data::replay_dir()
}

impl SaveReplayPanel {
    /// Builds the panel over the score screen and opens it.
    pub fn open(&mut self, tree: &mut FrameTree, db: &FrameDb) {
        if tree.get("SaveReplayPanel").is_none() {
            tree.create(db, "SaveReplayPanel", 0);
            tree.bind_points();
            use w3_ui::FramePoint as P;
            tree.set_point("SaveReplayFileEditBoxText", P::TopLeft, "SaveReplayFileEditBox", P::TopLeft, 0.009, -0.009);
            tree.set_point("SaveReplayFileEditBoxText", P::BottomRight, "SaveReplayFileEditBox", P::BottomRight, -0.009, 0.009);
        }
        self.names = dir()
            .and_then(|d| std::fs::read_dir(d).ok())
            .map(|rd| {
                let mut v: Vec<String> = rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("w3g"))).filter_map(|p| p.file_stem().map(|s| s.to_string_lossy().to_string())).collect();
                v.sort();
                v
            })
            .unwrap_or_default();
        self.selected = None;
        self.open = true;
        self.sync(tree);
    }

    pub fn sync(&self, tree: &mut FrameTree) {
        tree.set_visible("SaveReplayPanel", self.open);
        tree.set_text("SaveReplayFileEditBoxText", &format!("{}|", self.text));
        tree.set_enabled("SaveReplaySaveButton", !self.text.trim().is_empty());
        tree.set_enabled("SaveReplayDeleteButton", self.selected.is_some());
        tree.layout();
    }

    /// Typing goes to the name; Enter saves, Escape cancels.
    pub fn key(&mut self, c: char, tree: &mut FrameTree) -> Option<Event> {
        match c {
            '\u{1b}' => self.open = false,
            '\r' | '\n' => {
                let e = self.save_asking();
                self.sync(tree);
                return e;
            }
            '\u{8}' => {
                self.text.pop();
            }
            c if !c.is_control() && !"\\/:*?\"<>|".contains(c) && self.text.chars().count() < 64 => self.text.push(c),
            _ => {}
        }
        self.sync(tree);
        None
    }

    /// Saving over a replay of the same name asks first (FUN_6f2f8640).
    fn save_asking(&mut self) -> Option<Event> {
        let name = self.text.trim().to_string();
        if !name.is_empty() && dir().is_some_and(|d| d.join(format!("{name}.w3g")).is_file()) {
            self.pending = Some(Pending::Overwrite);
            return Some(Event::Ask("OVERWRITE_REPLAY_MESSAGE", format!("{name}.w3g")));
        }
        self.save()
    }

    /// The question's answer.
    pub fn answer(&mut self, yes: bool, tree: &mut FrameTree) -> Option<Event> {
        let e = match (self.pending.take(), yes) {
            (Some(Pending::Overwrite), true) => self.save(),
            (Some(Pending::Delete(k)), true) => {
                if let (Some(d), Some(name)) = (dir(), self.names.get(k).cloned()) {
                    let _ = std::fs::remove_file(d.join(format!("{name}.w3g")));
                    self.names.remove(k);
                    self.selected = None;
                }
                None
            }
            _ => None,
        };
        self.sync(tree);
        e
    }

    /// The last game's replay under the typed name.
    fn save(&mut self) -> Option<Event> {
        let name = self.text.trim().to_string();
        let (Some(d), Some(last)) = (dir(), crate::replays::last_replay()) else { return None };
        if name.is_empty() {
            return None;
        }
        match std::fs::copy(&last, d.join(format!("{name}.w3g"))) {
            Ok(_) => {
                self.open = false;
                Some(Event::Saved)
            }
            Err(e) => {
                log::warn!("saving replay {name}: {e}");
                None
            }
        }
    }

    pub fn button(&mut self, name: &str, tree: &mut FrameTree) -> Option<Event> {
        let e = match name {
            "SaveReplaySaveButton" => self.save_asking(),
            "SaveReplayCancelButton" => {
                self.open = false;
                None
            }
            // Deleting asks first (FUN_6f2eb5c0).
            "SaveReplayDeleteButton" => self.selected.and_then(|k| {
                self.pending = Some(Pending::Delete(k));
                self.names.get(k).map(|n| Event::Ask("DELETE_REPLAY_MESSAGE", format!("{n}.w3g")))
            }),
            _ => None,
        };
        self.sync(tree);
        e
    }

    /// A click in the list chooses a replay and takes its name.
    pub fn click(&mut self, tree: &mut FrameTree, x: f32, y: f32) {
        let Some(r) = self.list.filter(|_| self.open) else { return };
        if !r.contains(x, y) {
            return;
        }
        let k = ((r.y1 - y) / ROW) as usize;
        if k < self.names.len() {
            self.selected = Some(k);
            self.text = self.names[k].clone();
            self.sync(tree);
        }
    }

    pub fn draws(&mut self, tree: &FrameTree, db: &FrameDb, out: &mut Vec<Draw>) {
        if !self.open {
            return;
        }
        let Some(c) = tree.get("ReplayListFrame").and_then(|i| tree.frames[i].rect) else { return };
        let inner = Rect { x0: c.x0 + 0.004, x1: c.x1 - 0.004, y0: c.y0, y1: c.y1 };
        self.list = Some(inner);
        let font = db.decorate("MasterFont");
        for (k, n) in self.names.iter().enumerate().take((inner.h() / ROW) as usize) {
            let y1 = inner.y1 - k as f32 * ROW;
            let row = Rect { x0: inner.x0, x1: inner.x1, y0: y1 - ROW, y1 };
            let chosen = self.selected == Some(k);
            if chosen {
                out.push(Draw::Image { texture: String::new(), rect: row, uv: [0.0, 0.0, 1.0, 1.0], color: [0.0, 0.0, 0.8, 0.8], blend: w3_ui::Blend::Alpha, rotated: false });
            }
            let color = if chosen { [1.0, 0.8, 0.0, 1.0] } else { [1.0; 4] };
            out.push(Draw::Text { font: font.clone(), size: 0.0135, rect: Rect { x0: row.x0 + 0.004, ..row }, text: n.clone(), color, justify_h: Justify::Start, justify_v: Justify::Middle, shadow: None });
        }
    }
}
