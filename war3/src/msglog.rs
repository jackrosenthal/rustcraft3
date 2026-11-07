//! The message log (CLogDialog, UI\FrameDef\UI\LogDialog.fdf), opened by the Log
//! button or F12 in a single-player game, which it pauses: every text message shown and
//! every transmission ("Name: text", the name in |cffccac12, FUN_6f374e30) in its
//! LogArea, the newest at the bottom; OK closes it.

use w3_ui::{Draw, FrameDb, FramePoint as P, FrameTree};

pub struct LogView {
    pub tree: FrameTree,
}

/// A transmission's log line (FUN_6f374e30).
pub fn transmission_line(speaker: &str, text: &str) -> String {
    if speaker.is_empty() {
        text.to_string()
    } else {
        format!("|c{:02x}{:02x}{:02x}{:02x}{speaker}: |r{text}", 0xff, 0xcc, 0xac, 0x12)
    }
}

impl LogView {
    /// The dialog centered 0.05 above the screen's middle (CChatMode's constructor).
    pub fn open(db: &FrameDb, aspect: f32, lines: &[String]) -> Option<LogView> {
        let mut tree = FrameTree::new(aspect);
        tree.plain_button_text = true;
        tree.create(db, "LogDialog", 0)?;
        tree.set_point("LogDialog", P::Center, "", P::Center, 0.0, 0.05);
        tree.place_text_area_scroll_bars();
        tree.bind_points();
        tree.fit_text_widths();
        tree.layout();
        tree.set_text("LogArea", &lines.join("\n"));
        // Scrolled to the newest lines, its scroll bar at its end.
        let past = tree.text_area_past(db, "LogArea");
        tree.set_selected("LogArea", past);
        tree.sync_text_area_bars(db);
        Some(LogView { tree })
    }

    /// A click: OK closes the log (returns true); its scroll bar's arrows and track
    /// scroll it.
    pub fn click(&mut self, x: f32, y: f32) -> bool {
        self.tree.pointer_down(x, y);
        self.tree.pointer_up(x, y).is_some_and(|i| self.tree.frames[i].name == "LogOkButton")
    }

    pub fn pointer_move(&mut self, x: f32, y: f32) {
        self.tree.pointer_move(x, y);
    }

    /// The mouse wheel over the log scrolls it a line a notch; whether it did.
    pub fn wheel(&mut self, x: f32, y: f32, notches: i32) -> bool {
        self.tree.wheel_text_area(x, y, notches)
    }

    pub fn draws(&self, db: &FrameDb) -> Vec<Draw> {
        self.tree.draw(db)
    }
}
