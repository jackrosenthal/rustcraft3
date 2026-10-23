//! The in-game console: the race-skinned console art, resource bar, and upper button bar,
//! placed the way CGameUI's constructor (game.dll 0x6f367710) places them.

use w3_game::World;
use w3_game::types::player_state;
use w3_ui::{Draw, FrameDb, FramePoint, FrameTree, Point};

pub struct Hud {
    pub tree: FrameTree,
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

    pub fn draws(&self, db: &FrameDb) -> Vec<Draw> {
        self.tree.draw(db)
    }
}
