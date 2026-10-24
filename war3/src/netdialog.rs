//! A network game's waiting dialogs: the host's SuspendDialog (CSuspendDialog, centered
//! 0.05 above the middle, WAITING_FOR_PLAYERS over a SuspendPlayerSlot per player
//! waited on, 0.048 below its top and the slot's height plus 0.003125 apart, with Drop
//! Players), and a player's UnresponsiveDialog (WAITING_FOR_HOST, with Disconnect).

use w3_ui::{Draw, FrameDb, FramePoint as P, FrameTree};

/// What a click on the dialog asks.
pub enum Event {
    Drop,
    Disconnect,
}

pub struct NetDialog {
    pub tree: FrameTree,
    /// The players shown (empty for the player's dialog), and their colors.
    players: Vec<(u8, i32)>,
}

impl NetDialog {
    /// The host's dialog for players it waits on: their names and colors.
    pub fn suspend(db: &FrameDb, aspect: f32, players: &[(u8, String, i32)]) -> Option<NetDialog> {
        let mut tree = FrameTree::new(aspect);
        tree.plain_button_text = true;
        let root = tree.create(db, "SuspendDialog", 0)?;
        tree.set_point("SuspendDialog", P::Center, "", P::Center, 0.0, 0.05);
        let mut shown = Vec::new();
        for (k, (p, name, color)) in players.iter().enumerate() {
            let scope = format!("#{k}");
            let Some(slot) = tree.create_scoped(db, "SuspendPlayerSlot", root, &scope) else { continue };
            let h = tree.frames[slot].height.unwrap_or(0.02);
            tree.set_point(&format!("SuspendPlayerSlot{scope}"), P::Top, "SuspendDialog", P::Top, 0.0, -((h + 0.003125) * k as f32 + 0.048));
            tree.set_text(&format!("SuspendPlayerSlotPlayerNameLabel{scope}"), name);
            // The color square shows the player's color under its border.
            if let Some(c) = tree.get(&format!("SuspendPlayerSlotColorBackdrop{scope}")) {
                tree.frames[c].texture = Some(format!("ReplaceableTextures\\TeamColor\\TeamColor{color:02}.blp"));
            }
            shown.push((*p, *color));
        }
        tree.bind_points();
        tree.fit_text_widths();
        tree.layout();
        Some(NetDialog { tree, players: shown })
    }

    /// A player's dialog while the host is silent.
    pub fn unresponsive(db: &FrameDb, aspect: f32) -> Option<NetDialog> {
        let mut tree = FrameTree::new(aspect);
        tree.plain_button_text = true;
        tree.create(db, "UnresponsiveDialog", 0)?;
        tree.set_point("UnresponsiveDialog", P::Center, "", P::Center, 0.0, 0.05);
        tree.bind_points();
        tree.fit_text_widths();
        tree.layout();
        Some(NetDialog { tree, players: Vec::new() })
    }

    /// Who is shown, to tell when the dialog needs remaking.
    pub fn players(&self) -> Vec<u8> {
        self.players.iter().map(|p| p.0).collect()
    }

    pub fn click(&mut self, x: f32, y: f32) -> Option<Event> {
        self.tree.pointer_down(x, y);
        let i = self.tree.pointer_up(x, y)?;
        match self.tree.frames[i].name.as_str() {
            "SuspendDropPlayersButton" => Some(Event::Drop),
            "UnresponsiveDisconnectButton" => Some(Event::Disconnect),
            _ => None,
        }
    }

    pub fn draws(&self, db: &FrameDb, out: &mut Vec<Draw>) {
        out.extend(self.tree.draw(db));
    }
}
