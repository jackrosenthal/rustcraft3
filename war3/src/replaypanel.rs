//! Watching a replay: the panel in the command card's place (CReplayPanel,
//! ReplayPanel.fdf SimpleReplayPanel) and what it controls.  Four CReplayButtons, 0.032
//! squares 0.0346 apart from 0.003 above the panel's lower left (FUN_6f3b8f70), pause,
//! speed it up and down, and restart it; the status bar and elapsed time show how far it
//! has played (FUN_6f3c4ba0), the speed reads "%dX" or "%d/%dX" (FUN_6f3d8550) from 1/2X
//! to 8X (FUN_6f32d080); the vision menu lists the players in their colors and chooses
//! whose sight the interface shows (FUN_6f3dbcd0), and the fog check box shows or lifts
//! the fog of war.  The game itself runs as it was recorded: the viewed player and fog
//! are the interface's, and the game's own are put back while it steps.

use w3_game::World;
use w3_ui::{Draw, FrameDb, FramePoint, FrameTree, Point, Rect};

/// The buttons' skin icons, enabled and disabled, and their tooltips (FUN_6f3b4e00).
const BUTTONS: [(&str, &str, &str); 4] = [
    ("ReplayPauseIcon", "ReplayPauseIconDisabled", "REPLAY_TOOLTIP_PAUSE"),
    ("ReplaySpeedUpIcon", "ReplaySpeedUpIconDisabled", "REPLAY_TOOLTIP_SPEED_UP"),
    ("ReplaySpeedDownIcon", "ReplaySpeedDownIconDisabled", "REPLAY_TOOLTIP_SPEED_DOWN"),
    ("ReplayRestartIcon", "ReplayRestartIconDisabled", "REPLAY_TOOLTIP_RESTART"),
];

/// What a click on the panel asks of the scene.
pub enum Event {
    Restart,
}

pub struct ReplayPanel {
    pub paused: bool,
    /// The speed as a fraction: 1 to 8 over 1 or 2.
    pub speed: (u32, u32),
    /// The players the vision menu lists, in order, and the one shown.
    players: Vec<u8>,
    pub view: u8,
    /// Whether the fog of war shows.
    pub fog: bool,
    /// The game's own local player and fog settings, kept while the interface shows its.
    local: u8,
    real_fog: (bool, bool),
    /// The replay's length in seconds.
    length: f32,
}

impl ReplayPanel {
    /// Builds the panel in the command card's place (CARD_W by CARD_H, its lower right
    /// `right` in from the console's and `bottom` up).
    pub fn new(tree: &mut FrameTree, db: &FrameDb, world: &World, team_rgb: &[[f32; 3]; 16], card: (f32, f32, f32, f32), length: f32) -> ReplayPanel {
        let (w, h, right, bottom) = card;
        if let Some(i) = tree.create(db, "SimpleReplayPanel", 0) {
            let f = &mut tree.frames[i];
            f.all_points = false;
            f.width = Some(w);
            f.height = Some(h);
            f.points = vec![Point { point: FramePoint::BottomRight, relative: None, relative_point: FramePoint::BottomRight, x: -right, y: bottom }];
            tree.bind_points();
            tree.layout();
        }
        // The players in their slots, not observers (FUN_6f3dbcd0).
        let players: Vec<u8> = (0..12u8).filter(|&p| world.players.get(p as usize).is_some_and(|pl| pl.slot_state == 1)).collect();
        let items = players
            .iter()
            .map(|&p| {
                let pl = &world.players[p as usize];
                let c = team_rgb[pl.color as usize % 16].map(|v| (v * 255.0).round() as u8);
                format!("|cff{:02x}{:02x}{:02x}{}|r", c[0], c[1], c[2], pl.name)
            })
            .collect();
        let local = world.local_player;
        let selected = players.iter().position(|&p| p == local).unwrap_or(0);
        tree.set_items("ReplayVisionMenu", items, selected);
        tree.set_checked("ReplayFogCheckBox", true);
        tree.set_checked("ReplayCameraCheckBox", false);
        ReplayPanel { paused: false, speed: (1, 1), view: players.get(selected).copied().unwrap_or(local), players, fog: true, local, real_fog: (world.fog_enabled, world.fog_mask_enabled), length }
    }

    /// How much game time a frame of `dt` plays.
    pub fn scaled(&self, dt: f32) -> f32 {
        if self.paused { 0.0 } else { dt * self.speed.0 as f32 / self.speed.1 as f32 }
    }

    /// The game's own player and fog, for it to step.
    pub fn enter(&self, w: &mut World) {
        w.local_player = self.local;
        (w.fog_enabled, w.fog_mask_enabled) = self.real_fog;
    }

    /// The interface's player and fog, kept until the next step.
    pub fn leave(&mut self, w: &mut World) {
        self.real_fog = (w.fog_enabled, w.fog_mask_enabled);
        w.local_player = self.view;
        if !self.fog {
            w.fog_enabled = false;
        }
    }

    fn faster(&mut self) {
        let (n, d) = self.speed;
        self.speed = if d > 1 { (n, d - 1) } else { ((n + 1).min(8), d) };
    }

    fn slower(&mut self) {
        let (n, d) = self.speed;
        self.speed = if n > 1 { (n - 1, d) } else { (n, (d + 1).min(2)) };
    }

    /// The elapsed time, speed, and bar.
    pub fn sync(&self, tree: &mut FrameTree, elapsed: f32) {
        let s = elapsed.max(0.0) as u32;
        tree.set_text("ReplayElapsedTime", &format!("{:02}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60));
        let (n, d) = self.speed;
        tree.set_text("ReplaySpeedMultiplier", &if d == 1 { format!("{n}X") } else { format!("{n}/{d}X") });
        tree.set_value("ReplayStatusBar", if self.length > 0.0 { elapsed / self.length } else { 0.0 });
        // The row's strings follow one another, each as wide as its text.
        for name in ["ReplayElapsedTimeLabel", "ReplayElapsedTime", "ReplaySpeedAtLabel", "ReplaySpeedMultiplier", "ReplaySpeedLabel"] {
            if let Some(i) = tree.get(name) {
                tree.frames[i].width = None;
            }
        }
        tree.fit_text_widths();
        tree.layout();
    }

    fn button_rect(tree: &FrameTree, k: usize) -> Option<Rect> {
        let r = tree.get("SimpleReplayPanel").and_then(|i| tree.frames[i].rect)?;
        let x0 = r.x0 + k as f32 * 0.0346;
        let y0 = r.y0 + 0.003;
        Some(Rect { x0, x1: x0 + 0.032, y0, y1: y0 + 0.032 })
    }

    pub fn button_at(tree: &FrameTree, x: f32, y: f32) -> Option<usize> {
        (0..BUTTONS.len()).find(|&k| Self::button_rect(tree, k).is_some_and(|r| r.contains(x, y)))
    }

    /// The tooltip of the button under the pointer.
    pub fn tip(tree: &FrameTree, db: &FrameDb, x: f32, y: f32) -> Option<String> {
        Self::button_at(tree, x, y).map(|k| db.string(BUTTONS[k].2))
    }

    /// A click on the panel; returns whether it took it.
    pub fn click(&mut self, tree: &mut FrameTree, x: f32, y: f32, event: &mut Option<Event>) -> bool {
        if let Some(k) = Self::button_at(tree, x, y) {
            match k {
                0 => self.paused = !self.paused,
                1 => self.faster(),
                2 => self.slower(),
                _ => *event = Some(Event::Restart),
            }
            return true;
        }
        let open = tree.open_popup().is_some();
        let Some(i) = tree.pointer_down(x, y) else { return open };
        if let Some(i) = tree.pointer_up(x, y).or(Some(i)) {
            match tree.frames[i].name.as_str() {
                "ReplayVisionMenu" => {
                    if let Some(&p) = self.players.get(tree.frames[i].selected) {
                        self.view = p;
                    }
                }
                "ReplayFogCheckBox" => self.fog = tree.checked("ReplayFogCheckBox"),
                // The camera follows the viewed player's, which replays here do not keep.
                "ReplayCameraCheckBox" => tree.set_checked("ReplayCameraCheckBox", false),
                _ => {}
            }
        }
        true
    }

    /// The four buttons' icons.
    pub fn draws(tree: &FrameTree, db: &FrameDb, out: &mut Vec<Draw>) {
        for (k, (icon, _, _)) in BUTTONS.iter().enumerate() {
            if let Some(rect) = Self::button_rect(tree, k) {
                out.push(Draw::Image { texture: db.decorate(icon), rect, uv: [0.0, 0.0, 1.0, 1.0], color: [1.0; 4], blend: w3_ui::Blend::Alpha, rotated: false });
            }
        }
    }

    /// The + and - keys change the speed.
    pub fn key(&mut self, c: char) -> bool {
        match c {
            '+' | '=' => self.faster(),
            '-' => self.slower(),
            _ => return false,
        }
        true
    }
}
