//! A network game's chat dialog (CChatDialog, UI\FrameDef\UI\ChatDialog.fdf), opened by
//! the Chat button or F12 in place of the message log: whom Enter's chat goes to (a
//! player, allies, observers, or everyone, CChatDialog's radio buttons), and the chat so
//! far in ChatHistory's font; OK keeps the choice.

use w3_game::World;
use w3_ui::{Draw, FrameDb, FramePoint as P, FrameTree};

const RADIOS: [&str; 4] = ["ChatPlayerRadioButton", "ChatAlliesRadioButton", "ChatObserversRadioButton", "ChatEveryoneRadioButton"];

pub struct ChatDialog {
    pub tree: FrameTree,
    /// The other players the player menu lists.
    players: Vec<u8>,
    radio: usize,
}

impl ChatDialog {
    /// The dialog centered 0.05 above the screen's middle (CChatMode's constructor), with
    /// the recipient chosen (0 all, 1 allies, 2 observers, 3 on a player).
    pub fn open(db: &FrameDb, aspect: f32, w: &World, history: &[String], recipient: u8) -> Option<ChatDialog> {
        let mut tree = FrameTree::new(aspect);
        tree.plain_button_text = true;
        tree.create(db, "ChatDialog", 0)?;
        tree.set_point("ChatDialog", P::Center, "", P::Center, 0.0, 0.05);
        let me = w.local_player;
        let players: Vec<u8> = (0..12u8).filter(|&p| p != me && w.players[p as usize].slot_state == w3_game::types::slot_state::PLAYING).collect();
        let names = players.iter().map(|&p| w.player_name(p)).collect();
        // FUN_6f3b6440's mapping, backward: the player radio for a player, else allies,
        // observers, or everyone.
        let (radio, pick) = match recipient {
            0 => (3, 0),
            1 => (1, 0),
            2 => (2, 0),
            n => (0, players.iter().position(|&p| p + 3 == n).unwrap_or(0)),
        };
        tree.set_items("ChatPlayerMenu", names, pick);
        tree.set_text("ChatHistoryDisplay", &history.join("\n"));
        if let Some(i) = tree.get("ChatHistoryDisplay") {
            tree.frames[i].selected = history.len().saturating_sub(12);
        }
        tree.place_text_area_scroll_bars();
        tree.bind_points();
        tree.fit_text_widths();
        tree.layout();
        let mut d = ChatDialog { tree, players, radio };
        d.sync();
        Some(d)
    }

    fn sync(&mut self) {
        for (k, r) in RADIOS.iter().enumerate() {
            self.tree.set_checked(r, k == self.radio);
        }
        self.tree.set_enabled("ChatPlayerMenu", self.radio == 0 && !self.players.is_empty());
    }

    /// A click; OK gives the recipient chosen.
    pub fn click(&mut self, x: f32, y: f32) -> Option<u8> {
        self.tree.pointer_down(x, y);
        let i = self.tree.pointer_up(x, y)?;
        let name = self.tree.frames[i].name.clone();
        if let Some(k) = RADIOS.iter().position(|r| *r == name) {
            self.radio = k;
            self.sync();
            return None;
        }
        if name != "ChatAcceptButton" {
            return None;
        }
        Some(match self.radio {
            0 => {
                let k = self.tree.get("ChatPlayerMenu").map_or(0, |i| self.tree.frames[i].selected);
                self.players.get(k).map_or(0, |&p| p + 3)
            }
            1 => 1,
            2 => 2,
            _ => 0,
        })
    }

    pub fn draws(&self, db: &FrameDb, out: &mut Vec<Draw>) {
        out.extend(self.tree.draw(db));
    }
}
