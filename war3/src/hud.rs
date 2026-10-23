//! The in-game console: the race-skinned console art, resource bar, and upper button bar,
//! placed the way CGameUI's constructor (game.dll 0x6f367710) places them.

use w3_game::World;
use w3_game::types::player_state;
use w3_ui::{Draw, FrameDb, FramePoint, FrameTree, Point};

/// CCommandBar (game.dll 0x6f39d410): a 0.1745 x 0.129 grid of 4 x 3 buttons of 0.038,
/// anchored to the console's bottom right at (-0.007375, 0.002).
const CARD_W: f32 = 0.1745;
const CARD_H: f32 = 0.129;
const CARD_RIGHT: f32 = 0.007375;
const CARD_BOTTOM: f32 = 0.002;
const BUTTON: f32 = 0.038;

pub struct Hud {
    pub tree: FrameTree,
    pub card: Vec<crate::card::Button>,
    pub page: crate::card::Page,
    gold: Option<usize>,
    lumber: Option<usize>,
    supply: Option<usize>,
    upkeep: Option<usize>,
}

fn race_skin(race: i32) -> &'static str {
    match race {
        2 => "Orc",
        3 => "Undead",
        4 => "NightElf",
        _ => "Human",
    }
}

impl Hud {
    pub fn new(db: &mut FrameDb, aspect: f32, race: i32) -> Hud {
        db.skin = race_skin(race).to_string();
        // The console art is authored for 4:3; keep it at 4:3 and centered.
        let mut tree = FrameTree::new(4.0 / 3.0);
        tree.width = w3_ui::UI_HEIGHT * aspect.max(4.0 / 3.0);
        let x0 = (tree.width - w3_ui::UI_WIDTH) / 2.0;
        if let Some(r) = tree.frames[0].rect.as_mut() {
            r.x0 = x0;
            r.x1 = x0 + w3_ui::UI_WIDTH;
        }
        tree.create(db, "ConsoleUI", 0);
        let bar = tree.create(db, "ResourceBarFrame", 0);
        let upper = tree.create(db, "UpperButtonBarFrame", 0);
        tree.bind_points();
        let pin = |tree: &mut FrameTree, f: Option<usize>, p: FramePoint| {
            if let Some(f) = f {
                tree.frames[f].points = vec![Point { point: p, relative: None, relative_point: p, x: 0.0, y: 0.0 }];
            }
        };
        pin(&mut tree, bar, FramePoint::TopRight);
        pin(&mut tree, upper, FramePoint::TopLeft);
        tree.layout();
        Hud {
            card: Vec::new(),
            page: crate::card::Page::Main,
            gold: tree.get("ResourceBarGoldText"),
            lumber: tree.get("ResourceBarLumberText"),
            supply: tree.get("ResourceBarSupplyText"),
            upkeep: tree.get("ResourceBarUpkeepText"),
            tree,
        }
    }

    pub fn update(&mut self, db: &FrameDb, world: &World) {
        let Some(p) = world.players.get(world.local_player as usize) else { return };
        let set = |tree: &mut FrameTree, f: Option<usize>, s: String| {
            if let Some(f) = f {
                tree.frames[f].text = Some(s);
            }
        };
        set(&mut self.tree, self.gold, p.get_state(player_state::RESOURCE_GOLD).to_string());
        set(&mut self.tree, self.lumber, p.get_state(player_state::RESOURCE_LUMBER).to_string());
        set(
            &mut self.tree,
            self.supply,
            format!("{}/{}", p.get_state(player_state::RESOURCE_FOOD_USED), p.get_state(player_state::RESOURCE_FOOD_CAP)),
        );
        // The thresholds between low and high upkeep are not yet recovered from game.dll;
        // the label follows the player's gold upkeep rate state.
        let rate = p.get_state(player_state::GOLD_UPKEEP_RATE);
        let key = match rate {
            0 => "UPKEEP_NONE",
            r if r < 60 => "UPKEEP_LOW",
            _ => "UPKEEP_HIGH",
        };
        set(&mut self.tree, self.upkeep, db.string(key));
    }

    /// Interface rectangle of a command card slot.
    pub fn slot_rect(&self, x: u32, y: u32) -> w3_ui::Rect {
        let root = self.tree.frames[0].rect.unwrap_or_default();
        let (x1, y0) = (root.x1 - CARD_RIGHT, root.y0 + CARD_BOTTOM);
        let (x0, y1) = (x1 - CARD_W, y0 + CARD_H);
        let (cw, ch) = (CARD_W / 4.0, CARD_H / 3.0);
        let cx = x0 + cw * (x as f32 + 0.5);
        let cy = y1 - ch * (y as f32 + 0.5);
        w3_ui::Rect { x0: cx - BUTTON / 2.0, x1: cx + BUTTON / 2.0, y0: cy - BUTTON / 2.0, y1: cy + BUTTON / 2.0 }
    }

    /// The card button at an interface position.
    pub fn button_at(&self, x: f32, y: f32) -> Option<&crate::card::Button> {
        self.card.iter().find(|b| self.slot_rect(b.x, b.y).contains(x, y))
    }

    pub fn draws(&self, db: &FrameDb) -> Vec<Draw> {
        let mut out = self.tree.draw(db);
        for b in &self.card {
            let icon = if b.enabled { b.icon.clone() } else { crate::card::disabled_icon(&b.icon) };
            out.push(Draw::Image {
                texture: icon,
                rect: self.slot_rect(b.x, b.y),
                uv: [0.0, 0.0, 1.0, 1.0],
                color: [1.0; 4],
                blend: w3_ui::Blend::Alpha,
                rotated: false,
            });
        }
        out
    }
}
