//! The Allies dialog of a network game (CAllianceDialog, AllianceDialog.fdf): a row
//! (AllianceSlot) per other player in the game, with their alliance, shared vision,
//! and shared units, and the gold and lumber to send them.  The map flags rule it
//! (FUN_6f3c66d0): with MAP_LOCK_ALLIANCE_CHANGES the check boxes only show the
//! alliances; MAP_LOCK_RESOURCE_TRADING stops trading, and
//! MAP_RESOURCE_TRADING_ALLIES_ONLY keeps it to allies.  A click on an amount adds
//! TradingIncSmall (TradingIncLarge with Shift), a right click takes it away, and all
//! the gold (or lumber) offered stays within what the player has (FUN_6f3c2f30).
//! Accept sends it (CNetCommandTeamTradeResources), and changed alliances
//! (CNetCommandTeamChangeAlliance: ally 0x1F, vision 0x20, units 0x40); shared units
//! need the alliance and shared vision (FUN_6f3bc530).

use w3_game::World;
use w3_game::types::{ALLIANCE_TYPES, player_state, slot_state};
use w3_ui::{Draw, FrameDb, FramePoint as P, FrameTree};

/// The alliance kinds behind each column: ally (passive through shared spells), shared
/// vision, shared units.
const ALLY: [usize; 5] = [0, 1, 2, 3, 4];
const VISION: usize = 5;
const UNITS: usize = 6;

struct Row {
    player: u8,
    gold: i32,
    lumber: i32,
    trade: bool,
    /// The alliance kinds held toward the player (bits 0 to 6).
    flags: u32,
}

pub enum Event {
    /// Send these players this gold and lumber, and change these alliances.
    Accept(Vec<(u8, i32, i32)>, Vec<(u8, u32)>),
    Cancel,
}

pub struct AllianceDialog {
    pub tree: FrameTree,
    rows: Vec<Row>,
    steps: (i32, i32),
    /// The player's gold and lumber, which the offers stay within.
    have: (i32, i32),
}

impl AllianceDialog {
    pub fn new(db: &FrameDb, w: &World, aspect: f32) -> Option<AllianceDialog> {
        let mut tree = FrameTree::new(aspect);
        tree.plain_button_text = true;
        let root = tree.create(db, "AllianceDialog", 0)?;
        tree.set_point("AllianceDialog", P::Top, "", P::Top, 0.0, -0.02);
        let local = w.local_player;
        let me = &w.players[local as usize];
        let mut rows = Vec::new();
        for p in 0..12u8 {
            let pl = &w.players[p as usize];
            if p == local || pl.slot_state == slot_state::EMPTY || pl.get_state(player_state::OBSERVER) != 0 {
                continue;
            }
            let k = rows.len();
            let scope = format!("#{k}");
            let Some(slot) = tree.create_scoped(db, "AllianceSlot", root, &scope) else { continue };
            let h = tree.frames[slot].height.unwrap_or(0.024);
            tree.set_point(&format!("AllianceSlot{scope}"), P::Top, "AllianceDialog", P::Top, 0.0, -((h + 0.003125) * k as f32 + 0.07375));
            tree.set_text(&format!("PlayerNameLabel{scope}"), &pl.name);
            if let Some(c) = tree.get(&format!("ColorBackdrop{scope}")) {
                tree.frames[c].texture = Some(format!("ReplaceableTextures\\TeamColor\\TeamColor{:02}.blp", pl.color));
            }
            let a = &me.alliances[p as usize % me.alliances.len()];
            let get = |t: usize| t < ALLIANCE_TYPES && a[t];
            let ally = ALLY.iter().all(|&t| get(t));
            let locked = w.map_flags & 0x400 != 0;
            for (name, on) in [("AllyCheckBox", ally), ("VisionCheckBox", get(VISION)), ("UnitsCheckBox", get(UNITS))] {
                let n = format!("{name}{scope}");
                tree.set_checked(&n, on);
                tree.set_enabled(&n, !locked);
            }
            let held = (0..=UNITS).filter(|&t| get(t)).fold(0u32, |f, t| f | 1 << t);
            let flags = w.map_flags;
            let trade = flags & 0x100 == 0 && (flags & 0x200 == 0 || ally);
            for (backdrop, key) in [("GoldBackdrop", "AllianceGold"), ("LumberBackdrop", "AllianceLumber")] {
                if let Some(b) = tree.get(&format!("{backdrop}{scope}")) {
                    tree.frames[b].texture = Some(db.decorate(&if trade { key.to_string() } else { format!("{key}Disabled") }));
                }
            }
            rows.push(Row { player: p, gold: 0, lumber: 0, trade, flags: held });
        }
        tree.set_enabled("AlliedVictoryCheckBox", false);
        let misc = |k: &str, d: i32| w.misc.get_str("Misc", k).and_then(|v| v.trim().parse().ok()).unwrap_or(d);
        let steps = (misc("TradingIncSmall", 100), misc("TradingIncLarge", 200));
        let have = (me.get_state(player_state::RESOURCE_GOLD), me.get_state(player_state::RESOURCE_LUMBER));
        let mut d = AllianceDialog { tree, rows, steps, have };
        d.sync();
        d.tree.bind_points();
        d.tree.fit_text_widths();
        d.tree.layout();
        Some(d)
    }

    fn sync(&mut self) {
        // Shared units need the alliance and shared vision.
        for k in 0..self.rows.len() {
            let both = self.tree.checked(&format!("AllyCheckBox#{k}")) && self.tree.checked(&format!("VisionCheckBox#{k}"));
            let units = format!("UnitsCheckBox#{k}");
            if !both {
                self.tree.set_checked(&units, false);
            }
            if self.tree.get(&format!("AllyCheckBox#{k}")).is_some_and(|i| self.tree.frames[i].enabled) {
                self.tree.set_enabled(&units, both);
            }
        }
        for (k, r) in self.rows.iter().enumerate() {
            let show = |v: i32| if r.trade { v.to_string() } else { String::new() };
            self.tree.set_text(&format!("GoldText#{k}"), &show(r.gold));
            self.tree.set_text(&format!("LumberText#{k}"), &show(r.lumber));
        }
    }

    /// A click: on an amount (`more` adds, else takes away; `large` with Shift), or the
    /// dialog's buttons.
    pub fn click(&mut self, x: f32, y: f32, more: bool, large: bool) -> Option<Event> {
        for k in 0..self.rows.len() {
            for (gold, frame) in [(true, "GoldBackdrop"), (false, "LumberBackdrop")] {
                let Some(r) = self.tree.get(&format!("{frame}#{k}")).and_then(|i| self.tree.frames[i].rect) else { continue };
                if !r.contains(x, y) || !self.rows[k].trade {
                    continue;
                }
                let step = if large { self.steps.1 } else { self.steps.0 } * if more { 1 } else { -1 };
                let offered: i32 = self.rows.iter().map(|r| if gold { r.gold } else { r.lumber }).sum();
                let have = if gold { self.have.0 } else { self.have.1 };
                let row = &mut self.rows[k];
                let v = if gold { &mut row.gold } else { &mut row.lumber };
                // Never below none, nor past what the player has in all.
                *v = (*v + step.min(have - offered)).max(0);
                self.sync();
                return None;
            }
        }
        if !more {
            return None;
        }
        self.tree.pointer_down(x, y);
        let i = self.tree.pointer_up(x, y)?;
        match self.tree.frames[i].name.as_str() {
            "AllianceAcceptButton" => {
                let sends = self.rows.iter().filter(|r| r.gold > 0 || r.lumber > 0).map(|r| (r.player, r.gold, r.lumber)).collect();
                let changes = self
                    .rows
                    .iter()
                    .enumerate()
                    .filter_map(|(k, r)| {
                        let on = |n: &str| self.tree.checked(&format!("{n}#{k}"));
                        let f = if on("AllyCheckBox") { 0x1f } else { 0 } | if on("VisionCheckBox") { 0x20 } else { 0 } | if on("UnitsCheckBox") { 0x40 } else { 0 };
                        (f != r.flags).then_some((r.player, f))
                    })
                    .collect();
                Some(Event::Accept(sends, changes))
            }
            "AllianceCancelButton" => Some(Event::Cancel),
            _ => {
                self.sync();
                None
            }
        }
    }

    pub fn draws(&self, db: &FrameDb, out: &mut Vec<Draw>) {
        out.extend(self.tree.draw(db));
    }
}
