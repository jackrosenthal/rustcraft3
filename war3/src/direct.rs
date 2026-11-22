//! The main menu's Direct Connect (the port's own, in the original's Battle.net slot): a
//! popup, made as the glue's DialogWar3, asking for the address of a computer hosting a
//! game and, if it is not the game port, its port.  Connect asks that host for its game
//! and joins it (`lan::Lan::connect`), as the Local Area Network screen joins a game it
//! found.

use w3_ui::{FrameDb, FramePoint as P, FrameTree, Rect};

use crate::editline::{EditKey, EditLine};

/// The popup: DialogWar3's frames, scoped, and the boxes and labels made for it.
const DIALOG: &str = "DialogWar3Direct";
const STATUS: &str = "DialogTextDirect";
const CONNECT: &str = "DialogButtonYesDirect";
const CANCEL: &str = "DialogButtonNoDirect";
const HOST_LABEL: &str = "DirectHostLabel";
const HOST_BOX: &str = "DirectHostEditBox";
const PORT_LABEL: &str = "DirectPortLabel";
const PORT_BOX: &str = "DirectPortEditBox";

const HOST_LEN: usize = 63;
const PORT_LEN: usize = 5;

#[derive(Default)]
pub struct DirectConnect {
    pub open: bool,
    pub host: EditLine,
    pub port: EditLine,
    /// The port box has the focus (else the host box has).
    port_focus: bool,
    /// The line under the boxes: what Connect is doing, or why it failed.
    status: String,
    /// Connect is under way: only Cancel is taken.
    busy: bool,
}

/// What the popup asks of the glue.
#[derive(Debug, PartialEq, Eq)]
pub enum Event {
    /// Join the game at this host and port (the game port without one).
    Connect(String, Option<u16>),
    Cancel,
}

impl DirectConnect {
    /// Builds the popup over the main menu (once a tree) and opens it.
    pub fn open(&mut self, tree: &mut FrameTree, db: &FrameDb) {
        if tree.get(DIALOG).is_none() && !Self::build(tree, db) {
            log::warn!("direct connect: DialogWar3 is not defined");
            return;
        }
        self.open = true;
        self.busy = false;
        self.port_focus = false;
        self.status.clear();
        self.sync(tree);
    }

    fn build(tree: &mut FrameTree, db: &FrameDb) -> bool {
        let Some(dialog) = tree.create_scoped(db, "DialogWar3", 0, "Direct") else { return false };
        tree.create_like(db, STATUS, dialog, HOST_LABEL, &["SetPoint", "Width"]);
        tree.create_like(db, STATUS, dialog, PORT_LABEL, &["SetPoint", "Width"]);
        tree.create_like(db, "StandardEditBoxTemplate", dialog, HOST_BOX, &[]);
        tree.create_like(db, "StandardEditBoxTemplate", dialog, PORT_BOX, &[]);
        tree.bind_points();
        // In the middle of the screen, as CDialogWar3, a little taller for the boxes: the
        // labels over the boxes, the host's wide and the port's narrow beside it, and the
        // dialog's text under them as the status.
        tree.set_point(DIALOG, P::Center, "", P::Center, 0.0, 0.0);
        tree.set_size(DIALOG, Some(0.35), Some(0.23));
        tree.set_visible("DialogIconDirect", false);
        tree.set_point(HOST_LABEL, P::TopLeft, DIALOG, P::TopLeft, 0.035, -0.035);
        tree.set_text(HOST_LABEL, "Host address");
        tree.set_point(HOST_BOX, P::TopLeft, HOST_LABEL, P::BottomLeft, 0.0, -0.002);
        tree.set_size(HOST_BOX, Some(0.19), None);
        tree.set_point(PORT_BOX, P::Left, HOST_BOX, P::Right, 0.01, 0.0);
        tree.set_size(PORT_BOX, Some(0.09), None);
        tree.set_point(PORT_LABEL, P::BottomLeft, PORT_BOX, P::TopLeft, 0.0, 0.002);
        tree.set_text(PORT_LABEL, "Port (optional)");
        tree.set_point(STATUS, P::TopLeft, HOST_BOX, P::BottomLeft, 0.0, -0.008);
        tree.set_size(STATUS, Some(0.28), None);
        // Connect and Cancel are the dialog's Yes and No.
        tree.set_visible("DialogButtonOKBackdropDirect", false);
        tree.set_text("DialogButtonYesTextDirect", "Connect");
        tree.set_text("DialogButtonNoTextDirect", "Cancel");
        true
    }

    fn sync(&self, tree: &mut FrameTree) {
        tree.set_visible(DIALOG, self.open);
        tree.set_text(HOST_BOX, &self.host);
        tree.set_text(PORT_BOX, &self.port);
        tree.set_caret(HOST_BOX, (!self.port_focus && !self.busy).then(|| self.host.caret()));
        tree.set_caret(PORT_BOX, (self.port_focus && !self.busy).then(|| self.port.caret()));
        tree.set_text(STATUS, &self.status);
        tree.set_enabled(CONNECT, !self.busy);
        tree.layout();
    }

    /// Connect is under way, `status` saying so.
    pub fn wait(&mut self, tree: &mut FrameTree, status: String) {
        self.busy = true;
        self.status = status;
        self.sync(tree);
    }

    /// Connect failed, `status` saying why: the address can be changed and tried again.
    pub fn fail(&mut self, tree: &mut FrameTree, status: String) {
        self.busy = false;
        self.status = status;
        self.sync(tree);
    }

    pub fn close(&mut self, tree: &mut FrameTree) {
        self.open = false;
        self.busy = false;
        self.sync(tree);
    }

    /// Typing goes to the box with the focus (the port's takes digits only); Tab moves
    /// between them, Enter connects, Escape cancels.
    pub fn key(&mut self, c: char, tree: &mut FrameTree) -> Option<Event> {
        let event = match c {
            '\u{1b}' => Some(Event::Cancel),
            _ if self.busy => None,
            '\r' | '\n' => self.connect(),
            '\t' => {
                self.port_focus = !self.port_focus;
                None
            }
            '\u{8}' => {
                self.focused().backspace();
                None
            }
            c if self.port_focus => {
                if c.is_ascii_digit() && self.port.len() < PORT_LEN {
                    self.port.insert(c);
                }
                None
            }
            c => {
                if !c.is_control() && !c.is_whitespace() && self.host.chars().count() < HOST_LEN {
                    self.host.insert(c);
                }
                None
            }
        };
        self.sync(tree);
        event
    }

    fn focused(&mut self) -> &mut EditLine {
        if self.port_focus { &mut self.port } else { &mut self.host }
    }

    /// Left, Right, Home, End, or Delete in the box with the focus.
    pub fn edit_key(&mut self, key: EditKey, tree: &mut FrameTree) {
        if !self.busy {
            self.focused().key(key);
            self.sync(tree);
        }
    }

    /// A click in a box gives it the focus and puts its cursor there.
    pub fn click(&mut self, tree: &mut FrameTree, db: &FrameDb, x: f32, y: f32) {
        let inside = |name: &str| tree.get(name).and_then(|i| tree.frames[i].rect).is_some_and(|r: Rect| r.contains(x, y));
        let focus = if inside(PORT_BOX) {
            true
        } else if inside(HOST_BOX) {
            false
        } else {
            return;
        };
        self.port_focus = focus;
        let name = if focus { PORT_BOX } else { HOST_BOX };
        if let Some(k) = tree.caret_at(db, name, x) {
            self.focused().set_caret(k);
        }
        self.sync(tree);
    }

    pub fn button(&mut self, name: &str, tree: &mut FrameTree) -> Option<Event> {
        let event = match name {
            CONNECT if !self.busy => self.connect(),
            CANCEL => Some(Event::Cancel),
            _ => None,
        };
        self.sync(tree);
        event
    }

    fn connect(&mut self) -> Option<Event> {
        let port = match self.port.parse::<u16>() {
            _ if self.port.is_empty() => None,
            Ok(p) if p > 0 => Some(p),
            _ => {
                self.status = "The port is a number from 1 to 65535.".into();
                return None;
            }
        };
        Some(Event::Connect(self.host.trim().to_string(), port))
    }
}

