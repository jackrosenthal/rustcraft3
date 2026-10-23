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
        // Scrolled to the newest lines.
        let rows = Self::rows(&tree, db);
        let total = Self::wrapped(&tree, db);
        tree.set_selected("LogArea", total.saturating_sub(rows));
        Some(LogView { tree })
    }

    /// The lines the log area shows at once.
    fn rows(tree: &FrameTree, db: &FrameDb) -> usize {
        let _ = db;
        let Some(i) = tree.get("LogArea") else { return 1 };
        let Some(r) = tree.frames[i].rect else { return 1 };
        let d = &tree.frames[i].def;
        let num = |k: &str, v: f32| d.prop(k).and_then(|p| p.args.first()).map_or(v, |a| a.num());
        let (inset, line_h, gap) = (num("TextAreaInset", 0.0), num("TextAreaLineHeight", 0.015), num("TextAreaLineGap", 0.0));
        (((r.h() - 2.0 * inset) / (line_h + gap)).floor() as usize).max(1)
    }

    /// The log's lines once wrapped to the area.
    fn wrapped(tree: &FrameTree, db: &FrameDb) -> usize {
        let Some(i) = tree.get("LogArea") else { return 0 };
        let Some(r) = tree.frames[i].rect else { return 0 };
        let (font, size) = tree.text_font(db, "LogArea").unwrap_or((db.decorate("MasterFont"), 0.011));
        let inset = tree.frames[i].def.prop("TextAreaInset").and_then(|p| p.args.first()).map_or(0.0, |a| a.num());
        let w = (r.w() - 2.0 * inset).max(0.0);
        tree.frames[i].text.as_deref().unwrap_or("").split('\n').map(|l| if l.trim().is_empty() { 1 } else { db.wrap(&font, size, l, w).len() }).sum()
    }

    /// A click: OK closes the log (returns true).
    pub fn click(&mut self, x: f32, y: f32) -> bool {
        self.tree.pointer_down(x, y);
        self.tree.pointer_up(x, y).is_some_and(|i| self.tree.frames[i].name == "LogOkButton")
    }

    pub fn pointer_move(&mut self, x: f32, y: f32) {
        self.tree.pointer_move(x, y);
    }

    pub fn draws(&self, db: &FrameDb) -> Vec<Draw> {
        self.tree.draw(db)
    }
}
