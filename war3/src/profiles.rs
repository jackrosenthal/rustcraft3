//! The Single Player menu's profile panel (CSinglePlayerMenu): the player's profiles,
//! a box to name a new one, and Create, Delete, and Select.  Without a profile the
//! panel stays open and the menu's other choices wait (FUN_6f30e6d0).

use w3_ui::{Draw, FrameDb, FrameTree, Justify, Rect};

use crate::data::{PROFILE_NAME_MAX, Profiles};

/// The menu's choices that need a profile, with the panel's own.
const NEEDS_PROFILE: [&str; 8] = ["SelectProfileButton", "DeleteProfileButton", "ProfileButton", "CampaignButton", "LoadSavedButton", "ViewReplayButton", "SkirmishButton", "CustomCampaignButton"];

const ROW: f32 = 0.0195;

pub struct ProfilePanel {
    pub open: bool,
    /// What has been typed in NewProfileEditBox.
    pub text: String,
    pub profiles: Profiles,
    /// The list's rows' rectangle.
    list: Option<Rect>,
}

/// What a click or key asks of the menu.
pub enum Event {
    /// The chosen profile changed.
    Chosen,
    /// Delete asks to confirm (DELETE_PROFILE_MESSAGE).
    ConfirmDelete(String),
}

impl ProfilePanel {
    pub fn new(legacy_name: &str) -> ProfilePanel {
        ProfilePanel { open: false, text: String::new(), profiles: Profiles::load(legacy_name), list: None }
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
                let e = self.create();
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

    /// A click in the list chooses that profile (FUN_6f2f97d0).
    pub fn click(&mut self, tree: &mut FrameTree, x: f32, y: f32) -> Option<Event> {
        let r = self.list.filter(|_| self.open)?;
        if !r.contains(x, y) {
            return None;
        }
        let k = ((r.y1 - y) / ROW) as usize;
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
        let inner = Rect { x0: c.x0 + 0.012, x1: c.x1 - 0.0125, y0: c.y0 + 0.008, y1: c.y1 - 0.008 };
        self.list = Some(inner);
        let font = db.decorate("MasterFont");
        let rows = (inner.h() / ROW) as usize;
        for (k, e) in self.profiles.entries.iter().enumerate().take(rows) {
            let y1 = inner.y1 - k as f32 * ROW;
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
