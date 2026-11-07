//! The Single Player menu's profile panel (CSinglePlayerMenu): the player's profiles,
//! a box to name a new one, and Create, Delete, and Select.  Without a profile the
//! panel stays open and the menu's other choices wait (FUN_6f30e6d0).

use w3_ui::{Draw, FrameDb, FrameTree, Justify, Rect};

use crate::data::{PROFILE_NAME_MAX, Profiles};

/// The menu's choices that need a profile, with the panel's own.
const NEEDS_PROFILE: [&str; 8] = ["SelectProfileButton", "DeleteProfileButton", "ProfileButton", "CampaignButton", "LoadSavedButton", "ViewReplayButton", "SkirmishButton", "CustomCampaignButton"];

const ROW: f32 = 0.0195;
/// The list's margins at its ends and right side, as the file lists' (measured from
/// their rows in the original).
const END: f32 = 0.008;
const SIDE: f32 = 0.0125;

pub struct ProfilePanel {
    pub open: bool,
    /// What has been typed in NewProfileEditBox.
    pub text: String,
    pub profiles: Profiles,
    /// The list's rows' rectangle.
    list: Option<Rect>,
    /// The list's first row shown.
    scroll: usize,
    /// Wheel movement short of a notch.
    wheel_lines: f32,
}

/// What a click or key asks of the menu.
pub enum Event {
    /// The chosen profile changed.
    Chosen,
    /// Delete asks to confirm (DELETE_PROFILE_MESSAGE).
    ConfirmDelete(String),
    /// Enter with no name typed (PROFILE_NEEDS_A_NAME).
    NeedsName,
}

impl ProfilePanel {
    pub fn new() -> ProfilePanel {
        ProfilePanel { open: false, text: String::new(), profiles: Profiles::load(), list: None, scroll: 0, wheel_lines: 0.0 }
    }

    /// The chosen profile's name.
    pub fn name(&self) -> Option<String> {
        self.profiles.current().map(|p| p.name.clone())
    }

    /// Builds the panel's list box in its container; it opens by itself without a
    /// profile.
    pub fn attach(&mut self, tree: &mut FrameTree, db: &FrameDb) {
        if let (Some(parent), true) = (tree.get("ProfileListContainer"), db.templates.contains_key("ListBoxWar3")) {
            if let Some(i) = tree.create(db, "ListBoxWar3", parent) {
                tree.frames[i].all_points = true;
            }
        }
        // The typed name sits inside the box's border (EditBorderSize).
        use w3_ui::FramePoint as P;
        tree.set_point("NewProfileEditBoxText", P::TopLeft, "NewProfileEditBox", P::TopLeft, 0.009, -0.009);
        tree.set_point("NewProfileEditBoxText", P::BottomRight, "NewProfileEditBox", P::BottomRight, -0.009, 0.009);
        // ListBoxWar3's scroll bar (only its top right point in its template) goes inside
        // the list's margins, the list's height less them, as the map lists' do
        // (provisional: these margins are its rows').
        tree.set_point("ListScrollBar", P::TopRight, "ListBoxWar3", P::TopRight, -SIDE, -END);
        tree.set_point("ListScrollBar", P::BottomRight, "ListBoxWar3", P::BottomRight, -SIDE, END);
        self.scroll = 0;
        if self.profiles.entries.is_empty() {
            self.open = true;
        }
        self.sync(tree);
    }

    /// The panel's visibility, the typed name, and which buttons may be pressed.
    pub fn sync(&self, tree: &mut FrameTree) {
        tree.set_visible("ProfilePanel", self.open);
        tree.set_text("NewProfileEditBoxText", &if self.open { format!("{}|", self.text) } else { self.text.clone() });
        let any = !self.profiles.entries.is_empty();
        for n in NEEDS_PROFILE {
            tree.set_enabled(n, any);
        }
        self.set_bar(tree);
        // Create needs a name not already taken (FUN_6f2faa30).
        tree.set_enabled("AddProfileButton", !self.text.is_empty() && self.profiles.find(&self.text).is_none());
        tree.set_text("ProfileNameText", &self.name().unwrap_or_default());
    }

    /// A typed character while the panel is open: the name grows, shrinks with
    /// Backspace, and Enter creates the profile.  Returns whether the key was taken.
    pub fn key(&mut self, c: char, tree: &mut FrameTree) -> Option<Option<Event>> {
        if !self.open {
            return None;
        }
        match c {
            '\u{8}' => {
                self.text.pop();
            }
            '\r' | '\n' => {
                // With nothing typed the original says a name is needed (FUN_6f2e4c40).
                let e = if self.text.is_empty() { Some(Event::NeedsName) } else { self.create() };
                self.sync(tree);
                return Some(e);
            }
            c if !c.is_control() && self.text.chars().count() < PROFILE_NAME_MAX => self.text.push(c),
            _ => return Some(None),
        }
        self.sync(tree);
        Some(None)
    }

    /// Create (FUN_6f2e4c40): a new profile by the typed name, chosen at once, and the
    /// panel closes.
    pub fn create(&mut self) -> Option<Event> {
        let name = self.text.trim().to_string();
        if name.is_empty() || self.profiles.find(&name).is_some() {
            return None;
        }
        self.profiles.add(&name);
        self.profiles.store();
        self.text.clear();
        self.open = false;
        Some(Event::Chosen)
    }

    /// The menu's buttons for the panel.
    pub fn button(&mut self, name: &str, tree: &mut FrameTree) -> Option<Event> {
        let e = match name {
            "ProfileButton" => {
                self.open = !self.open;
                None
            }
            "AddProfileButton" => self.create(),
            "DeleteProfileButton" => self.name().map(Event::ConfirmDelete),
            "SelectProfileButton" => {
                self.open = false;
                None
            }
            _ => return None,
        };
        self.sync(tree);
        e
    }

    /// Deletes the chosen profile (after its confirmation); the first left is chosen.
    pub fn delete(&mut self, tree: &mut FrameTree) -> Event {
        if let Some(k) = self.profiles.current {
            self.profiles.remove(k);
        }
        if self.profiles.current.is_none() && !self.profiles.entries.is_empty() {
            self.profiles.current = Some(0);
        }
        if self.profiles.entries.is_empty() {
            self.open = true;
        }
        self.profiles.store();
        self.sync(tree);
        Event::Chosen
    }

    /// The rows the list shows at once, once it is laid out.
    fn rows(tree: &FrameTree) -> Option<usize> {
        tree.get("ProfileListContainer").and_then(|i| tree.frames[i].rect).map(|c| ((c.h() - 2.0 * END) / ROW) as usize)
    }

    /// The rows past the first page, and the first row shown.
    fn past(&self, tree: &FrameTree) -> (usize, usize) {
        let past = Self::rows(tree).map_or(0, |n| self.profiles.entries.len().saturating_sub(n));
        (past, self.scroll.min(past))
    }

    /// The list's scroll bar shows only when its rows outgrow it, its range 0 to the rows
    /// past the first page, a row a step, its value from that at the top to 0 at the end
    /// (FUN_6f2ac2a0, as the map lists' bar); not before the list is laid out.
    fn set_bar(&self, tree: &mut FrameTree) {
        let Some(bar) = tree.get("ListScrollBar") else { return };
        let (past, first) = self.past(tree);
        tree.frames[bar].visible = past > 0;
        tree.frames[bar].range = (past > 0).then_some((0.0, past as f32, 1.0));
        tree.frames[bar].value = (past - first) as f32;
        tree.layout();
    }

    /// The pointer moved the list's scroll bar: the list follows it.
    pub fn follow_bar(&mut self, tree: &mut FrameTree) {
        let Some(bar) = tree.scrolled.filter(|&b| Some(b) == tree.get("ListScrollBar")) else { return };
        tree.scrolled = None;
        let past = tree.frames[bar].range.map_or(0.0, |r| r.1);
        self.scroll = (past - tree.frames[bar].value).round().max(0.0) as usize;
    }

    /// The mouse wheel over the open list scrolls it a row a notch, as the map list's.
    pub fn wheel(&mut self, tree: &mut FrameTree, x: f32, y: f32, lines: f32) {
        if !self.open || !self.list.is_some_and(|r| r.contains(x, y)) {
            return;
        }
        self.wheel_lines += lines;
        let notches = self.wheel_lines.trunc();
        self.wheel_lines -= notches;
        let (past, first) = self.past(tree);
        self.scroll = (first as i64 - notches as i64).clamp(0, past as i64) as usize;
        self.set_bar(tree);
    }

    /// A click in the list chooses that profile (FUN_6f2f97d0).
    pub fn click(&mut self, tree: &mut FrameTree, x: f32, y: f32) -> Option<Event> {
        let r = self.list.filter(|_| self.open)?;
        if !r.contains(x, y) {
            return None;
        }
        let k = ((r.y1 - y) / ROW) as usize + self.past(tree).1;
        if k >= self.profiles.entries.len() || self.profiles.current == Some(k) {
            return None;
        }
        self.profiles.current = Some(k);
        self.profiles.store();
        self.sync(tree);
        Some(Event::Chosen)
    }

    /// The list's rows, the chosen one highlighted.
    pub fn draws(&mut self, tree: &FrameTree, db: &FrameDb, out: &mut Vec<Draw>) {
        if !self.open {
            return;
        }
        let Some(c) = tree.get("ProfileListContainer").and_then(|i| tree.frames[i].rect) else { return };
        // The rows stop at the scroll bar while it shows (FUN_6f2ac2a0).
        let bar = tree.get("ListScrollBar").filter(|&b| tree.is_visible(b)).and_then(|b| tree.frames[b].rect).map_or(0.0, |r| r.w());
        let inner = Rect { x0: c.x0 + 0.012, x1: c.x1 - SIDE - bar, y0: c.y0 + END, y1: c.y1 - END };
        self.list = Some(inner);
        let font = db.decorate("MasterFont");
        let rows = (inner.h() / ROW) as usize;
        let first = self.past(tree).1;
        for (k, e) in self.profiles.entries.iter().enumerate().skip(first).take(rows) {
            let y1 = inner.y1 - (k - first) as f32 * ROW;
            let row = Rect { x0: inner.x0, x1: inner.x1, y0: y1 - ROW, y1 };
            let chosen = self.profiles.current == Some(k);
            if chosen {
                out.push(Draw::Image { texture: String::new(), rect: row, uv: [0.0, 0.0, 1.0, 1.0], color: [0.0, 0.0, 0.8, 0.8], blend: w3_ui::Blend::Alpha, rotated: false });
            }
            let color = if chosen { [1.0, 0.8, 0.0, 1.0] } else { [1.0; 4] };
            let tr = Rect { x0: row.x0 + 0.004, x1: row.x1, y0: row.y0, y1: row.y1 };
            out.push(Draw::Text { font: font.clone(), size: 0.0135, rect: tr, text: e.name.clone(), color, justify_h: Justify::Start, justify_v: Justify::Middle, shadow: None });
        }
    }
}
