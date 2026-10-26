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

/// CMinimap (game.dll 0x6f39f8c0 as placed by CGameUI): from (0.009375, 0.006875) to
/// (0.14875, 0.145) of the console.
/// The info panel's icons (CInfoPanelUnitDetail, in its order) and the scope each
/// instance's frame names take.
const INFO_ICONS: [(&str, &str); 8] = [
    ("SimpleInfoPanelIconDamage", "Damage0"),
    ("SimpleInfoPanelIconDamage", "Damage1"),
    ("SimpleInfoPanelIconArmor", "Armor"),
    ("SimpleInfoPanelIconRank", "Rank"),
    ("SimpleInfoPanelIconFood", "Food"),
    ("SimpleInfoPanelIconGold", "Gold"),
    ("SimpleInfoPanelIconHero", "Hero"),
    ("SimpleInfoPanelIconAlly", "Ally"),
];

const MINIMAP: [f32; 4] = [0.009375, 0.006875, 0.14875, 0.145];

/// The run-time texture holding the minimap's fog of war.
pub const MINIMAP_FOG: &str = "@minimapfog";
/// Its resolution.
pub const MINIMAP_FOG_SIZE: u32 = 128;
/// The minimap's image with the blight shaded in, made at run time.
pub const MINIMAP_TERRAIN: &str = "@minimapterrain";

/// What the minimap shows: the map's playable area, letterboxed to a square.
#[derive(Clone, Copy, Debug)]
pub struct MinimapArea {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

impl MinimapArea {
    pub fn square(&self) -> (f32, f32, f32) {
        let (w, h) = (self.x1 - self.x0, self.y1 - self.y0);
        let side = w.max(h);
        (self.x0 - (side - w) / 2.0, self.y0 - (side - h) / 2.0, side)
    }
}

/// Hit point text color by life fraction (CPortraitButton, game.dll 0x6f3d8fd0).
fn life_color(p: f32) -> [f32; 4] {
    if p > 0.6 {
        [(255.0 - (p - 0.5) * 255.0 * 2.0).clamp(0.0, 255.0) / 255.0, 1.0, 0.0, 1.0]
    } else {
        let scale = if p > 0.3 { 0.6 } else { 0.8 };
        [1.0, ((p / scale) * 255.0).clamp(0.0, 255.0) / 255.0, 0.0, 1.0]
    }
}

/// One unit in the group panel: its icon, life and mana fractions, and whether it is
/// in the active subgroup.
#[derive(Clone, Debug)]
pub struct GroupSlot {
    pub unit: u32,
    pub icon: String,
    pub life: f32,
    pub mana: Option<f32>,
    pub active: bool,
    /// The icon's color (FUN_6f3da4e0): illusions 0xff1460ff, the animated dead
    /// 0xffa04040, the ethereal 0xffc0ff80, those under an enemy's dispellable spell red
    /// (Tip49), others white.
    pub color: [f32; 4],
}

/// A script dialog on screen: its handle, the message and button labels it was built
/// from, its frames, and each button frame's handle.
pub struct DialogView {
    pub dialog: u32,
    pub content: (String, Vec<String>),
    pub tree: FrameTree,
    pub buttons: Vec<(usize, u32)>,
}

/// Timed text in one of the world frame's message areas.
#[derive(Clone, Debug)]
pub struct Message {
    pub text: String,
    /// Scene time when it disappears.
    pub until: f32,
}

pub struct Hud {
    pub tree: FrameTree,
    /// The chat edit bar's text, while it is open (CChatEditBar), and its recipient (0
    /// all, 1 allies, 2 observers) and prompt (COLON_MESSAGE_*).
    pub chat: Option<String>,
    pub chat_to: u8,
    pub chat_prompt: String,
    /// Scene time, for message expiry.
    pub now: f32,
    /// WorldFrameUnitMessage: trigger text, oldest first.
    pub unit_messages: Vec<Message>,
    /// Chat lines and the game's notices under a player's name (CGameUI +0x3ec).
    pub chat_messages: Vec<Message>,
    /// The unit message frame's placement from the last DisplayTextToPlayer x and y.
    pub unit_message_offset: (f32, f32),
    /// WorldFrameMessage: the error line above the console.
    pub error_message: Option<Message>,
    /// "Select Target" in the message frame while choosing a target (CTargetMode::vf9),
    /// and whether target mode was on.
    target_prompt: bool,
    targeting: bool,
    pub dialog: Option<DialogView>,
    /// Timer dialogs, the multiboard, and the leaderboard.
    pub boards: Option<crate::boards::BoardsView>,
    /// The Team Resources board is minimized.
    pub team_board_minimized: bool,
    /// The game menu (F10), built by CEscMenu: EscMenuBackdrop centered 0.06 above the
    /// middle, holding EscMenuMainPanel.
    pub menu: Option<FrameTree>,
    /// A transport's cargo grid: rows, columns, and each aboard unit's icon, life, and
    /// the slots it takes.
    pub cargo: Option<(usize, usize, Vec<(String, f32, usize, u32)>)>,
    /// The quest log (F9), while open.
    pub quests: Option<crate::quests::QuestView>,
    /// The message log (F12), while open, and its lines; the transmission last logged.
    pub log_view: Option<crate::msglog::LogView>,
    pub log: Vec<String>,
    logged_scene: Option<(f32, String)>,
    /// QuestChangedParticles on the Quests button until this time (FUN_6f3b0ce0).
    pub quest_indicator: Option<f32>,
    /// The game menu's Save Game or Load Game panel, while shown.
    pub save_panel: Option<crate::saveload::SavePanel>,
    /// The selected unit's inventory: icon and charges per slot it has.
    pub inventory: Vec<Option<(String, i32)>>,
    /// Each inventory slot's tooltip (CInventoryBar::vf28).
    pub inventory_tips: Vec<Option<crate::card::Button>>,
    /// Each inventory slot's share of its item's cooldown gone, while it cools.
    pub inventory_cooldowns: Vec<Option<f32>>,
    /// The interface's other elements that show a tooltip under the pointer: where
    /// each is, and its tip.
    hover_tips: Vec<(w3_ui::Rect, crate::card::Button)>,
    /// The Tooltips option (0x2a): whether tooltips show their full text.
    pub tooltips: bool,
    /// The unit detail's icons' tooltips, by the icon's frame.
    info_tips: Vec<(String, crate::card::Button)>,
    /// Text under the portrait: hit points (with color) and mana.
    pub portrait_stats: Option<(String, [f32; 4], Option<String>)>,
    info: Option<usize>,
    /// The hero level bar's experience fraction within the level, when a hero is shown.
    level_bar: Option<f32>,
    /// CBuffBar: the shown unit's buff icons (Buffart), when it has the bar.
    buff_icons: Option<Vec<String>>,
    /// CInfoPanelBuildingDetail (frame names scoped "B"), shown while a structure
    /// produces, and what it is producing.
    building: Option<usize>,
    production: Option<w3_game::info::Production>,
    /// CInfoPanelGroup: the selection when it has more than one unit.
    pub group: Vec<GroupSlot>,
    /// The pointer in interface units, for tooltips.
    pub pointer: (f32, f32),
    /// CinematicPanel while the interface is hidden (ShowInterface false), and the cine
    /// filter overlay: texture, texture coordinates, color, and additive blending.
    pub cine: Option<FrameTree>,
    /// CHeroBar: the local player's heroes (unit, icon, life and mana fractions, and
    /// whether it has skill points to spend).
    pub heroes: Vec<(u32, String, f32, Option<f32>, bool)>,
    /// Each hero button's tooltip.
    hero_tips: Vec<crate::card::Button>,
    /// When each hero's button began to flash for an attack (FUN_6f3b0920).
    pub hero_flash: std::collections::HashMap<u32, f32>,
    /// CPeonBar: the local player's idle workers, and the last one its button chose
    /// and when.
    pub idle_workers: Vec<u32>,
    /// The game is paused: the command card, hero, and idle worker buttons show
    /// disabled (as the original greys them while paused).
    pub paused: bool,
    idle_pick: (f32, usize),
    pub cinematic: bool,
    /// How much of the game interface and of the cinematic panel shows, while
    /// ShowInterface fades between them.
    pub fades: (f32, f32),
    filter: Option<(String, [f32; 4], [f32; 4], bool)>,
    pub minimap_area: Option<MinimapArea>,
    pub minimap_dots: Vec<(f32, f32, [f32; 4], f32)>,
    /// Creep camps on the minimap (CMinimap, FUN_6f3d6100): position, color, and scale
    /// (negative for a tough camp).
    pub minimap_camps: Vec<(f32, f32, [f32; 4], f32)>,
    /// [Minimap] MinimapColorBorder, the playable area's outline.
    pub minimap_border: [f32; 4],
    /// Units using the alternate minimap icon (UnitSetUsesAltIcon), and that icon
    /// (SetAltMinimapIcon).
    pub minimap_alt: Vec<(f32, f32)>,
    pub alt_icon: Option<String>,
    /// war3map.mmp: icons stamped on the minimap image (kind, x and y on its 256-pixel
    /// grid): 0 gold mine, 1 neutral building, 2 start location.
    pub minimap_icons: Vec<(i32, f32, f32)>,
    /// Which of the icons show: those where the local player has explored, or all where
    /// masked areas are partially visible (as the original hides an unexplored
    /// fountain's on NightElfX01 but shows BootyBay's mines from the start).
    pub minimap_icons_shown: Vec<bool>,
    /// The minimap buttons' states: terrain shown, ally color mode (0 player colors, 1
    /// ally and enemy colors on the minimap, 2 also in the world), creep camps shown,
    /// and formation movement.
    pub minimap_terrain: bool,
    pub ally_mode: u8,
    pub show_creeps: bool,
    /// EnableMinimapFilterButtons: the ally color and creep buttons usable.
    pub filter_buttons: (bool, bool),
    pub formation: bool,
    /// Whether the "@minimapfog" overlay texture is current.
    pub minimap_fog: bool,
    /// Whether the minimap's image with its blight (MINIMAP_TERRAIN) stands for the
    /// map's.
    pub minimap_blighted: bool,
    pub camera_box: Vec<(f32, f32)>,
    pub card: Vec<crate::card::Button>,
    pub page: crate::card::Page,
    /// While a replay plays, its panel in the command card's place.
    pub replay: Option<crate::replaypanel::ReplayPanel>,
    /// A network game's waiting dialog, while it waits.
    pub net_dialog: Option<crate::netdialog::NetDialog>,
    /// The Allies dialog, while open.
    pub alliance: Option<crate::alliance::AllianceDialog>,
    /// A network game's chat dialog, while open, and the chat lines so far.
    pub chat_dialog: Option<crate::chatdialog::ChatDialog>,
    pub chat_history: Vec<String>,
    /// Players whose chat is not shown (/squelch), and whether other players' minimap
    /// signals are hidden (/hideminimapsignals).
    pub squelched: Vec<u8>,
    pub hide_signals: bool,
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
        // A destructable's or an item's detail takes the same place.
        for panel in ["SimpleInfoPanelDestructableDetail", "SimpleInfoPanelItemDetail"] {
            if let Some(i) = tree.create(db, panel, 0) {
                tree.frames[i].all_points = false;
                tree.frames[i].width = Some(0.188125);
                tree.frames[i].height = Some(0.114063);
                tree.frames[i].points = vec![Point { point: FramePoint::BottomLeft, relative: None, relative_point: FramePoint::BottomLeft, x: 0.309375, y: 0.0 }];
                tree.frames[i].visible = false;
            }
        }
        // The info panel's unit detail fills CInfoBar: 0.188125 x 0.114063 at (0.309375, 0).
        let mut building = None;
        let info = tree.create(db, "SimpleInfoPanelUnitDetail", 0);
        if let Some(i) = info {
            tree.frames[i].all_points = false;
            tree.frames[i].width = Some(0.188125);
            tree.frames[i].height = Some(0.114063);
            tree.frames[i].points = vec![Point { point: FramePoint::BottomLeft, relative: None, relative_point: FramePoint::BottomLeft, x: 0.309375, y: 0.0 }];
            tree.frames[i].visible = false;
            // Parts shown only for heroes, timed units, and structures at work.
            for name in ["SimpleBuildingActionLabel", "SimpleBuildTimeIndicator", "SimpleHeroLevelBar", "SimpleProgressIndicator"] {
                if let Some(f) = tree.get(name) {
                    tree.frames[f].visible = false;
                }
            }
            for (template, scope) in INFO_ICONS {
                tree.create_scoped(db, template, i, scope);
            }
            // The building and cargo details share the unit detail's place.
            if let Some(c) = tree.create_scoped(db, "SimpleInfoPanelCargoDetail", 0, "C") {
                let f = &mut tree.frames[c];
                f.all_points = false;
                f.width = Some(0.188125);
                f.height = Some(0.114063);
                f.points = vec![Point { point: FramePoint::BottomLeft, relative: None, relative_point: FramePoint::BottomLeft, x: 0.309375, y: 0.0 }];
                f.visible = false;
            }
            building = tree.create_scoped(db, "SimpleInfoPanelBuildingDetail", 0, "B");
            if let Some(b) = building {
                let f = &mut tree.frames[b];
                f.all_points = false;
                f.width = Some(0.188125);
                f.height = Some(0.114063);
                f.points = vec![Point { point: FramePoint::BottomLeft, relative: None, relative_point: FramePoint::BottomLeft, x: 0.309375, y: 0.0 }];
                f.visible = false;
            }
            tree.bind_points();
            // Each icon is 0.094062 wide; the first column starts 0.03125 below the top,
            // and the second weapon and the hero attributes sit to the right of the first.
            for (template, scope) in INFO_ICONS {
                let name = format!("{template}{scope}");
                tree.clear_points(&name);
                tree.set_size(&name, Some(0.094062), None);
                if let Some(f) = tree.get(&name) {
                    tree.frames[f].visible = false;
                }
            }
            let detail = "SimpleInfoPanelUnitDetail";
            let damage0 = "SimpleInfoPanelIconDamageDamage0";
            tree.set_point(damage0, FramePoint::TopLeft, detail, FramePoint::TopLeft, 0.0, -0.03125);
            tree.set_point("SimpleInfoPanelIconDamageDamage1", FramePoint::TopLeft, damage0, FramePoint::TopRight, 0.0, 0.0);
            tree.set_point("SimpleInfoPanelIconGoldGold", FramePoint::TopLeft, detail, FramePoint::TopLeft, 0.0, -0.03125);
            tree.set_point("SimpleInfoPanelIconHeroHero", FramePoint::TopLeft, damage0, FramePoint::TopRight, 0.0, 0.0);
            tree.layout();
        }
        Hud {
            portrait_stats: None,
            info,
            minimap_area: None,
            minimap_dots: Vec::new(),
            minimap_alt: Vec::new(),
            alt_icon: None,
            minimap_fog: false,
            minimap_blighted: false,
            minimap_icons: Vec::new(),
            minimap_icons_shown: Vec::new(),
            minimap_camps: Vec::new(),
            minimap_border: [60.0 / 255.0, 60.0 / 255.0, 70.0 / 255.0, 1.0],
            minimap_terrain: true,
            ally_mode: 0,
            show_creeps: true,
            filter_buttons: (true, true),
            formation: true,
            now: 0.0,
            unit_messages: Vec::new(),
            chat_messages: Vec::new(),
            unit_message_offset: (0.0, 0.0),
            error_message: None,
            target_prompt: false,
            targeting: false,
            dialog: None,
            boards: None,
            team_board_minimized: false,
            menu: None,
            quests: None,
            cargo: None,
            log_view: None,
            log: Vec::new(),
            logged_scene: None,
            quest_indicator: None,
            save_panel: None,
            chat: None,
            chat_to: 0,
            chat_prompt: String::new(),
            inventory: Vec::new(),
            inventory_tips: Vec::new(),
            inventory_cooldowns: Vec::new(),
            hover_tips: Vec::new(),
            tooltips: true,
            info_tips: Vec::new(),
            hero_tips: Vec::new(),
            level_bar: None,
            buff_icons: None,
            building,
            production: None,
            group: Vec::new(),
            pointer: (-1.0, -1.0),
            cine: None,
            heroes: Vec::new(),
            hero_flash: Default::default(),
            idle_workers: Vec::new(),
            paused: false,
            idle_pick: (f32::MIN, 0),
            cinematic: false,
            fades: (1.0, 0.0),
            filter: None,
            camera_box: Vec::new(),
            card: Vec::new(),
            page: crate::card::Page::Main,
            replay: None,
            net_dialog: None,
            alliance: None,
            chat_dialog: None,
            chat_history: Vec::new(),
            squelched: Vec::new(),
            hide_signals: false,
            gold: tree.get("ResourceBarGoldText"),
            lumber: tree.get("ResourceBarLumberText"),
            supply: tree.get("ResourceBarSupplyText"),
            upkeep: tree.get("ResourceBarUpkeepText"),
            tree,
        }
    }

    /// Shows the selected unit's name, class, and portrait statistics.
    /// CInventoryBar's buttons: 0.03125 squares in two columns 0.04 apart and three rows
    /// 0.038125 apart, from 0.01875 right of and 0.0021875 below CInfoBar's top right.
    pub fn inventory_rect(&self, slot: usize) -> w3_ui::Rect {
        let root = self.tree.frames[0].rect.unwrap_or_default();
        let (ix, iy) = (root.x0 + 0.309375 + 0.188125, root.y0 + 0.114063);
        let x0 = ix + 0.01875 + (slot & 1) as f32 * 0.04;
        let y1 = iy - 0.0021875 - (slot >> 1) as f32 * 0.038125;
        w3_ui::Rect { x0, x1: x0 + 0.03125, y0: y1 - 0.03125, y1 }
    }

    /// The inventory slot under an interface point.
    pub fn inventory_at(&self, x: f32, y: f32) -> Option<usize> {
        (0..self.inventory.len()).find(|&s| self.inventory_rect(s).contains(x, y))
    }

    /// The info panel's icons for a unit (CInfoPanelUnitDetail::vf27 and the
    /// CInfoPanelIcon* updates): which show, where they sit, and their texts.
    fn show_info_icons(&mut self, db: &FrameDb, world: &World, h: u32) {
        let s = world.info_stats(h);
        let tooltips = self.tooltips;
        let t = &mut self.tree;
        let frame = |kind: &str| format!("SimpleInfoPanelIcon{kind}{kind}");
        for (template, scope) in INFO_ICONS {
            t.set_visible(&format!("{template}{scope}"), false);
        }
        // A bonus follows the value in green with a plus, or red (" |CFF00FF00+%d|R").
        let bonus = |v: f32| {
            if v == 0.0 {
                String::new()
            } else {
                let color = if v > 0.0 { " |CFF00FF00+" } else { " |CFFFF0000" };
                if v == v.trunc() { format!("{color}{}|R", v as i32) } else { format!("{color}{v:.1}|R") }
            }
        };
        let capitalized = |s: &str| {
            let mut c = s.chars();
            c.next().map(|f| f.to_ascii_uppercase().to_string() + c.as_str()).unwrap_or_default()
        };
        // The neutral variant of an icon when no research applies, if the data has its
        // file (there is no neutral hero armor).
        let icon = |key: String, upgrade: Option<i32>| {
            let neutral = format!("{key}Neutral");
            if upgrade.is_none() && db.skin_value(&neutral).is_some_and(|f| world.chain.contains(&f)) { neutral } else { key }
        };
        let detail = "SimpleInfoPanelUnitDetail";
        let damage0 = "SimpleInfoPanelIconDamageDamage0";
        let armor = frame("Armor");
        let mut tips = Self::info_icon_tips(db, world, &s, &bonus);
        if let Some(hero) = &s.hero {
            tips.push(("SimpleInfoPanelIconHeroHero".to_string(), Self::hero_attribute_tip(db, world, &hero.primary, tooltips)));
        }
        let weapons = s.attack.is_some() as u32;
        if let Some(a) = &s.attack {
            t.set_visible(damage0, true);
            // Neutral icons when no research improves it.
            t.set_model("InfoPanelIconBackdropDamage0", &icon(format!("InfoPanelIconDamage{}", capitalized(&a.attack_type)), a.upgrade));
            t.set_text("InfoPanelIconValueDamage0", &format!("{} - {}{}", a.min.max(0), a.max.max(0), bonus(a.bonus as f32)));
            t.set_text("InfoPanelIconLevelDamage0", &a.upgrade.map(|l| l.to_string()).unwrap_or_default());
        }
        // Armor: below gold (mines), below the attack, or first.
        if s.gold.is_some() {
            t.set_point(&armor, FramePoint::TopLeft, &frame("Gold"), FramePoint::BottomLeft, 0.0, 0.0);
        } else if weapons == 0 {
            t.set_point(&armor, FramePoint::TopLeft, detail, FramePoint::TopLeft, 0.0, -0.03125);
        } else {
            t.set_point(&armor, FramePoint::TopLeft, damage0, FramePoint::BottomLeft, 0.0, 0.0);
        }
        t.set_visible(&armor, true);
        let ar = &s.armor;
        t.set_model("InfoPanelIconBackdropArmor", &icon(format!("InfoPanelIconArmor{}", capitalized(&ar.def_type)), ar.upgrade));
        let value = if ar.invulnerable { world.localized("INVULNERABLE") } else { format!("{}{}", ar.armor, bonus(ar.bonus)) };
        t.set_text("InfoPanelIconValueArmor", &value);
        t.set_text("InfoPanelIconLevelArmor", &ar.upgrade.map(|l| l.to_string()).unwrap_or_default());
        // Gold left in a mine.
        if let Some(g) = s.gold {
            t.set_visible(&frame("Gold"), true);
            t.set_text("InfoPanelIconValueGold", &g.to_string());
            t.set_text("InfoPanelIconLevelGold", "");
        }
        // A spellcaster's rank, beside the attack (or the armor).
        if let Some(rank) = &s.rank {
            let beside = if weapons == 1 { damage0.to_string() } else { armor.clone() };
            t.set_point(&frame("Rank"), FramePoint::TopLeft, &beside, FramePoint::TopRight, 0.0, 0.0);
            t.set_visible(&frame("Rank"), true);
            t.set_model("InfoPanelIconBackdropRank", &rank.art);
            t.set_text("InfoPanelIconValueRank", &rank.name);
            t.set_text("InfoPanelIconLevelRank", &rank.level.to_string());
        }
        // A structure of another player who shares vision with the local one (or seen by
        // an observer) shows that player's resources where its food would be
        // (CInfoPanelUnitDetail::vf27, CInfoPanelIconAlly::vf27).
        let local = world.local_player as usize;
        let ally = world.unit(h).filter(|u| u.is_building && s.hero.is_none() && s.gold.is_none()).map(|u| u.owner as usize).filter(|&o| {
            let observer = world.players.get(local).is_some_and(|p| p.get_state(w3_game::types::player_state::OBSERVER) != 0);
            o != local && o < 12 && (observer || world.players.get(o).is_some_and(|p| p.alliances[local][5]))
        });
        if let Some(o) = ally {
            use w3_game::types::player_state as ps;
            let p = &world.players[o];
            let beside = if weapons == 1 { damage0.to_string() } else { armor.clone() };
            t.set_point(&frame("Ally"), FramePoint::TopLeft, &beside, FramePoint::TopRight, 0.0, 0.0);
            t.set_visible(&frame("Ally"), true);
            t.set_text("InfoPanelIconAllyTitleAlly", &world.player_name(o as u8));
            let (gold, lumber) = (p.get_state(ps::RESOURCE_GOLD), p.get_state(ps::RESOURCE_LUMBER));
            let (used, cap) = (p.get_state(ps::RESOURCE_FOOD_USED), p.get_state(ps::RESOURCE_FOOD_CAP));
            t.set_text("InfoPanelIconAllyGoldValueAlly", &gold.to_string());
            t.set_text("InfoPanelIconAllyWoodValueAlly", &lumber.to_string());
            t.set_text("InfoPanelIconAllyFoodValueAlly", &format!("{used}/{cap}"));
            let level = world.upkeep_level(o as u8);
            let upkeep = Self::upkeep_name(db, level.min(2));
            t.set_text("InfoPanelIconAllyUpkeepAlly", &upkeep);
            // Each value's tooltip: its name, the value, and the resource's text.
            let tip = |title: &str, middle: String, uber: &str| {
                let mut b = crate::card::Button::text_only(db.string(title), db.string(uber));
                b.requires = middle;
                b
            };
            tips.push(("InfoPanelIconAllyGoldIconAlly".to_string(), tip("COLON_GOLD", gold.to_string(), "RESOURCE_UBERTIP_GOLD")));
            tips.push(("InfoPanelIconAllyWoodIconAlly".to_string(), tip("COLON_LUMBER", lumber.to_string(), "RESOURCE_UBERTIP_LUMBER")));
            let food = format!("{} {used}\n{} {cap}", db.string("COLON_USED"), db.string("COLON_PROVIDED"));
            tips.push(("InfoPanelIconAllyFoodIconAlly".to_string(), tip("COLON_FOOD", food, "RESOURCE_UBERTIP_SUPPLY")));
            let rate = format!("{} {}%", db.string("COLON_GOLD_INCOME_RATE"), 100 - world.upkeep_tax(o as u8).0);
            let mut b = crate::card::Button::text_only(upkeep, db.string("RESOURCE_UBERTIP_UPKEEP"));
            b.requires = rate;
            tips.push(("InfoPanelIconAllyUpkeepAlly".to_string(), b));
        }
        // Food provided, beside the armor (or the attack when there is one).
        if s.hero.is_none() && s.food_made > 0 && ally.is_none() {
            let beside = if weapons == 1 { damage0.to_string() } else { armor.clone() };
            t.set_point(&frame("Food"), FramePoint::TopLeft, &beside, FramePoint::TopRight, 0.0, 0.0);
            t.set_visible(&frame("Food"), true);
            t.set_text("InfoPanelIconValueFood", &s.food_made.to_string());
            t.set_text("InfoPanelIconLevelFood", "");
        }
        // Hero attributes, with the primary attribute's icon.
        if let Some(hero) = &s.hero {
            t.set_visible(&frame("Hero"), true);
            // InfoPanelIconHeroIcon<primary attribute>.
            let primary = if hero.primary.is_empty() { "STR" } else { hero.primary.as_str() };
            t.set_model("InfoPanelIconHeroIconHero", &format!("InfoPanelIconHeroIcon{primary}"));
            for (k, name) in ["Strength", "Agility", "Intellect"].iter().enumerate() {
                let (base, b) = hero.attributes[k];
                t.set_text(&format!("InfoPanelIconHero{name}ValueHero"), &format!("{base}{}", bonus(b as f32)));
            }
        }
        t.layout();
        self.info_tips = tips;
    }

    /// The hero attributes icon's tooltip (CInfoPanelIconHero::vf27): each attribute,
    /// the primary one marked with the damage it adds, and under the Tooltips option what
    /// each point of each gives, in the tip's text; without the option the names alone,
    /// the primary one with a colon, as its middle text.
    fn hero_attribute_tip(db: &FrameDb, world: &World, primary: &str, ext: bool) -> crate::card::Button {
        let misc = |k: &str, d: f32| world.misc_f32(k, d);
        let mut primary_block = format!(" - {} \n", db.string("PRIMARY_ATTRIBUTE"));
        if ext {
            primary_block.push_str(&format!("{} \n", db.string("BONUS_DAMAGE").replacen("%d", &(misc("StrAttackBonus", 1.0) as i32).to_string(), 1)));
        }
        let mut text = String::new();
        let sections: [(&str, &str, &str, Vec<String>); 3] = [
            ("STR", "STRENGTH", "COLON_STRENGTH", vec![db.string("BONUS_HITPOINTS").replacen("%d", &(misc("StrHitPointBonus", 25.0) as i32).to_string(), 1), db.string("BONUS_HPREGEN")]),
            ("AGI", "AGILITY", "COLON_AGILITY", vec![db.string("BONUS_DEFENSE_FIXED").replacen("%d", &((1.0 / misc("AgiDefenseBonus", 0.3)) as i32).to_string(), 1), db.string("BONUS_ATTACK_SPEED")]),
            ("INT", "INTELLECT", "COLON_INTELLECT", vec![db.string("BONUS_MANA").replacen("%d", &(misc("IntManaBonus", 15.0) as i32).to_string(), 1), db.string("BONUS_MANAREGEN")]),
        ];
        for (code, name, colon, bonuses) in sections {
            let is_primary = primary.eq_ignore_ascii_case(code);
            text.push_str(&format!("{} \n", db.string(if ext || is_primary { colon } else { name })));
            if is_primary {
                text.push_str(&primary_block);
            }
            if ext {
                for b in bonuses {
                    text.push_str(&format!("{b} \n"));
                }
            }
        }
        let text = text.trim_end().to_string();
        let title = db.string("COLON_HERO_ATTRIBUTES");
        if ext {
            crate::card::Button::text_only(title, text)
        } else {
            let mut b = crate::card::Button::text_only(title, String::new());
            b.requires = text;
            b
        }
    }

    /// The tooltips of the unit detail's damage and armor icons
    /// (CInfoPanelIconDamage::vf27, CInfoPanelIconArmor::vf27): the value as the title;
    /// the type, range and speed or damage reduction and move speed, and the research
    /// that improves it; and the type's tip.
    fn info_icon_tips(db: &FrameDb, world: &World, s: &w3_game::info::InfoStats, bonus: &dyn Fn(f32) -> String) -> Vec<(String, crate::card::Button)> {
        use crate::card::Button;
        let mut out = Vec::new();
        let ip = |key: &str, d: f32| db.misc_ui.get_str("InfoPanel", key).and_then(|v| v.split("//").next().and_then(|v| v.trim().parse().ok())).unwrap_or(d);
        // A colored value when something adds to it (green) or takes from it (red).
        let tinted = |text: String, by: f32| if by == 0.0 { text } else { format!("{}{text}|R", if by > 0.0 { "|CFF00FF00" } else { "|CFFFF0000" }) };
        // The type's tip: <key>_V<version><C>, else <key>.
        let type_tip = |key: &str| {
            let versioned = format!("{key}_V{}C", world.version);
            let v = db.string(&versioned);
            if !v.is_empty() && v != versioned { v } else { db.string(key) }
        };
        let upgrade_line = |id: Option<w3_game::objects::Id>, level: Option<i32>| -> String {
            let (Some(id), Some(level)) = (id, level) else { return String::new() };
            let name = world.objects.upgrades.get(id).map(|o| crate::card::level_entry(&o.list("Name"), level.max(1) as u32)).unwrap_or_default();
            format!("\n{} {}", db.string("COLON_UPGRADE"), db.string("UPGRADE_TOOLTIP").replacen("%s", &name, 1).replacen("%d", &level.to_string(), 1))
        };
        if let Some(a) = &s.attack {
            let kind = match a.attack_type.as_str() {
                "pierce" => "PIERCE",
                "siege" => "SIEGE",
                "magic" => "MAGIC",
                "chaos" => "CHAOS",
                "hero" => "HERO",
                _ => "NORMAL",
            };
            let title = format!("{} {} - {}{}", db.string("COLON_DAMAGE"), a.min.max(0), a.max.max(0), bonus(a.bonus as f32));
            let range = if a.range as f32 > ip("MeleeRangeMax", 128.0) {
                format!("{}{}", a.range, if a.range_bonus == 0 { String::new() } else { format!("{}{}|R", if a.range_bonus > 0 { " |CFF00FF00+" } else { " |CFFFF0000" }, a.range_bonus) })
            } else {
                format!("{} ", db.string("MELEE"))
            };
            // Attack speed by cooldown: AttackVerySlow and slower are very slow, below
            // AttackFast very fast (FUN_6f3a9e60).
            let c = a.cooldown;
            let speed = if c >= ip("AttackVerySlow", 3.0) {
                "MOVESPEEDVERYSLOW"
            } else if c >= ip("AttackSlow", 2.0) {
                "MOVESPEEDSLOW"
            } else if c >= ip("AttackAverage", 1.5) {
                "MOVESPEEDAVERAGE"
            } else if c >= ip("AttackFast", 1.0) {
                "MOVESPEEDFAST"
            } else {
                "MOVESPEEDVERYFAST"
            };
            let mut middle = format!("{} \n{} {range}\n{} {}", db.string(&format!("DAMAGE_{kind}")), db.string("COLON_RANGE"), db.string("COLON_SPEED"), tinted(db.string(speed), a.speed_bonus));
            middle.push_str(&upgrade_line(a.upgrade_id, a.upgrade));
            let mut b = Button::text_only(title, type_tip(&format!("DAMAGETIP_{kind}")));
            b.requires = middle;
            out.push(("SimpleInfoPanelIconDamageDamage0".to_string(), b));
        }
        let ar = &s.armor;
        let kind = match ar.def_type.as_str() {
            "small" => "SMALL",
            "medium" => "MEDIUM",
            "large" => "LARGE",
            "fort" => "FORT",
            "hero" => "HERO",
            "divine" => "DIVINE",
            "none" => "NONE",
            _ => "NORMAL",
        };
        let value = if ar.invulnerable { db.string("INVULNERABLE") } else { format!("{}{}", ar.armor, bonus(ar.bonus)) };
        let title = format!("{} {value}", db.string("COLON_ARMOR"));
        let mut middle = format!("{} \n", db.string(&format!("ARMOR_{kind}")));
        if !ar.invulnerable {
            let pct = (ar.reduction * 100.0).floor() as i32;
            middle.push_str(&format!("{} {}", db.string("COLON_DAMAGE_REDUCTION"), if ar.bonus == 0.0 { format!("{pct}%") } else { format!("{}{pct}|R%", if ar.bonus > 0.0 { "|CFF00FF00" } else { "|CFFFF0000" }) }));
        }
        if let Some((speed, by)) = ar.move_speed {
            // Movement by speed: SpeedVerySlow and below very slow, above SpeedFast
            // very fast (FUN_6f3aabc0).
            let word = if speed <= ip("SpeedVerySlow", 175.0) {
                "MOVESPEEDVERYSLOW"
            } else if speed <= ip("SpeedSlow", 220.0) {
                "MOVESPEEDSLOW"
            } else if speed <= ip("SpeedAverage", 280.0) {
                "MOVESPEEDAVERAGE"
            } else if speed <= ip("SpeedFast", 350.0) {
                "MOVESPEEDFAST"
            } else {
                "MOVESPEEDVERYFAST"
            };
            middle.push_str(&format!("\n{} {}", db.string("COLON_MOVE_SPEED"), tinted(db.string(word), if by.abs() < 0.001 { 0.0 } else { by })));
        }
        middle.push_str(&upgrade_line(ar.upgrade_id, ar.upgrade));
        let mut b = Button::text_only(title, type_tip(&format!("ARMORTIP_{kind}")));
        b.requires = middle;
        out.push(("SimpleInfoPanelIconArmorArmor".to_string(), b));
        // A spellcaster's rank (CInfoPanelIconRank::vf27): its name and training, and
        // its spells at that rank.
        if let Some(rank) = &s.rank {
            let mut middle = format!("{} {}", db.string("COLON_NAME"), rank.name);
            middle.push_str(&upgrade_line(rank.upgrade_id, rank.upgrade_id.map(|_| rank.level)));
            let mut uber = format!("{} \n{}", db.string("COLON_SPELLS"), rank.tip);
            if rank.level != 0 {
                uber.push_str(&format!("\n\n{} ", db.string("CASTER_UPGRADE_TIP")));
            }
            let mut b = Button::text_only(db.string("COLON_RANK"), uber);
            b.requires = middle;
            out.push(("SimpleInfoPanelIconRankRank".to_string(), b));
        }
        // A mine's gold (CInfoPanelIconGold::vf27) and the food a structure provides
        // (CInfoPanelIconFood::vf27).
        if let Some(g) = s.gold {
            let mut b = Button::text_only(db.string("COLON_GOLD"), db.string("RESOURCE_UBERTIP_GOLD"));
            b.requires = g.to_string();
            out.push(("SimpleInfoPanelIconGoldGold".to_string(), b));
        }
        if s.hero.is_none() && s.food_made > 0 {
            out.push(("SimpleInfoPanelIconFoodFood".to_string(), Button::text_only(db.string("COLON_FOOD"), format!("{}{}", db.string("COLON_PROVIDED"), s.food_made))));
        }
        out
    }

    fn info_visible(&self) -> bool {
        [self.info, self.building].iter().flatten().any(|&i| self.tree.frames[i].visible)
    }

    /// CInfoPanelGroup for a selection of several units: up to twelve, subgroup by
    /// subgroup (units of one type), the first subgroup active.  It replaces the unit
    /// and building details.
    pub fn show_group(&mut self, world: &World, selection: &[u32]) {
        self.group.clear();
        if selection.len() < 2 {
            return;
        }
        let mut types: Vec<w3_game::objects::Id> = Vec::new();
        for &h in selection {
            if let Some(u) = world.unit(h) {
                if !types.contains(&u.type_id) {
                    types.push(u.type_id);
                }
            }
        }
        for (k, t) in types.iter().enumerate() {
            for &h in selection {
                let Some(u) = world.unit(h).filter(|u| u.type_id == *t) else { continue };
                if self.group.len() >= 12 {
                    break;
                }
                let icon = world.objects.units.get(u.type_id).map(|o| o.str("Art")).unwrap_or_default();
                let mana = (u.max_mana > 0.0).then(|| u.mana / u.max_mana);
                let argb = |c: u32| [((c >> 16) & 0xff) as f32 / 255.0, ((c >> 8) & 0xff) as f32 / 255.0, (c & 0xff) as f32 / 255.0, 1.0];
                let color = if u.extra.illusion.is_some() {
                    argb(0xff1460ff)
                } else if u.extra.animated {
                    argb(0xffa04040)
                } else if world.is_ethereal(h) {
                    argb(0xffc0ff80)
                } else if u.buffs.iter().any(|b| world.unit(b.caster).is_some_and(|c| !w3_game::natives::players::is_ally(world, u.owner, c.owner))) {
                    argb(0xffff0000)
                } else {
                    [1.0; 4]
                };
                self.group.push(GroupSlot { unit: h, icon, life: u.life / u.max_life.max(1.0), mana, active: k == 0, color });
            }
        }
        for f in [self.info, self.building].into_iter().flatten() {
            self.tree.frames[f].visible = false;
        }
        self.production = None;
        self.buff_icons = None;
        self.level_bar = None;
    }

    /// Follows the game's cinematic state: CinematicPanel (spanning the window) with the
    /// transmission's speaker and text, and the cine filter's current look.
    pub fn sync_cinematic(&mut self, db: &FrameDb, world: &World, aspect: f32) {
        let c = &world.cinematic;
        // Each transmission goes to the message log once.
        if let Some(s) = &c.scene {
            let key = (s.ends, s.speaker.clone());
            if self.logged_scene.as_ref() != Some(&key) {
                self.log.push(crate::msglog::transmission_line(&s.speaker, &s.text));
                self.logged_scene = Some(key);
            }
        }
        self.cinematic = !c.interface;
        let t = if c.interface_fade > 0.0 { ((world.time - c.interface_changed) / c.interface_fade).clamp(0.0, 1.0) } else { 1.0 };
        self.fades = if c.interface { (t, 1.0 - t) } else { (1.0 - t, t) };
        self.filter = c.filter.shown.and_then(|shown| {
            let f = &c.filter;
            let t = if f.duration > 0.0 { ((world.time - shown) / f.duration).clamp(0.0, 1.0) } else { 1.0 };
            let lerp = |a: f32, b: f32| a + (b - a) * t;
            let col: Vec<f32> = (0..4).map(|i| lerp(f.start_color[i] as f32, f.end_color[i] as f32) / 255.0).collect();
            let uv: Vec<f32> = (0..4).map(|i| lerp(f.start_uv[i], f.end_uv[i])).collect();
            // BLEND_MODE_ADDITIVE is 3; BLEND_MODE_BLEND (2) blends by alpha.
            (!f.texture.is_empty()).then(|| (f.texture.clone(), [uv[0], uv[1], uv[2], uv[3]], [col[0], col[1], col[2], col[3]], f.blend == 3))
        });
        if !self.cinematic && self.fades.1 <= 0.0 {
            return;
        }
        if self.cine.is_none() {
            let mut t = FrameTree::new(aspect);
            if let Some(p) = t.create(db, "CinematicPanel", 0) {
                // The panel and its borders span the whole window, not only the 4:3 area.
                let extra = (t.width - w3_ui::UI_WIDTH) / 2.0;
                t.frames[p].all_points = false;
                t.frames[p].points = vec![
                    Point { point: FramePoint::TopLeft, relative: None, relative_point: FramePoint::TopLeft, x: -extra, y: 0.0 },
                    Point { point: FramePoint::BottomRight, relative: None, relative_point: FramePoint::BottomRight, x: extra, y: 0.0 },
                ];
            }
            t.bind_points();
            let full = t.width;
            t.set_size("CinematicBottomBorder", Some(full), None);
            t.set_size("CinematicTopBorder", Some(full), None);
            // The portrait sits in the 4:3 area's corner.
            let extra = (t.width - w3_ui::UI_WIDTH) / 2.0;
            for name in ["CinematicPortraitBackground", "CinematicPortrait"] {
                t.set_point(name, FramePoint::BottomLeft, "CinematicPanel", FramePoint::BottomLeft, 0.014 + extra, 0.014);
            }
            t.set_point("CinematicPortraitCover", FramePoint::BottomLeft, "CinematicPanel", FramePoint::BottomLeft, extra, 0.0);
            t.layout();
            self.cine = Some(t);
        }
        let t = self.cine.as_mut().unwrap();
        t.set_visible("CinematicScenePanel", c.scene.is_some());
        if let Some(s) = &c.scene {
            t.set_text("CinematicSpeakerText", &s.speaker);
            t.set_text("CinematicDialogueText", &s.text);
        }
        t.layout();
    }

    /// The cine filter over the whole window.
    fn filter_draws(&self, out: &mut Vec<Draw>) {
        if let Some((texture, uv, color, add)) = &self.filter {
            let rect = w3_ui::Rect { x0: 0.0, y0: 0.0, x1: self.tree.width, y1: w3_ui::UI_HEIGHT };
            let blend = if *add { w3_ui::Blend::Add } else { w3_ui::Blend::Alpha };
            out.push(Draw::Image { texture: texture.clone(), rect, uv: *uv, color: *color, blend, rotated: false });
        }
    }

    /// Where the transmission's portrait model is drawn.
    pub fn cine_portrait_rect(&self) -> Option<w3_ui::Rect> {
        let t = self.cine.as_ref()?;
        let f = t.get("CinematicPortrait")?;
        t.frames[f].rect
    }

    /// CUnitTip over what the pointer is on (CPreselectUI, FUN_6f3d16c0): a ToolTip box
    /// (corners 0.01, insets 0.0019) of the given lines, the first 0.008 in and 0.006
    /// down, 0.0125 apart (measured from the original), centered on and resting on the
    /// health bar's top at `x`, `bottom`.
    pub fn unit_tip_draws(&self, db: &FrameDb, lines: &[(String, [f32; 4])], x: f32, bottom: f32, out: &mut Vec<Draw>) {
        use w3_ui::fdf::{Arg, Def, Prop};
        if lines.is_empty() {
            return;
        }
        let font = db.decorate("MasterFont");
        let size = db.misc_ui.get_str("FontHeights", "UnitTipPlayerName").and_then(|v| v.trim().parse().ok()).unwrap_or(0.011f32);
        let widest = lines.iter().map(|(t, _)| db.text_width(&font, size, t)).fold(0.0, f32::max);
        let (w, h) = (widest + 0.016, 0.006 + lines.len() as f32 * 0.0125 + 0.004);
        let r = w3_ui::Rect { x0: x - w / 2.0, x1: x + w / 2.0, y0: bottom, y1: bottom + h };
        let prop = |key: &str, args: Vec<Arg>| Prop { key: key.into(), args };
        let s = |v: &str| Arg::Str(v.into());
        let def = Def {
            block: "Frame".into(),
            kind: "BACKDROP".into(),
            name: String::new(),
            inherits: None,
            with_children: false,
            props: vec![
                prop("BackdropBackground", vec![s("ToolTipBackground")]),
                prop("BackdropCornerFlags", vec![s("UL|UR|BL|BR|T|L|B|R")]),
                prop("BackdropCornerSize", vec![Arg::Num(0.01)]),
                prop("BackdropBackgroundInsets", vec![Arg::Num(0.0019); 4]),
                prop("BackdropEdgeFile", vec![s("ToolTipBorder")]),
            ],
            children: vec![],
        };
        self.tree.backdrop(db, &def, r, out);
        let mut y = r.y1 - 0.006;
        for (t, color) in lines {
            out.push(Draw::Text { font: font.clone(), size, rect: w3_ui::Rect { x0: r.x0 + 0.008, x1: r.x1, y0: y - size * 1.2, y1: y }, text: t.clone(), color: *color, justify_h: w3_ui::Justify::Start, justify_v: w3_ui::Justify::Start, shadow: None });
            y -= 0.0125;
        }
    }

    /// CUberToolTipWar3 for a command button: its BOTTOMRIGHT 0.1625 above the
    /// interface's, a ToolTipBorder/ToolTipBackground box (corners 0.01, insets 0.0019),
    /// the name 0.005 in, the cost row (0.009375 icons with gold values, 0.004 below),
    /// and the text wrapped at 0.21 another 0.005 below; the box is the name's width
    /// (at least 0.21) plus 0.01 wide and as tall as its parts plus 0.01.
    fn tooltip_draws(&self, db: &FrameDb, b: &crate::card::Button, out: &mut Vec<Draw>) {
        use w3_ui::fdf::{Arg, Def, Prop};
        let root = self.tree.frames[0].rect.unwrap_or_default();
        let font = db.decorate("MasterFont");
        let height = |key: &str| db.misc_ui.get_str("FontHeights", key).and_then(|v| v.trim().parse().ok()).unwrap_or(0.011f32);
        let (name_h, desc_h, cost_h) = (height("ToolTipName"), height("ToolTipDesc"), height("ToolTipCost"));
        let name_w = db.text_width(&font, name_h, &b.tip);
        let lines = if b.ubertip.is_empty() { Vec::new() } else { db.wrap(&font, desc_h, &b.ubertip, 0.21) };
        // Unmet requirements come between the costs and the text.
        let req_lines = if b.requires.is_empty() { Vec::new() } else { db.wrap(&font, desc_h, &b.requires, 0.21) };
        let line_h = desc_h;
        let costs: Vec<(usize, i32)> = b.costs.iter().copied().enumerate().filter(|&(_, c)| c != 0).collect();
        let cost_row = if costs.is_empty() { 0.0 } else { 0.013375 };
        let uber_h = if lines.is_empty() { 0.0 } else { lines.len() as f32 * line_h + 0.005 };
        let req_h = if req_lines.is_empty() { 0.0 } else { req_lines.len() as f32 * line_h + 0.005 };
        let w = name_w.max(0.21) + 0.01;
        let h = name_h + cost_row + req_h + uber_h + 0.01;
        let r = w3_ui::Rect { x0: root.x1 - w, x1: root.x1, y0: root.y0 + 0.1625, y1: root.y0 + 0.1625 + h };
        let prop = |key: &str, args: Vec<Arg>| Prop { key: key.into(), args };
        let s = |v: &str| Arg::Str(v.into());
        let def = Def {
            block: "Frame".into(),
            kind: "BACKDROP".into(),
            name: String::new(),
            inherits: None,
            with_children: false,
            props: vec![
                prop("BackdropBackground", vec![s("ToolTipBackground")]),
                prop("BackdropCornerFlags", vec![s("UL|UR|BL|BR|T|L|B|R")]),
                prop("BackdropCornerSize", vec![Arg::Num(0.01)]),
                prop("BackdropBackgroundInsets", vec![Arg::Num(0.0019); 4]),
                prop("BackdropEdgeFile", vec![s("ToolTipBorder")]),
            ],
            children: vec![],
        };
        self.tree.backdrop(db, &def, r, out);
        let text = |out: &mut Vec<Draw>, t: &str, x0: f32, y1: f32, size: f32, w: f32, color: [f32; 4]| {
            out.push(Draw::Text { font: font.clone(), size, rect: w3_ui::Rect { x0, x1: x0 + w, y0: y1 - size * 1.2, y1 }, text: t.to_string(), color, justify_h: w3_ui::Justify::Start, justify_v: w3_ui::Justify::Start, shadow: None });
        };
        let (x0, mut y) = (r.x0 + 0.005, r.y1 - 0.005);
        text(out, &b.tip, x0, y, name_h, name_w + 0.01, [1.0; 4]);
        y -= name_h;
        if !costs.is_empty() {
            // ToolTipManaIcon, ToolTipGoldIcon, ToolTipLumberIcon, ToolTipSupplyIcon.
            let keys = ["ToolTipManaIcon", "ToolTipGoldIcon", "ToolTipLumberIcon", "ToolTipSupplyIcon"];
            let (mut x, top) = (x0, y - 0.004);
            let gold = [0xfe as f32 / 255.0, 0xd3 as f32 / 255.0, 0x12 as f32 / 255.0, 1.0];
            for (k, c) in costs {
                let icon = w3_ui::Rect { x0: x, x1: x + 0.009375, y0: top - 0.009375, y1: top };
                out.push(Draw::Image { texture: db.decorate(keys[k]), rect: icon, uv: [0.0, 0.0, 1.0, 1.0], color: [1.0; 4], blend: w3_ui::Blend::Alpha, rotated: false });
                let v = c.to_string();
                let tw = db.text_width(&font, cost_h, &v);
                let tx = icon.x1 + 0.003125;
                out.push(Draw::Text { font: font.clone(), size: cost_h, rect: w3_ui::Rect { x0: tx, x1: tx + tw + 0.005, y0: icon.y0 - 0.003, y1: icon.y1 + 0.003 }, text: v, color: gold, justify_h: w3_ui::Justify::Start, justify_v: w3_ui::Justify::Middle, shadow: None });
                x = tx + tw + 0.00625;
            }
            y -= cost_row;
        }
        if !req_lines.is_empty() {
            y -= 0.005;
            // The tooltip's middle text is gray (145, 144, 144, measured from the original).
            text(out, &req_lines.join("\n"), x0, y, desc_h, 0.21 + 0.005, [145.0 / 255.0, 144.0 / 255.0, 144.0 / 255.0, 1.0]);
            y -= req_lines.len() as f32 * line_h;
        }
        if !lines.is_empty() {
            y -= 0.005;
            text(out, &lines.join("\n"), x0, y, desc_h, 0.21 + 0.005, [1.0; 4]);
        }
    }

    /// Follows the local player's idle workers (units classified peon with nothing to
    /// do) for the idle worker button.
    pub fn sync_idle_workers(&mut self, world: &World) {
        self.idle_workers = world
            .unit_order
            .iter()
            .copied()
            .filter(|&h| {
                world.unit(h).is_some_and(|u| {
                    u.owner == world.local_player
                        && !u.dead
                        && !u.hidden
                        && u.constructing.is_none()
                        && matches!(u.action, w3_game::types::Action::Idle)
                        && u.queue.is_empty()
                        && world.objects.units.get(u.type_id).is_some_and(|o| o.str("type").to_ascii_lowercase().split(',').any(|c| c.trim() == "peon"))
                })
            })
            .collect();
    }

    /// The idle worker button (CPeonBar, 0.038 square): its lower left 0.00375 right of
    /// and 0.17 above the interface's; the icon shows 0.002 further right and 0.001
    /// higher (measured from the original).
    pub fn idle_worker_rect(&self) -> w3_ui::Rect {
        let root = self.tree.frames[0].rect.unwrap_or_default();
        let (x0, y0) = (root.x0 + 0.00575, root.y0 + 0.171);
        w3_ui::Rect { x0, x1: x0 + 0.038, y0, y1: y0 + 0.038 }
    }

    /// The idle worker its button chooses at `now`: the next after the last one chosen
    /// within a second, else the first (CPeonBar::vf3_6f3bd9c0).
    pub fn pick_idle_worker(&mut self, x: f32, y: f32, now: f32) -> Option<u32> {
        if !self.idle_worker_rect().contains(x, y) {
            return None;
        }
        self.next_idle_worker(now)
    }

    /// The idle worker the button (or its F8 hotkey) chooses at `now`.
    pub fn next_idle_worker(&mut self, now: f32) -> Option<u32> {
        if self.idle_workers.is_empty() {
            return None;
        }
        let k = if now < self.idle_pick.0 + 1.0 { (self.idle_pick.1 + 1) % self.idle_workers.len() } else { 0 };
        self.idle_pick = (now, k);
        Some(self.idle_workers[k])
    }

    /// A command button's number (CCommandButton FUN_6f3adba0): CommandButtonNumberOverlay
    /// 0.015625 square at its lower right, the number on it in MasterFont
    /// [FontHeights] CommandButtonNumber, 0.0006 low.
    fn number_overlay(db: &FrameDb, r: w3_ui::Rect, n: i32, out: &mut Vec<Draw>) {
        let o = w3_ui::Rect { x0: r.x1 - 0.015625, x1: r.x1, y0: r.y0, y1: r.y0 + 0.015625 };
        out.push(Draw::Image { texture: db.decorate("CommandButtonNumberOverlay"), rect: o, uv: [0.0, 0.0, 1.0, 1.0], color: [1.0; 4], blend: w3_ui::Blend::Alpha, rotated: false });
        let size = db.misc_ui.get_str("FontHeights", "CommandButtonNumber").and_then(|v| v.split("//").next().and_then(|v| v.trim().parse().ok())).unwrap_or(0.009);
        let t = w3_ui::Rect { y0: o.y0 - 0.0006, y1: o.y1 - 0.0006, ..o };
        out.push(Draw::Text { font: db.decorate("MasterFont"), size, rect: t, text: n.to_string(), color: [1.0; 4], justify_h: w3_ui::Justify::Middle, justify_v: w3_ui::Justify::Middle, shadow: None });
    }

    /// Follows the local player's heroes for the hero bar, in the order they came, then
    /// those of players who give it advanced control of their units (CHeroBar keeps its
    /// own before others, FUN_6f3b92d0; measured: UndeadX01 shows Arthas, then the
    /// heroes of the two allies giving advanced control; a player giving shared control
    /// alone shows none), and their buttons' tooltips (FUN_6f3c6e90): the hero's name
    /// with its key, F1 to F5, and its level, or "Dead" and "Revive at Altar".
    pub fn sync_heroes(&mut self, db: &FrameDb, world: &World) {
        let replay = self.replay.is_some();
        const ALLIANCE_SHARED_ADVANCED_CONTROL: usize = 7;
        let me = world.local_player as usize;
        let advanced = |owner: u8| owner as usize != me && world.players.get(owner as usize).is_some_and(|p| p.alliances.get(me).is_some_and(|a| a[ALLIANCE_SHARED_ADVANCED_CONTROL]));
        let order: Vec<u32> = {
            let shown = |u: &w3_game::types::Unit| u.is_hero && !u.hidden;
            let own = world.unit_order.iter().copied().filter(|&h| world.unit(h).is_some_and(|u| shown(u) && u.owner as usize == me));
            let allied = world.unit_order.iter().copied().filter(|&h| world.unit(h).is_some_and(|u| shown(u) && advanced(u.owner)));
            own.chain(allied).take(7).collect()
        };
        self.hero_tips = order
            .iter()
            .filter_map(|&h| world.unit(h))
            .enumerate()
            .map(|(k, u)| {
                let name = u.proper_name.clone();
                let key = |k: usize| format!("{name} (|cfffed312F{}|r)", k + 1);
                let level = format!("{} {}", db.string("LEVEL"), u.level);
                match (u.dead, replay) {
                    (true, false) => crate::card::Button::text_only(format!("{name} (|cfffed312{}|r)", db.string("DEAD")), db.string("REVIVE_AT_ALTAR")),
                    (true, true) => crate::card::Button::text_only(name.clone(), level),
                    (false, _) if k <= 4 => crate::card::Button::text_only(key(k), level),
                    (false, _) => crate::card::Button::text_only(name.clone(), level),
                }
            })
            .collect();
        self.heroes = order
            .iter()
            .filter_map(|&h| world.unit(h).map(|u| (h, u)))
            .map(|(h, u)| {
                let icon = world.objects.units.get(u.type_id).map(|o| o.str("Art")).unwrap_or_default();
                let icon = if u.dead { crate::card::disabled_icon(&icon) } else { icon };
                (h, icon, if u.dead { 0.0 } else { u.life / u.max_life.max(1.0) }, (u.max_mana > 0.0).then(|| u.mana / u.max_mana), u.skill_points > 0)
            })
            .collect();
    }

    /// The hero bar's button `k`: CHeroBar's 7-row grid (0.038 x 0.35825) from 0.00375
    /// right of and 0.02625 below the interface's top left; the icons show 0.002
    /// further right (measured from the original).
    pub fn hero_button_rect(&self, k: usize) -> w3_ui::Rect {
        let root = self.tree.frames[0].rect.unwrap_or_default();
        let cell = 0.35825 / 7.0;
        let (x0, y1) = (root.x0 + 0.00575, root.y1 - 0.02625 - cell * k as f32);
        w3_ui::Rect { x0, x1: x0 + 0.038, y0: y1 - 0.038, y1 }
    }

    pub fn hero_at(&self, x: f32, y: f32) -> Option<u32> {
        (0..self.heroes.len()).find(|&k| self.hero_button_rect(k).contains(x, y)).map(|k| self.heroes[k].0)
    }

    /// Each hero's icon with its life and mana bars (CStatBar's small kinds, 0.003125
    /// tall, 0.00125 apart).
    fn hero_bar_draws(&self, db: &FrameDb, out: &mut Vec<Draw>) {
        let image = |texture: String, rect: w3_ui::Rect, color: [f32; 4]| Draw::Image { texture, rect, uv: [0.0, 0.0, 1.0, 1.0], color, blend: w3_ui::Blend::Alpha, rotated: false };
        for (k, (hero, icon, life, mana, _)) in self.heroes.iter().enumerate() {
            let r = self.hero_button_rect(k);
            // An attacked hero's button turns 0xffff4444 and back every half second for
            // three seconds (FUN_6f3b08c0, FUN_6f3be330).
            let flashing = self.hero_flash.get(hero).is_some_and(|&t0| {
                let e = self.now - t0;
                (0.5..3.0).contains(&e) && e % 1.0 >= 0.5
            });
            let tint = if flashing { [1.0, 0x44 as f32 / 255.0, 0x44 as f32 / 255.0, 1.0] } else { [1.0; 4] };
            let icon = if self.paused { crate::card::disabled_icon(icon) } else { icon.clone() };
            out.push(image(icon, r, tint));
            let mut top = r.y0 - 0.00125;
            let mut bar = |frac: f32, key: &str, color: [f32; 4], out: &mut Vec<Draw>| {
                let b = w3_ui::Rect { x0: r.x0, x1: r.x1, y0: top - 0.003125, y1: top };
                out.push(image(String::new(), w3_ui::Rect { x0: b.x0 - 0.001, x1: b.x1 + 0.001, y0: b.y0 - 0.001, y1: b.y1 + 0.001 }, [0.0, 0.0, 0.0, 1.0]));
                out.push(image(db.decorate(key), w3_ui::Rect { x1: b.x0 + (b.x1 - b.x0) * frac.clamp(0.0, 1.0), ..b }, color));
                top = b.y0 - 0.00125;
            };
            bar(*life, "SimpleHpBarConsoleSmall", life_color(*life), out);
            if let Some(m) = mana {
                bar(*m, "SimpleManaBarConsoleSmall", [0.0, 0.0, 1.0, 1.0], out);
            }
        }
    }

    /// CGameUI's minimap buttons (FUN_6f36d890): 0.023125 x 0.020625 with their top left
    /// at (0.154375, 0.14375 / 0.121875 / 0.1 / 0.0775) and the formation button at
    /// (0.155, 0.05) from the interface's lower left: signal, terrain, ally colors,
    /// creeps, formation.
    pub fn minimap_button_rect(&self, k: usize) -> w3_ui::Rect {
        let root = self.tree.frames[0].rect.unwrap_or_default();
        let (x, y) = [(0.154375, 0.14375), (0.154375, 0.121875), (0.154375, 0.1), (0.154375, 0.0775), (0.155, 0.05)][k.min(4)];
        let (x0, y1) = (root.x0 + x, root.y0 + y);
        w3_ui::Rect { x0, x1: x0 + 0.023125, y0: y1 - 0.020625, y1 }
    }

    pub fn minimap_button_at(&self, x: f32, y: f32) -> Option<usize> {
        (0..5).find(|&k| self.minimap_button_rect(k).contains(x, y))
    }

    /// Toggles a minimap button; the signal button (0) has no state.
    pub fn press_minimap_button(&mut self, k: usize) {
        match k {
            1 => self.minimap_terrain = !self.minimap_terrain,
            // The scene cycles the world's ally color mode (FUN_6f3704f0).
            2 => {}
            // The scene turns the world's creep camp filter (option 0x39).
            3 => {}
            4 => self.formation = !self.formation,
            _ => {}
        }
    }

    fn minimap_button_draws(&self, db: &FrameDb, out: &mut Vec<Draw>) {
        let keys = [
            "MiniMapSignalButtonEnabled",
            // Active while the terrain shows (as the original shows it at the start).
            if self.minimap_terrain { "MiniMapTerrainButtonActiveEnabled" } else { "MiniMapTerrainButtonInactiveEnabled" },
            // FUN_6f3731a0: Off for player colors, Inactive for the minimap's, Active for
            // everywhere.
            match (self.filter_buttons.0, self.ally_mode % 3) {
                (false, 0) => "MiniMapAllyButtonOffDisabled",
                (false, 1) => "MiniMapAllyButtonInactiveDisabled",
                (false, _) => "MiniMapAllyButtonActiveDisabled",
                (true, m) => ["MiniMapAllyButtonOffEnabled", "MiniMapAllyButtonInactiveEnabled", "MiniMapAllyButtonActiveEnabled"][m as usize],
            },
            match (self.filter_buttons.1, self.show_creeps) {
                (true, true) => "MiniMapCreepButtonActiveEnabled",
                (true, false) => "MiniMapCreepButtonInactiveEnabled",
                (false, true) => "MiniMapCreepButtonActiveDisabled",
                (false, false) => "MiniMapCreepButtonInactiveDisabled",
            },
            if self.formation { "MiniMapFormationOnButtonEnabled" } else { "MiniMapFormationOffButtonEnabled" },
        ];
        for (k, key) in keys.iter().enumerate() {
            out.push(Draw::Image { texture: db.decorate(key), rect: self.minimap_button_rect(k), uv: [0.0, 0.0, 1.0, 1.0], color: [1.0; 4], blend: w3_ui::Blend::Alpha, rotated: false });
        }
    }

    /// The center of group button `k`, in interface units.
    fn group_button_center(&self, k: usize) -> (f32, f32) {
        let root = self.tree.frames[0].rect.unwrap_or_default();
        let (cx, y0) = (root.x0 + 0.309375 + 0.188125 / 2.0 - 0.001563, root.y0);
        let (gw, gh) = (0.185937, 0.101562);
        let (col, row) = ((k % 6) as f32, (k / 6) as f32);
        (cx - gw / 2.0 + gw / 6.0 * (col + 0.5), y0 + gh - gh / 2.0 * (row + 0.5))
    }

    /// The unit whose group button is under an interface point.
    pub fn group_at(&self, x: f32, y: f32) -> Option<u32> {
        (0..self.group.len()).find_map(|k| {
            let (bx, by) = self.group_button_center(k);
            ((x - bx).abs() <= 0.03125 / 2.0 && (y - by).abs() <= 0.046 / 2.0).then(|| self.group[k].unit)
        })
    }

    /// The group panel's buttons (CInfoPanelGroupButton): a 2 x 6 grid 0.185937 x
    /// 0.101562 centered 0.001563 left of the panel's bottom; each 0.03125 x 0.046
    /// button has its icon centered 0.016875 below its top and the life and mana bars
    /// below it 0.0025 apart.  The active subgroup's icons are 0.028 with 0.00375 bars
    /// under SelectedSubgroupHighlight (0.0375 wide, tinted 0xFFFFE400, added).
    fn group_draws(&self, db: &FrameDb, out: &mut Vec<Draw>) {
        if self.group.is_empty() {
            return;
        }
        let image = |texture: String, rect: w3_ui::Rect, color: [f32; 4], blend: w3_ui::Blend| Draw::Image { texture, rect, uv: [0.0, 0.0, 1.0, 1.0], color, blend, rotated: false };
        let bar_tex = db.decorate("SimpleHpBarConsoleSmall");
        let mana_tex = db.decorate("SimpleManaBarConsoleSmall");
        for (k, s) in self.group.iter().enumerate() {
            let (bx, by) = self.group_button_center(k);
            let top = by + 0.046 / 2.0;
            let (size, bar_h) = if s.active { (0.028, 0.00375) } else { (0.021875, 0.0025) };
            if s.active {
                let (hh, dy) = if s.mana.is_some() { (0.05225, 0.0) } else { (0.0466, 0.0025) };
                let r = w3_ui::Rect { x0: bx - 0.01875, x1: bx + 0.01875, y0: by + dy - hh / 2.0, y1: by + dy + hh / 2.0 };
                out.push(image(db.decorate("SelectedSubgroupHighlight"), r, [1.0, 0xe4 as f32 / 255.0, 0.0, 1.0], w3_ui::Blend::Add));
            }
            let icy = top - 0.016875;
            let icon = w3_ui::Rect { x0: bx - size / 2.0, x1: bx + size / 2.0, y0: icy - size / 2.0, y1: icy + size / 2.0 };
            out.push(image(s.icon.clone(), icon, s.color, w3_ui::Blend::Alpha));
            let mut bar_top = icon.y0 - 0.0025;
            let mut bar = |frac: f32, tex: &String, color: [f32; 4], out: &mut Vec<Draw>| {
                let r = w3_ui::Rect { x0: icon.x0, x1: icon.x1, y0: bar_top - bar_h, y1: bar_top };
                out.push(image(String::new(), r, [0.0, 0.0, 0.0, 1.0], w3_ui::Blend::Alpha));
                let f = w3_ui::Rect { x1: r.x0 + (r.x1 - r.x0) * frac.clamp(0.0, 1.0), ..r };
                out.push(image(tex.clone(), f, color, w3_ui::Blend::Alpha));
                bar_top = r.y0 - 0.0025;
            };
            bar(s.life, &bar_tex, life_color(s.life), out);
            if let Some(m) = s.mana {
                bar(m, &mana_tex, [0.0, 0.0, 1.0, 1.0], out);
            }
        }
    }

    /// A transport's cargo detail (CInfoPanelCargoDetail vf27): its name, and its grid
    /// of the hold's DataA slots, one row when there are at most three (or five), else
    /// two; the class line only with one row.
    fn show_cargo(&mut self, world: &World, h: u32, name: &str, class: &str) {
        self.cargo = None;
        self.tree.set_visible("SimpleInfoPanelCargoDetailC", false);
        if self.production.is_some() {
            return;
        }
        let Some((capacity, _, _)) = world.cargo_hold(h) else { return };
        let Some(u) = world.unit(h) else { return };
        if u.constructing.is_some() {
            return;
        }
        let capacity = capacity.max(0) as usize;
        let (rows, cols) = if capacity > 3 && capacity != 5 { (2, capacity / 2) } else { (1, capacity) };
        let aboard = u
            .extra
            .cargo
            .iter()
            .filter_map(|&c| {
                let cu = world.unit(c)?;
                let icon = world.objects.units.get(cu.type_id).map(|o| o.str("Art")).unwrap_or_default();
                Some((icon, cu.life / cu.max_life.max(1.0), world.cargo_size(c).max(1) as usize, c))
            })
            .collect();
        self.cargo = Some((rows, cols, aboard));
        if let Some(i) = self.info {
            self.tree.frames[i].visible = false;
        }
        self.tree.set_visible("SimpleInfoPanelCargoDetailC", true);
        self.tree.set_text("SimpleHoldNameValueC", name);
        self.tree.set_text("SimpleHoldDescriptionValueC", class);
        self.tree.set_visible("SimpleHoldDescriptionValueC", rows == 1);
    }

    /// The cargo grid (CCargoGrid, FUN_6f3cdef0): centered 0.007 below the detail's
    /// middle, 0.036875 x 0.044375 slots 0.00125 apart, each over CargoBackdrop; a unit
    /// aboard shows its icon (0.02875, 0.005 below the slot's top) with its life under
    /// it, and fills as many slots as its cargoSize.
    fn cargo_slot(&self, k: usize) -> Option<w3_ui::Rect> {
        let (rows, cols, _) = self.cargo.as_ref()?;
        let r = self.tree.get("SimpleInfoPanelCargoDetailC").and_then(|i| self.tree.frames[i].rect)?;
        let (gap, sw, sh) = (0.00125, 0.036875, 0.044375);
        let (gw, gh) = ((*cols as f32 + 1.0) * gap + *cols as f32 * sw, (*rows as f32 + 1.0) * gap + *rows as f32 * sh);
        let (cx, cy) = ((r.x0 + r.x1) / 2.0, (r.y0 + r.y1) / 2.0 - 0.007);
        let (x0, y1) = (cx - gw / 2.0, cy + gh / 2.0);
        let (row, col) = (k / (*cols).max(1), k % (*cols).max(1));
        let left = x0 + gap + col as f32 * (sw + gap);
        let top = y1 - (row as f32 + 1.0) * gap - row as f32 * sh;
        Some(w3_ui::Rect { x0: left, x1: left + sw, y0: top - sh, y1: top })
    }

    /// The unit aboard whose cargo slot is under an interface point.
    pub fn cargo_at(&self, x: f32, y: f32) -> Option<u32> {
        let (_, _, aboard) = self.cargo.as_ref()?;
        let mut k = 0;
        for (_, _, size, h) in aboard {
            for _ in 0..*size {
                if self.cargo_slot(k).is_some_and(|r| r.contains(x, y)) {
                    return Some(*h);
                }
                k += 1;
            }
        }
        None
    }

    fn cargo_draws(&self, db: &FrameDb, out: &mut Vec<Draw>) {
        let Some((rows, cols, aboard)) = &self.cargo else { return };
        let image = |texture: String, rect: w3_ui::Rect, color: [f32; 4]| Draw::Image { texture, rect, uv: [0.0, 0.0, 1.0, 1.0], color, blend: w3_ui::Blend::Alpha, rotated: false };
        let slot = |k: usize| self.cargo_slot(k).unwrap_or_default();
        let backdrop = db.decorate("CargoBackdrop");
        for k in 0..rows * cols {
            out.push(image(backdrop.clone(), slot(k), [1.0; 4]));
        }
        let bar_tex = db.decorate("SimpleHpBarConsoleSmall");
        let mut k = 0;
        for (icon, life, size, _) in aboard {
            for _ in 0..*size {
                if k >= rows * cols {
                    return;
                }
                let s = slot(k);
                let mid = (s.x0 + s.x1) / 2.0;
                let icon_r = w3_ui::Rect { x0: mid - 0.014375, x1: mid + 0.014375, y0: s.y1 - 0.005 - 0.02875, y1: s.y1 - 0.005 };
                out.push(image(icon.clone(), icon_r, [1.0; 4]));
                let bar = w3_ui::Rect { x0: icon_r.x0, x1: icon_r.x1, y0: icon_r.y0 - 0.0025 - 0.0025, y1: icon_r.y0 - 0.0025 };
                out.push(image(String::new(), bar, [0.0, 0.0, 0.0, 1.0]));
                out.push(image(bar_tex.clone(), w3_ui::Rect { x1: bar.x0 + (bar.x1 - bar.x0) * life.clamp(0.0, 1.0), ..bar }, life_color(*life)));
                k += 1;
            }
        }
    }

    /// The building detail while a structure produces (FUN_6f3ccfd0's modes): the
    /// action label, the queue backdrop when training or reviving, and the queue.
    fn show_production(&mut self, db: &FrameDb, world: &World, h: u32, name: &str) {
        use w3_game::info::ProductionKind as K;
        // Another's structure shows what it is making only under DisplayBuildingStatus;
        // the player's own, one it shares control of, and any to an observer always
        // (CInfoPanelUnitDetail::vf27).
        let me = world.local_player;
        let observer = world.players.get(me as usize).is_some_and(|p| p.get_state(w3_game::types::player_state::OBSERVER) != 0);
        let shown = observer || w3_game::natives::players::controls(world, me, h) || world.misc.get_str("Misc", "DisplayBuildingStatus").and_then(|v| v.trim().parse::<f32>().ok()).unwrap_or(0.0) != 0.0;
        self.production = if shown { world.production(h) } else { None };
        let Some(b) = self.building else { return };
        let Some(p) = &self.production else {
            self.tree.frames[b].visible = false;
            return;
        };
        self.tree.frames[b].visible = true;
        if let Some(i) = self.info {
            self.tree.frames[i].visible = false;
        }
        let t = &mut self.tree;
        t.set_text("SimpleBuildingNameValueB", name);
        t.set_visible("SimpleBuildingDescriptionValueB", false);
        let label = match p.kind {
            K::Training | K::Upgrading => "TRAINING",
            K::Researching => "RESEARCHING",
            K::Reviving => "REVIVING",
            K::Constructing => "CONSTRUCTING",
            K::Morphing => "MORPHING",
        };
        t.set_text("SimpleBuildingActionLabelB", &db.string(label));
        t.set_visible("SimpleBuildQueueBackdropB", matches!(p.kind, K::Training | K::Reviving));
        // No size in its definition; 0.105 x 0.009375 measured from the original.
        t.set_size("SimpleBuildTimeIndicatorB", Some(0.105), Some(0.009375));
        t.layout();
    }

    /// The building detail's bar and queue icons: the item in progress at 0.032 from
    /// the lower left + (0.0105, 0.081344), the rest in a 0.1772 x 0.02301 row of six
    /// 0.02301 icons from + (0.0095, 0.040438); the bar is SimpleBuildTimeIndicator's
    /// fill and border.
    fn production_draws(&self, db: &FrameDb, out: &mut Vec<Draw>) {
        let (Some(p), Some(b)) = (&self.production, self.building) else { return };
        let Some(r) = self.tree.frames[b].rect.filter(|_| self.tree.frames[b].visible) else { return };
        let image = |texture: String, rect: w3_ui::Rect, uv: [f32; 4]| Draw::Image { texture, rect, uv, color: [1.0; 4], blend: w3_ui::Blend::Alpha, rotated: false };
        if let Some(bar) = self.tree.get("SimpleBuildTimeIndicatorB").and_then(|f| self.tree.frames[f].rect) {
            let fill = w3_ui::Rect { x1: bar.x0 + (bar.x1 - bar.x0) * p.progress, ..bar };
            out.push(image(db.decorate("SimpleBuildTimeIndicator"), fill, [0.0, 0.0, p.progress, 1.0]));
            out.push(image(db.decorate("SimpleBuildTimeIndicatorBorder"), bar, [0.0, 0.0, 1.0, 1.0]));
        }
        if let Some(first) = p.icons.first() {
            let (x0, y1) = (r.x0 + 0.0105, r.y0 + 0.081344);
            out.push(image(first.clone(), w3_ui::Rect { x0, x1: x0 + 0.032, y0: y1 - 0.032, y1 }, [0.0, 0.0, 1.0, 1.0]));
        }
        let cell = 0.1772 / 6.0;
        for (k, icon) in p.icons.iter().skip(1).take(6).enumerate() {
            let (x0, y1) = (r.x0 + 0.0095 + cell * k as f32, r.y0 + 0.040438);
            out.push(image(icon.clone(), w3_ui::Rect { x0, x1: x0 + 0.02301, y0: y1 - 0.02301, y1 }, [0.0, 0.0, 1.0, 1.0]));
        }
    }

    /// The queue icon under an interface point: 0 for the one in progress, then the
    /// rest in order (as [`Hud::production_draws`] lays them out).
    pub fn queue_slot_at(&self, x: f32, y: f32) -> Option<usize> {
        let (Some(p), Some(b)) = (&self.production, self.building) else { return None };
        let r = self.tree.frames[b].rect.filter(|_| self.tree.frames[b].visible)?;
        if p.icons.is_empty() {
            return None;
        }
        let (x0, y1) = (r.x0 + 0.0105, r.y0 + 0.081344);
        if (w3_ui::Rect { x0, x1: x0 + 0.032, y0: y1 - 0.032, y1 }).contains(x, y) {
            return Some(0);
        }
        let cell = 0.1772 / 6.0;
        (0..p.icons.len().saturating_sub(1).min(6)).find(|&k| {
            let (x0, y1) = (r.x0 + 0.0095 + cell * k as f32, r.y0 + 0.040438);
            (w3_ui::Rect { x0, x1: x0 + 0.02301, y0: y1 - 0.02301, y1 }).contains(x, y)
        }).map(|k| k + 1)
    }

    /// A chosen destructable's name in SimpleInfoPanelDestructableDetail, or none.
    pub fn show_destructable(&mut self, name: Option<&str>) {
        self.tree.set_visible("SimpleInfoPanelDestructableDetail", name.is_some());
        if let Some(n) = name {
            self.tree.set_text("SimpleDestructableNameValue", n);
        }
    }

    /// A chosen item's name and description in SimpleInfoPanelItemDetail, or none.
    pub fn show_item(&mut self, item: Option<(&str, &str)>) {
        self.tree.set_visible("SimpleInfoPanelItemDetail", item.is_some());
        if let Some((name, desc)) = item {
            // The description hangs below the name, as tall as its text.
            let changed = self.tree.get("SimpleItemNameValue").is_some_and(|i| self.tree.frames[i].text.as_deref() != Some(name));
            self.tree.set_text("SimpleItemNameValue", name);
            self.tree.set_text("SimpleItemDescriptionValue", desc);
            if changed {
                self.tree.layout();
            }
        }
    }

    pub fn show_unit(&mut self, db: &FrameDb, world: &World, unit: Option<u32>) {
        let u = unit.and_then(|h| world.unit(h));
        // Inventory icons (ItemFunc Art) and charges.
        self.inventory = match (unit, u) {
            (Some(h), Some(u)) => (0..world.inventory_size(h))
                .map(|s| {
                    let it = world.item(u.inventory[s])?;
                    let art = world.objects.items.get(it.type_id).map(|o| o.str("Art")).unwrap_or_default();
                    Some((art, it.charges))
                })
                .collect(),
            _ => Vec::new(),
        };
        // Items cooling down, by their cooldown group.
        self.inventory_cooldowns = match (unit, u) {
            (Some(h), Some(u)) => (0..world.inventory_size(h))
                .map(|s| {
                    let it = world.item(u.inventory[s])?;
                    let group = world.item_cooldown_group(it.type_id)?;
                    let left = u.cooldowns.get(&group).copied().filter(|&l| l > 0.0)?;
                    let total = world.item_abilities(it.type_id).into_iter().map(|a| world.data(a, "Cool", 1)).fold(0.0f32, f32::max);
                    (total > 0.0).then(|| (1.0 - left / total).clamp(0.0, 1.0))
                })
                .collect(),
            _ => Vec::new(),
        };
        // Their tooltips (CInventoryBar::vf28): a usable item's name with its NumPad
        // key, "Drop item on shop to sell" with the price for a pawnable one of the
        // player's own (The Frozen Throne), "Left-Click to Use", and the Ubertip under
        // the Tooltips option.
        let mine = self.replay.is_none() && unit.is_some_and(|h| w3_game::natives::players::controls(world, world.local_player, h));
        self.inventory_tips = match (unit, u) {
            (Some(h), Some(u)) => (0..world.inventory_size(h))
                .map(|s| {
                    let item = u.inventory[s];
                    let o = world.objects.items.get(world.item(item)?.type_id)?;
                    let pawn = mine && world.version >= 1 && o.bool("pawnable");
                    let usable = mine && o.bool("usable");
                    let uber = if self.tooltips { crate::card::fill_placeholders(world, &o.str("Ubertip")) } else { String::new() };
                    let mut body = String::new();
                    if pawn {
                        body.push_str(&db.string("ITEM_PAWN_TOOLTIP"));
                        body.push('\n');
                    }
                    let mut title = o.str("Name");
                    if usable {
                        const NUMPAD: [u32; 6] = [7, 8, 4, 5, 1, 2];
                        title = db.string("ITEM_NAME_HOTKEY").replacen("%s", &title, 1).replacen("%u", &NUMPAD.get(s).copied().unwrap_or(0).to_string(), 1);
                        body.push_str(&db.string("ITEM_USE_TOOLTIP"));
                        if !uber.is_empty() {
                            body.push('\n');
                        }
                    }
                    body.push_str(&uber);
                    let (gold, lumber) = if pawn { world.pawn_value(item) } else { (0, 0) };
                    let mut b = crate::card::Button::text_only(title, body);
                    b.costs = [0, gold, lumber, 0];
                    Some(b)
                })
                .collect(),
            _ => Vec::new(),
        };
        if let Some(i) = self.info {
            self.tree.frames[i].visible = u.is_some();
        }
        if let Some(b) = self.building {
            self.tree.frames[b].visible = false;
        }
        self.production = None;
        let Some(u) = u else {
            self.portrait_stats = None;
            self.cargo = None;
            self.tree.set_visible("SimpleInfoPanelCargoDetailC", false);
            return;
        };
        let o = world.objects.units.get(u.type_id);
        let type_name = world.localized(&o.map(|o| o.str("Name")).unwrap_or_default());
        // Heroes go by their proper name (FUN_6f3af530).
        let name = if u.is_hero && !u.proper_name.is_empty() { world.localized(&u.proper_name) } else { type_name.clone() };
        if let Some(f) = self.tree.get("SimpleNameValue") {
            self.tree.frames[f].text = Some(name);
        }
        // The class line (FUN_6f3af350): "Level %u %s" for heroes, "Level %u" for
        // creeps, the gold left for mines.
        let class = if u.is_hero {
            world.localized("INFOPANEL_LEVEL_CLASS").replacen("%u", &u.level.to_string(), 1).replacen("%s", &type_name, 1)
        } else if let Some(g) = world.info_stats(unit.unwrap_or(0)).gold {
            format!("{}{g}", world.localized("COLON_GOLD"))
        } else if o.is_some_and(|o| o.str("race").eq_ignore_ascii_case("creeps")) {
            world.localized("INFOPANEL_LEVEL").replacen("%u", &u.level.to_string(), 1)
        } else {
            String::new()
        };
        self.tree.set_text("SimpleClassValue", &class);
        let shown_name = self.tree.get("SimpleNameValue").and_then(|f| self.tree.frames[f].text.clone()).unwrap_or_default();
        self.show_production(db, world, unit.unwrap_or(0), &shown_name);
        self.show_cargo(world, unit.unwrap_or(0), &shown_name, &class);
        // Buff icons for units (structures have no status bar), at most the bar's eight.
        self.buff_icons = (!u.is_building).then(|| {
            let mut icons: Vec<String> = Vec::new();
            // Auras' buffs after the rest (measured: Inner Fire shows before Devotion
            // Aura whichever came first).
            let mut order: Vec<&w3_game::types::Buff> = u.buffs.iter().collect();
            order.sort_by_key(|b| b.aura_left.is_some());
            for b in order {
                let art = world.objects.buffs.get(b.id).map(|o| o.str("Buffart")).unwrap_or_default();
                if !art.is_empty() && !icons.contains(&art) {
                    icons.push(art);
                }
            }
            icons.truncate(8);
            icons
        });
        // The hero level bar: experience through the current level, full at the top
        // level (measured: a level 10 Death Knight's shows full).
        self.level_bar = u.is_hero.then(|| {
            if u.level >= world.misc_f32("MaxHeroLevel", 10.0) as i32 {
                return 1.0;
            }
            let (lo, hi) = (world.hero_xp_needed(u.level), world.hero_xp_needed(u.level + 1));
            if hi > lo { ((u.hero_xp - lo) as f32 / (hi - lo) as f32).clamp(0.0, 1.0) } else { 1.0 }
        });
        self.tree.set_visible("SimpleHeroLevelBar", u.is_hero);
        // No width in its definition; 0.17875 measured from the original.
        self.tree.set_size("SimpleHeroLevelBar", Some(0.17875), None);
        self.tree.layout();
        self.show_info_icons(db, world, unit.unwrap_or(0));
        let hp = format!("{} / {}", u.life.ceil().max(0.0) as u32, u.max_life.ceil() as u32);
        let mana = (u.max_mana > 0.0).then(|| format!("{} / {}", u.mana.floor().max(0.0) as u32, u.max_mana.floor() as u32));
        self.portrait_stats = Some((hp, life_color(u.life / u.max_life.max(1.0)), mana));
        let _ = db;
    }

    pub fn update(&mut self, db: &FrameDb, world: &World) {
        // CUpperButtonBar (vf9, FUN_6f3af910): Menu always; Allies only in network
        // games; the F12 button is the message log outside them; Quests once a map has
        // made a quest.
        // The game menu, the quest log, the message log, the allies and chat dialogs,
        // and a script dialog (FUN_6f3aee90) disable them all while open (as captured
        // from the original).
        let menu = self.menu.is_some() || self.quests.is_some() || self.log_view.is_some() || self.alliance.is_some() || self.chat_dialog.is_some() || self.dialog.is_some();
        self.tree.set_enabled("UpperButtonBarMenuButton", !menu);
        self.tree.set_enabled("UpperButtonBarAlliesButton", world.network && !menu);
        self.tree.set_enabled("UpperButtonBarQuestsButton", !menu && !world.quests.is_empty());
        self.tree.set_enabled("UpperButtonBarChatButton", !menu);
        if let Some(f) = self.tree.get("UpperButtonBarChatButton") {
            // In a network game it is Chat (CUpperButtonBar::vf9).
            let key = match (world.network, menu) {
                (true, true) => "CHAT",
                (true, false) => "KEY_CHAT",
                (false, true) => "LOG",
                (false, false) => "KEY_LOG",
            };
            self.tree.frames[f].text = Some(db.string(key));
        }
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
            {
                // FUN_6f3d1080: the food used alone when nothing provides any.
                let (used, cap) = (p.get_state(player_state::RESOURCE_FOOD_USED), p.get_state(player_state::RESOURCE_FOOD_CAP));
                if cap == 0 { used.to_string() } else { format!("{used}/{cap}") }
            },
        );
        // Red when more food is used than provided.
        if let Some(f) = self.supply {
            let (used, cap) = (p.get_state(player_state::RESOURCE_FOOD_USED), p.get_state(player_state::RESOURCE_FOOD_CAP));
            self.tree.frames[f].color = (cap != 0 && used > cap).then_some([1.0, 0.0, 0.0, 1.0]);
        }
        let key = match world.upkeep_level(world.local_player) {
            0 => "UPKEEP_NONE",
            1 => "UPKEEP_LOW",
            _ => "UPKEEP_HIGH",
        };
        set(&mut self.tree, self.upkeep, db.string(key));
        self.hover_tips = self.hover_tip_list(db, world);
    }

    /// An upkeep level's name (FUN_6f3dc320): none, low, high, and UPKEEP_HIGH<n> past
    /// those (UPKEEP_HIGH and the number when that is missing).
    fn upkeep_name(db: &FrameDb, level: usize) -> String {
        match level {
            0 => db.string("UPKEEP_NONE"),
            1 => db.string("UPKEEP_LOW"),
            2 => db.string("UPKEEP_HIGH"),
            n => {
                let s = db.string(&format!("UPKEEP_HIGH{}", n - 1));
                if s.is_empty() || s == format!("UPKEEP_HIGH{}", n - 1) { format!("{}{}", db.string("UPKEEP_HIGH"), n - 1) } else { s }
            }
        }
    }

    /// The tooltips of the resource bar (FUN_6f3c99d0), the upper buttons (FUN_6f3bc7e0),
    /// the time of day (CTimeCover::vf34), and the minimap buttons and formation toggle
    /// (FUN_6f370540), with where each shows.
    fn hover_tip_list(&self, db: &FrameDb, world: &World) -> Vec<(w3_ui::Rect, crate::card::Button)> {
        use crate::card::Button;
        let mut out = Vec::new();
        let Some(p) = world.players.get(world.local_player as usize) else { return out };
        // The resource bar's four: from each icon to its count.
        if let Some(r) = self.tree.get("ResourceBarFrame").and_then(|f| self.tree.frames[f].rect) {
            let (y1, y0) = (r.y1 - 0.003125, r.y1 - 0.003125 - 0.01640625);
            let cell = |x0: f32, right: f32| w3_ui::Rect { x0: r.x0 + x0, x1: r.x1 - right, y0, y1 };
            let gold = p.get_state(player_state::RESOURCE_GOLD);
            out.push((cell(0.0, 0.26484375), Button::text_only(format!("{} {gold}", db.string("COLON_GOLD")), db.string("RESOURCE_UBERTIP_GOLD"))));
            let lumber = p.get_state(player_state::RESOURCE_LUMBER);
            out.push((cell(0.0859375, 0.178125), Button::text_only(format!("{} {lumber}", db.string("COLON_LUMBER")), db.string("RESOURCE_UBERTIP_LUMBER"))));
            let (used, cap) = (p.get_state(player_state::RESOURCE_FOOD_USED), p.get_state(player_state::RESOURCE_FOOD_CAP));
            let (open, close) = if used > cap { ("|CFFFF0000", "|r") } else { ("", "") };
            out.push((cell(0.17265625, 0.09140625), Button::text_only(format!("{} {open}{used}/{cap}{close}", db.string("COLON_FOOD")), db.string("RESOURCE_UBERTIP_SUPPLY"))));
            if let Some(u) = self.upkeep.and_then(|f| self.tree.frames[f].rect) {
                let (gold_tax, lumber_tax) = world.upkeep_tax(world.local_player);
                let mut rate = format!("{} {}%", db.string("COLON_GOLD_INCOME_RATE"), 100 - gold_tax);
                if lumber_tax > 0 {
                    rate.push_str(&format!("|n{} {}%", db.string("COLON_LUMBER_INCOME_RATE"), 100 - lumber_tax));
                }
                let mut uber = db.string("RESOURCE_UBERTIP_UPKEEP");
                for (from, to, level, gold, lumber) in world.upkeep_levels() {
                    let name = Self::upkeep_name(db, level);
                    let line = if lumber < 100 {
                        c_format(&db.string("RESOURCE_UBERTIP_UPKEEP_INFO_WOOD"), &[&from.to_string(), &to.to_string(), &name, &gold.to_string(), &lumber.to_string()])
                    } else {
                        c_format(&db.string("RESOURCE_UBERTIP_UPKEEP_INFO"), &[&from.to_string(), &to.to_string(), &name, &gold.to_string()])
                    };
                    uber.push_str(&line);
                }
                let mut b = Button::text_only(Self::upkeep_name(db, world.upkeep_level(world.local_player)), uber);
                b.requires = rate;
                out.push((u, b));
            }
        }
        if self.tooltips {
            // The upper buttons, while usable; F12 is Chat in network games and the
            // message log otherwise.
            let chat = if world.network { "CHAT" } else { "LOG" };
            for (frame, key) in [("UpperButtonBarMenuButton", "MENU"), ("UpperButtonBarAlliesButton", "ALLY"), ("UpperButtonBarChatButton", chat), ("UpperButtonBarQuestsButton", "QUEST")] {
                let Some(f) = self.tree.get(frame) else { continue };
                if !self.tree.frames[f].enabled {
                    continue;
                }
                if let Some(r) = self.tree.frames[f].rect {
                    out.push((r, Button::text_only(db.string(&format!("UPPER_BUTTON_{key}_TIP")), db.string(&format!("UPPER_BUTTON_{key}_UBER")))));
                }
            }
            // The clock: a 0.04125 square at the top of the console.
            let root = self.tree.frames[0].rect.unwrap_or_default();
            let cx = (root.x0 + root.x1) / 2.0;
            let clock = w3_ui::Rect { x0: cx - 0.020625, x1: cx + 0.020625, y0: root.y1 - 0.04125, y1: root.y1 };
            let hour = world.time_of_day;
            let time = format!("{}:{:02}", hour as i32, ((hour - hour.trunc()) * 60.0) as i32);
            out.push((clock, Button::text_only(db.string("TIME_OF_DAY_TOOLTIP").replacen("%s", &time, 1), db.string("TIME_OF_DAY_UBERTIP"))));
        }
        // The minimap buttons and the formation toggle: the full tip under the Tooltips
        // option, the name alone without.
        for (k, key) in ["MINIMAPSIGNALTOOLTIP", "MINIMAPTERRAINTOOLTIP", "MINIMAPALLYCOLORTOOLTIP", "MINIMAPCREEPCOLORTOOLTIP", "FORMATION_TOOLTIP"].iter().enumerate() {
            let uber = if self.tooltips { db.string(&format!("{key}_UBER")) } else { String::new() };
            out.push((self.minimap_button_rect(k), Button::text_only(db.string(key), uber)));
        }
        out
    }

    fn minimap_rect(&self) -> w3_ui::Rect {
        let root = self.tree.frames[0].rect.unwrap_or_default();
        w3_ui::Rect { x0: root.x0 + MINIMAP[0], y0: root.y0 + MINIMAP[1], x1: root.x0 + MINIMAP[2], y1: root.y0 + MINIMAP[3] }
    }

    /// Interface position of a world point on the minimap.
    pub fn world_to_minimap(&self, x: f32, y: f32) -> Option<(f32, f32)> {
        let a = self.minimap_area?;
        let (sx, sy, side) = a.square();
        let r = self.minimap_rect();
        Some((r.x0 + (x - sx) / side * r.w(), r.y0 + (y - sy) / side * r.h()))
    }

    /// The world point of a war3map.mmp icon (256 units across the minimap, from the
    /// top).
    pub fn minimap_icon_world(&self, x: f32, y: f32) -> Option<(f32, f32)> {
        let a = self.minimap_area?;
        let (sx, sy, side) = a.square();
        Some((sx + x / 256.0 * side, sy + (256.0 - y) / 256.0 * side))
    }

    /// World point under an interface position, if it is on the minimap.
    pub fn minimap_to_world(&self, ux: f32, uy: f32) -> Option<(f32, f32)> {
        let a = self.minimap_area?;
        let r = self.minimap_rect();
        if !r.contains(ux, uy) {
            return None;
        }
        let (sx, sy, side) = a.square();
        Some((sx + (ux - r.x0) / r.w() * side, sy + (uy - r.y0) / r.h() * side))
    }

    /// CPortraitButton (game.dll 0x6f3a0d40): 0.09 x 0.0856 at (0.2075, 0.030625).
    pub fn portrait_rect(&self) -> w3_ui::Rect {
        let root = self.tree.frames[0].rect.unwrap_or_default();
        let (x0, y0) = (root.x0 + 0.2075, root.y0 + 0.030625);
        w3_ui::Rect { x0, y0, x1: x0 + 0.09, y1: y0 + 0.0856 }
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

    /// The replay panel, in the command card's place.
    pub fn show_replay(&mut self, db: &FrameDb, world: &World, team_rgb: &[[f32; 3]; 16], length: f32) {
        self.replay = Some(crate::replaypanel::ReplayPanel::new(&mut self.tree, db, world, team_rgb, (CARD_W, CARD_H, CARD_RIGHT, CARD_BOTTOM), length));
    }

    /// The card button at an interface position.
    /// The card slots whose autocast is on.
    pub fn autocast_slots(&self) -> Vec<(u32, u32, w3_ui::Rect)> {
        self.card.iter().filter(|b| b.autocast.is_some_and(|a| a.2)).map(|b| (b.x, b.y, self.slot_rect(b.x, b.y))).collect()
    }

    /// Buttons whose ability is cooling down, with the share of its cooldown gone.
    pub fn cooldown_slots(&self) -> Vec<(u32, u32, w3_ui::Rect, f32)> {
        let mut out: Vec<(u32, u32, w3_ui::Rect, f32)> = self.card.iter().filter_map(|b| b.cooldown.map(|(left, total)| (b.x, b.y, self.slot_rect(b.x, b.y), (1.0 - left / total).clamp(0.0, 1.0)))).collect();
        // Inventory slots too, keyed apart from the card's.
        for (s, gone) in self.inventory_cooldowns.iter().enumerate() {
            if let Some(g) = gone {
                out.push((100 + s as u32, 0, self.inventory_rect(s), *g));
            }
        }
        out
    }

    pub fn button_at(&self, x: f32, y: f32) -> Option<&crate::card::Button> {
        self.card.iter().find(|b| self.slot_rect(b.x, b.y).contains(x, y))
    }

    /// Adds trigger text (DisplayTextToPlayer): `x` and `y` from 0 to 1 move the frame,
    /// and a negative duration means the default of 3 seconds plus 0.1 per byte
    /// (game.dll jass_DisplayTextToPlayer).
    pub fn unit_message(&mut self, text: &str, x: f32, y: f32, duration: f32) {
        let duration = if duration < 0.0 { 3.0 + 0.1 * text.len() as f32 } else { duration };
        self.unit_message_offset = (x.max(0.0), y.max(0.0));
        self.unit_messages.push(Message { text: text.to_string(), until: self.now + duration });
        self.log.push(text.to_string());
    }

    /// A line in the chat message frame for `duration` seconds (or by its length).
    pub fn chat_message(&mut self, text: &str, duration: f32) {
        let duration = if duration < 0.0 { 3.0 + 0.1 * text.len() as f32 } else { duration };
        self.chat_messages.push(Message { text: text.to_string(), until: self.now + duration });
        self.log.push(text.to_string());
    }

    /// Shows an error line for 10 seconds, replacing the previous one (FUN_6f36de80).
    pub fn error(&mut self, text: &str) {
        self.error_message = Some(Message { text: text.to_string(), until: self.now + 10.0 });
        self.target_prompt = false;
    }

    /// Follows target mode: entering it clears the message frame for "Select Target".
    pub fn set_targeting(&mut self, on: bool) {
        if on && !self.targeting {
            self.error_message = None;
            self.target_prompt = true;
        }
        if !on {
            self.target_prompt = false;
        }
        self.targeting = on;
    }

    /// Shows the first script dialog displayed to `player`, rebuilding its frames when it
    /// changes, laid out as CScriptDialogWar3 does (FUN_6f3c9550, FUN_6f3aa500).
    pub fn sync_dialog(&mut self, db: &FrameDb, w: &w3_game::World, player: u8, aspect: f32) {
        use w3_game::types::Obj;
        let shown = w.handles.iter().find_map(|(h, o)| match o {
            Obj::Dialog(d) if d.shown & (1 << player) != 0 => Some((*h, d.clone())),
            _ => None,
        });
        let Some((h, d)) = shown else {
            self.dialog = None;
            return;
        };
        let labels: Vec<String> = d.buttons.iter().filter_map(|b| match w.handles.get(*b) {
            Some(Obj::DialogButton(b)) => Some(b.text.clone()),
            _ => None,
        }).collect();
        let content = (d.message.clone(), labels.clone());
        if self.dialog.as_ref().is_some_and(|v| v.dialog == h && v.content == content) {
            return;
        }
        let mut tree = FrameTree::new(aspect);
        let Some(root) = tree.create(db, "ScriptDialog", 0) else { return };
        tree.set_text("ScriptDialogText", &d.message);
        let mut buttons = Vec::new();
        let mut prev: Option<String> = None;
        for (k, (label, &b)) in labels.iter().zip(&d.buttons).enumerate() {
            let scope = format!("#{k}");
            let Some(i) = tree.create_scoped(db, "ScriptDialogButton", root, &scope) else { continue };
            tree.set_text(&format!("ScriptDialogButtonText{scope}"), label);
            let name = format!("ScriptDialogButton{scope}");
            use w3_ui::FramePoint as P;
            match &prev {
                // Each button hangs 0.003 below the last; the first 0.011 below the
                // message, or 0.027 below the top without one.
                Some(p) => tree.set_point(&name, P::Top, p, P::Bottom, 0.0, -0.003),
                None if d.message.is_empty() => tree.set_point(&name, P::Top, "ScriptDialog", P::Top, 0.0, -0.027),
                None => tree.set_point(&name, P::Top, "ScriptDialogText", P::Bottom, 0.0, -0.011),
            }
            prev = Some(name);
            buttons.push((i, b));
        }
        // Height: 0.03 margins around the message (plus 0.011 to the buttons) and the
        // button stack, rounded up to 0.048 steps with at least 0.096; the message is
        // centered in the extra space.
        let text_h = 0.012;
        let button_h = buttons.first().and_then(|&(i, _)| tree.frames[i].height).unwrap_or(0.035);
        let title = if d.message.is_empty() { 0.03 } else { text_h + 0.008 + 0.003 + 0.03 };
        let n = buttons.len() as f32;
        let stack = if n > 0.0 { (button_h + 0.003) * n - 0.003 + 0.03 } else { 0.03 };
        let total = title + stack;
        let (height, top) = if total >= 0.096 {
            let rem = total % 0.048;
            let h = if rem != 0.0 { total - rem + 0.048 } else { total };
            (h, -((h - (total - 0.03 - 0.03)) * 0.5))
        } else {
            (0.096, -0.03)
        };
        tree.set_size("ScriptDialog", None, Some(height));
        tree.set_point("ScriptDialogText", w3_ui::FramePoint::Top, "ScriptDialog", w3_ui::FramePoint::Top, 0.0, top);
        // Centered in its parent, the world frame (CWorldFrameWar3), which runs from 0.13
        // above the bottom to 0.02 below the top (FUN_6f367710).
        tree.set_point("ScriptDialog", w3_ui::FramePoint::Center, "", w3_ui::FramePoint::Center, 0.0, (0.13 - 0.02) / 2.0);
        tree.bind_points();
        tree.layout();
        self.dialog = Some(DialogView { dialog: h, content, tree, buttons });
    }

    pub fn open_menu(&mut self, db: &FrameDb, aspect: f32) {
        let mut tree = FrameTree::new(aspect);
        tree.plain_button_text = true;
        let Some(_) = tree.create(db, "EscMenuBackdrop", 0) else { return };
        use w3_ui::FramePoint as P;
        if let Some(b) = tree.get("EscMenuBackdrop") {
            tree.create(db, "EscMenuMainPanel", b);
            tree.create(db, "EscMenuOptionsPanel", b);
            // The save and load panel, its file list's border (an edit box's), and the
            // name typed inside the edit box's border.
            if let Some(s) = tree.create(db, "EscMenuSaveGamePanel", b) {
                if tree.create(db, "EscMenuEditBoxBackdropTemplate", s).is_some() {
                    tree.set_point("EscMenuEditBoxBackdropTemplate", P::TopLeft, "FileListFrame", P::TopLeft, 0.0, 0.0);
                    tree.set_point("EscMenuEditBoxBackdropTemplate", P::BottomRight, "FileListFrame", P::BottomRight, 0.0, 0.0);
                }
                tree.set_point("SaveGameFileEditBoxText", P::TopLeft, "SaveGameFileEditBox", P::TopLeft, 0.009, -0.009);
                tree.set_point("SaveGameFileEditBoxText", P::BottomRight, "SaveGameFileEditBox", P::BottomRight, -0.009, 0.009);
            }
        }
        tree.set_point("EscMenuBackdrop", P::Center, "", P::Center, 0.0, 0.06);
        tree.place_text_area_scroll_bars();
        // Pausing is for network games (shown then by the scene); a single player game
        // pauses while the menu is open.
        tree.set_visible("PauseButton", false);
        Self::menu_panel(&mut tree, "MainPanel");
        self.save_panel = None;
        tree.bind_points();
        tree.fit_text_widths();
        tree.layout();
        self.menu = Some(tree);
    }

    /// Shows the save panel's state in the menu.
    pub fn sync_save_panel(&mut self, db: &FrameDb) {
        if let (Some(p), Some(m)) = (self.save_panel.as_mut(), self.menu.as_mut()) {
            p.sync(m, db);
        }
    }

    /// Shows one of the menu's panels.
    pub fn menu_panel(tree: &mut FrameTree, panel: &str) {
        for p in ["MainPanel", "EndGamePanel", "ConfirmQuitPanel", "HelpPanel", "TipsPanel", "EscMenuOptionsPanel", "EscMenuSaveGamePanel"] {
            tree.set_visible(p, p == panel);
        }
        // The backdrop takes the panel's size (Help and Tips are wider), else its own;
        // the save and load panel sizes it itself.
        if panel != "EscMenuSaveGamePanel" {
            let size = |k: &str| tree.get(panel).and_then(|i| tree.frames[i].def.prop(k)).and_then(|p| p.args.first()).map(|a| a.num());
            tree.set_size("EscMenuBackdrop", Some(size("Width").unwrap_or(0.288)), Some(size("Height").unwrap_or(0.384)));
            tree.bind_points();
            tree.layout();
        }
    }

    /// Drops expired messages.
    pub fn tick(&mut self, now: f32) {
        self.now = now;
        self.unit_messages.retain(|m| m.until > now);
        self.chat_messages.retain(|m| m.until > now);
        if self.error_message.as_ref().is_some_and(|m| m.until <= now) {
            self.error_message = None;
        }
    }

    fn message_draws(&self, db: &FrameDb, out: &mut Vec<Draw>) {
        let root = self.tree.frames[0].rect.unwrap_or_default();
        let cx = (root.x0 + root.x1) / 2.0;
        let font = db.decorate("MessageFont");
        let height = |key: &str| db.misc_ui.get_str("FontHeights", key).and_then(|v| v.trim().parse().ok()).unwrap_or(0.015);
        let shadow = Some(([0.0, 0.0, 0.0, 1.0], 0.001, -0.001));
        // WorldFrameMessage: 0.38 x 0.05, its bottom 0.18 above the bottom of the screen.
        let msg = w3_ui::Rect { x0: cx - 0.19, x1: cx + 0.19, y0: root.y0 + 0.18, y1: root.y0 + 0.23 };
        // Its text runs from its left edge in gold (0xfed312, as captured from the
        // original): an error, or "Select Target".
        let message = match &self.error_message {
            Some(m) => Some(m.text.clone()),
            None => self.target_prompt.then(|| db.string("SELECTTARGET")),
        };
        if let Some(text) = message {
            let size = height("WorldFrameMessage");
            let gold = [0xfe as f32 / 255.0, 0xd3 as f32 / 255.0, 0x12 as f32 / 255.0, 1.0];
            out.push(Draw::Text { font: font.clone(), size, rect: msg, text, color: gold, justify_h: w3_ui::Justify::Start, justify_v: w3_ui::Justify::End, shadow });
        }
        // WorldFrameUnitMessage: 0.5 x 0.25 with its bottom center on the message frame's
        // top, 0.098125 left of center and 0.0005 up at offset (0, 0); offsets of 1 move
        // it to x 0.8 and y 0.452 (FUN_6f374d20).
        // On a wider screen its x measures stretch with it (measured at 16:9: its text
        // starts 86 pixels in at 1280 by 720, and a line 766 pixels long goes unwrapped).
        if !self.unit_messages.is_empty() {
            let size = height("WorldFrameUnitMessage");
            let (ox, oy) = self.unit_message_offset;
            let wide = self.tree.width / w3_ui::UI_WIDTH;
            let ax0 = cx - 0.098125 * wide;
            let ay0 = msg.y1 + 0.0005;
            let ax = ax0 + (root.x0 + 0.8 * wide - ax0) * ox;
            let ay = ay0 + (root.y0 + 0.452 - ay0) * oy;
            let rect = w3_ui::Rect { x0: ax - 0.25 * wide, x1: ax + 0.25 * wide, y0: ay, y1: ay + 0.25 };
            let text = self.unit_messages.iter().map(|m| m.text.replace("|n", "\n")).collect::<Vec<_>>().join("\n");
            out.push(Draw::Text { font, size, rect, text, color: [1.0; 4], justify_h: w3_ui::Justify::Start, justify_v: w3_ui::Justify::End, shadow });
        }
        // The chat message frame: from the screen's top down to 0.2 above its bottom,
        // 0.00625 in from its left and 0.02 from its right, in MasterFont (unless the
        // player chose another) at WorldFrameChatMessage, its lines kept to the bottom.
        if !self.chat_messages.is_empty() {
            let size = height("WorldFrameChatMessage");
            let rect = w3_ui::Rect { x0: root.x0 + 0.00625, x1: root.x1 - 0.02, y0: root.y0 + 0.2, y1: root.y1 };
            let text = self.chat_messages.iter().map(|m| m.text.replace("|n", "\n")).collect::<Vec<_>>().join("\n");
            out.push(Draw::Text { font: db.decorate("MasterFont"), size, rect, text, color: [1.0; 4], justify_h: w3_ui::Justify::Start, justify_v: w3_ui::Justify::End, shadow });
        }
    }

    /// The interface's draws.  ShowInterface's fade (FUN_6f373c20) fades the game
    /// interface out and the cinematic panel in, or back, over its seconds.
    pub fn draws(&self, db: &FrameDb) -> Vec<Draw> {
        let mut out = self.layer_draws(db);
        self.chat_bar_draws(db, &mut out);
        out
    }

    /// CChatEditBar (FUN_6f39cb10, placed by CGameUI): 0.03 tall, from 0.2 in from the
    /// screen's left to 0.2 in from its right, centered 0.145 above its bottom, a
    /// ToolTipBorder/ToolTipBackground box (corners 0.01, insets 0.0019) with the line in
    /// ChatFont at [FontHeights] ChatEditBar, 0.003 in.
    fn chat_bar_draws(&self, db: &FrameDb, out: &mut Vec<Draw>) {
        use w3_ui::fdf::{Arg, Def, Prop};
        let Some(text) = &self.chat else { return };
        let root = self.tree.frames[0].rect.unwrap_or_default();
        let (x0, x1, cy) = (root.x0 + 0.2, root.x1 - 0.2, root.y0 + 0.145);
        let r = w3_ui::Rect { x0, x1, y0: cy - 0.015, y1: cy + 0.015 };
        let prop = |key: &str, args: Vec<Arg>| Prop { key: key.into(), args };
        let s = |v: &str| Arg::Str(v.into());
        let def = Def {
            block: "Frame".into(),
            kind: "BACKDROP".into(),
            name: String::new(),
            inherits: None,
            with_children: false,
            props: vec![
                prop("BackdropBackground", vec![s("ToolTipBackground")]),
                prop("BackdropCornerFlags", vec![s("UL|UR|BL|BR|T|L|B|R")]),
                prop("BackdropCornerSize", vec![Arg::Num(0.01)]),
                prop("BackdropBackgroundInsets", vec![Arg::Num(0.0019); 4]),
                prop("BackdropEdgeFile", vec![s("ToolTipBorder")]),
            ],
            children: vec![],
        };
        self.tree.backdrop(db, &def, r, out);
        let size = db.misc_ui.get_str("FontHeights", "ChatEditBar").and_then(|v| v.trim().parse().ok()).unwrap_or(0.013f32);
        let font = db.decorate("ChatFont");
        // The prompt, 0xFFCCAC12, 0.003 in, then the line 0.002 after it, both shadowed
        // 0.001 down and right (FUN_6f39cb10).
        let shadow = Some(([0.0, 0.0, 0.0, 1.0], 0.001, -0.001));
        let prompt_w = db.text_width(&font, size, &self.chat_prompt);
        let pr = w3_ui::Rect { x0: r.x0 + 0.003, x1: r.x0 + 0.003 + prompt_w, y0: r.y0, y1: r.y1 };
        out.push(Draw::Text { font: font.clone(), size, rect: pr, text: self.chat_prompt.clone(), color: [0xcc as f32 / 255.0, 0xac as f32 / 255.0, 0x12 as f32 / 255.0, 1.0], justify_h: w3_ui::Justify::Start, justify_v: w3_ui::Justify::Middle, shadow });
        let tr = w3_ui::Rect { x0: pr.x1 + 0.002, x1: r.x1 - 0.0015, y0: r.y0, y1: r.y1 };
        out.push(Draw::Text { font, size, rect: tr, text: format!("{text}|"), color: [1.0; 4], justify_h: w3_ui::Justify::Start, justify_v: w3_ui::Justify::Middle, shadow });
    }

    fn layer_draws(&self, db: &FrameDb) -> Vec<Draw> {
        let (ui, cine) = self.fades;
        if ui >= 1.0 || cine <= 0.0 {
            return self.interface_draws(db, false);
        }
        if cine >= 1.0 || ui <= 0.0 {
            return self.interface_draws(db, true);
        }
        let mut out = faded(self.interface_draws(db, false), ui);
        if let Some(c) = &self.cine {
            out.extend(faded(c.draw(db), cine));
        }
        out
    }

    fn interface_draws(&self, db: &FrameDb, cinematic: bool) -> Vec<Draw> {
        // While the interface is hidden only the cinematic panel, text messages, and the
        // filter show.
        if cinematic {
            let mut out = Vec::new();
            self.filter_draws(&mut out);
            if let Some(c) = &self.cine {
                out.extend(c.draw(db));
            }
            self.message_draws(db, &mut out);
            return out;
        }
        let mut out = Vec::new();
        out.extend(self.tree.draw(db));
        self.filter_draws(&mut out);
        self.hero_bar_draws(db, &mut out);
        if !self.idle_workers.is_empty() {
            let r = self.idle_worker_rect();
            let icon = db.decorate("IdlePeon");
            let icon = if self.paused { crate::card::disabled_icon(&icon) } else { icon };
            out.push(Draw::Image { texture: icon, rect: r, uv: [0.0, 0.0, 1.0, 1.0], color: [1.0; 4], blend: w3_ui::Blend::Alpha, rotated: false });
            Self::number_overlay(db, r, self.idle_workers.len() as i32, &mut out);
        }
        self.minimap_button_draws(db, &mut out);
        // CStatBar of the experience kind: a black backdrop 0.001 outside, the
        // SimpleXpBarConsole fill tinted 0xFF8B0083, and the SimpleXpBarBorder.
        if let (Some(p), Some(f)) = (self.level_bar, self.tree.get("SimpleHeroLevelBar")) {
            if let Some(r) = self.tree.frames[f].rect.filter(|_| self.tree.is_visible(f)) {
                let image = |texture: String, rect: w3_ui::Rect, uv: [f32; 4], color: [f32; 4]| Draw::Image { texture, rect, uv, color, blend: w3_ui::Blend::Alpha, rotated: false };
                let outer = w3_ui::Rect { x0: r.x0 - 0.001, x1: r.x1 + 0.001, y0: r.y0 - 0.001, y1: r.y1 + 0.001 };
                let fill = w3_ui::Rect { x1: r.x0 + (r.x1 - r.x0) * p, ..r };
                let bar = [
                    image(String::new(), outer, [0.0, 0.0, 1.0, 1.0], [0.0, 0.0, 0.0, 1.0]),
                    image(db.decorate("SimpleXpBarConsole"), fill, [0.0, 0.0, p, 1.0], [0x8b as f32 / 255.0, 0.0, 0x83 as f32 / 255.0, 1.0]),
                    image(db.decorate("SimpleXpBarBorder"), r, [0.0, 0.0, 1.0, 1.0], [1.0; 4]),
                ];
                // Under the class text, which SimpleUnitStatsPanel keeps above the bars.
                let class_text = self.tree.get("SimpleClassValue").and_then(|c| self.tree.frames[c].text.clone());
                let at = out.iter().position(|d| matches!(d, Draw::Text { text, .. } if Some(text) == class_text.as_ref())).unwrap_or(out.len());
                out.splice(at..at, bar);
            }
        }
        self.message_draws(db, &mut out);
        let root = self.tree.frames[0].rect.unwrap_or_default();
        {
            if !self.info_visible() || self.inventory.is_empty() {
                // CInfoBar's cover over the slots of a unit without an inventory: texture
                // coordinates (0.31640625, 0)-(1, 1) with the lower right at (0.6, 0), at
                // the console's 0.0005 per texel (fit to the original's rendering).
                let r = w3_ui::Rect { x0: root.x0 + 0.6 - 0.0875, x1: root.x0 + 0.6, y0: root.y0, y1: root.y0 + 0.256 };
                out.push(Draw::Image { texture: db.decorate("ConsoleInventoryCoverTexture"), rect: r, uv: [0.31640625, 0.0, 1.0, 1.0], color: [1.0; 4], blend: w3_ui::Blend::Alpha, rotated: false });
            } else if self.info_visible() {
                // Slots beyond the capacity take the filler.
                for s in self.inventory.len()..6 {
                    let r = self.inventory_rect(s);
                    out.push(Draw::Image { texture: db.decorate("ConsoleInventoryNoCapacity"), rect: r, uv: [0.0, 0.0, 1.0, 1.0], color: [1.0; 4], blend: w3_ui::Blend::Alpha, rotated: false });
                }
                // "INVENTORY": 0.076875 x 0.01125 from CInfoBar's top right + (0.015625,
                // 0.016563), MasterFont at [FontHeights] Inventory, gold with a shadow.
                let x0 = root.x0 + 0.309375 + 0.188125 + 0.015625;
                let y1 = root.y0 + 0.114063 + 0.016563;
                let rect = w3_ui::Rect { x0, x1: x0 + 0.076875, y0: y1 - 0.01125, y1 };
                let size = db.misc_ui.get_str("FontHeights", "Inventory").and_then(|v| v.trim().parse().ok()).unwrap_or(0.011);
                let color = [0xfe as f32 / 255.0, 0xd3 as f32 / 255.0, 0x12 as f32 / 255.0, 1.0];
                out.push(Draw::Text { font: db.decorate("MasterFont"), size, rect, text: db.string("INVENTORY"), color, justify_h: w3_ui::Justify::Middle, justify_v: w3_ui::Justify::Middle, shadow: Some(([0.0, 0.0, 0.0, 0xee as f32 / 255.0], 0.001, -0.001)) });
            }
        }
        self.production_draws(db, &mut out);
        self.cargo_draws(db, &mut out);
        self.group_draws(db, &mut out);
        let mut tip = self.button_at(self.pointer.0, self.pointer.1).filter(|b| !b.tip.is_empty() && self.replay.is_none()).cloned();
        if self.replay.is_some() {
            if let Some(t) = crate::replaypanel::ReplayPanel::tip(&self.tree, db, self.pointer.0, self.pointer.1) {
                tip = Some(crate::card::Button::text_only(t, String::new()));
            }
        }
        // The idle worker button's tooltip (CPeonBar: IDLE_PEON and IDLE_PEON_DESC).
        if tip.is_none() && !self.idle_workers.is_empty() && self.idle_worker_rect().contains(self.pointer.0, self.pointer.1) {
            tip = Some(crate::card::Button::text_only(db.string("IDLE_PEON"), db.string("IDLE_PEON_DESC")));
        }
        // An inventory item's, then the rest of the interface's.
        if tip.is_none() {
            tip = self.inventory_at(self.pointer.0, self.pointer.1).and_then(|s| self.inventory_tips.get(s).cloned().flatten());
        }
        if tip.is_none() {
            tip = self.hover_tips.iter().find(|(r, _)| r.contains(self.pointer.0, self.pointer.1)).map(|(_, b)| b.clone());
        }
        if tip.is_none() {
            tip = (0..self.hero_tips.len()).find(|&k| self.hero_button_rect(k).contains(self.pointer.0, self.pointer.1)).map(|k| self.hero_tips[k].clone());
        }
        if tip.is_none() && self.info.is_some_and(|i| self.tree.frames[i].visible) {
            tip = self.info_tips.iter().find(|(f, _)| self.tree.get(f).is_some_and(|f| self.tree.frames[f].visible && self.tree.frames[f].rect.is_some_and(|r| r.contains(self.pointer.0, self.pointer.1)))).map(|(_, b)| b.clone());
        }
        if let Some(b) = tip {
            self.tooltip_draws(db, &b, &mut out);
        }
        // CBuffBar: 0.12 x 0.015 with its lower right 0.003 left of and 0.005687 above
        // the unit detail's, eight 0.015 icons in a row, and COLON_STATUS 0.01 left of it
        // (InfoPanelTextFont 0.0085, white, shadowed).
        if let (Some(icons), Some(r)) = (&self.buff_icons, self.info.filter(|&i| self.tree.frames[i].visible).and_then(|i| self.tree.frames[i].rect)) {
            let (x1, y0) = (r.x1 - 0.003, r.y0 + 0.005687);
            let x0 = x1 - 0.12;
            for (k, icon) in icons.iter().enumerate() {
                let rect = w3_ui::Rect { x0: x0 + 0.015 * k as f32, x1: x0 + 0.015 * (k + 1) as f32, y0, y1: y0 + 0.015 };
                out.push(Draw::Image { texture: icon.clone(), rect, uv: [0.0, 0.0, 1.0, 1.0], color: [1.0; 4], blend: w3_ui::Blend::Alpha, rotated: false });
            }
            let rect = w3_ui::Rect { x0: x0 - 0.01 - 0.1, x1: x0 - 0.01, y0, y1: y0 + 0.015 };
            out.push(Draw::Text { font: db.decorate("InfoPanelTextFont"), size: 0.0085, rect, text: db.string("COLON_STATUS"), color: [1.0; 4], justify_h: w3_ui::Justify::End, justify_v: w3_ui::Justify::Middle, shadow: Some(([0.0, 0.0, 0.0, 1.0], 0.002, -0.002)) });
        }
        for (s, slot) in self.inventory.iter().enumerate() {
            let Some((icon, charges)) = slot else { continue };
            let r = self.inventory_rect(s);
            out.push(Draw::Image { texture: icon.clone(), rect: r, uv: [0.0, 0.0, 1.0, 1.0], color: [1.0; 4], blend: w3_ui::Blend::Alpha, rotated: false });
            if *charges > 0 {
                Self::number_overlay(db, r, *charges, &mut out);
            }
        }
        if let Some(b) = &self.boards {
            b.draws(db, &mut out);
        }
        if let Some(d) = &self.dialog {
            out.extend(d.tree.draw(db));
        }
        if let Some(d) = &self.alliance {
            d.draws(db, &mut out);
        }
        if let Some(d) = &self.chat_dialog {
            d.draws(db, &mut out);
        }
        if let Some(d) = &self.net_dialog {
            d.draws(db, &mut out);
        }
        if let Some(q) = &self.quests {
            out.extend(q.draws(db));
        }
        if let Some(l) = &self.log_view {
            out.extend(l.draws(db));
        }
        if let Some(m) = &self.menu {
            out.extend(m.draw(db));
            if let Some(p) = &self.save_panel {
                p.draws(db, &mut out);
            }
        }
        if let Some((hp, color, mana)) = &self.portrait_stats {
            // Hit points and mana: 0.078125 x 0.011875 at (0.214375, 0.0283) and
            // (0.214375, 0.01375), MasterFont at [FontHeights] PortraitStats.
            let root = self.tree.frames[0].rect.unwrap_or_default();
            let size = db.misc_ui.get_str("FontHeights", "PortraitStats").and_then(|v| v.trim().parse().ok()).unwrap_or(0.011);
            let font = db.decorate("MasterFont");
            let mut text = |t: &str, y: f32, c: [f32; 4]| {
                let rect = w3_ui::Rect { x0: root.x0 + 0.214375, x1: root.x0 + 0.214375 + 0.078125, y1: root.y0 + y, y0: root.y0 + y - 0.011875 };
                out.push(Draw::Text { font: font.clone(), size, rect, text: t.to_string(), color: c, justify_h: w3_ui::Justify::Middle, justify_v: w3_ui::Justify::Middle, shadow: Some(([0.0, 0.0, 0.0, 0.93], -0.001, -0.001)) });
            };
            text(hp, 0.0283, *color);
            if let Some(m) = mana {
                text(m, 0.01375, [1.0; 4]);
            }
        }
        if self.minimap_area.is_some() {
            let r = self.minimap_rect();
            if self.minimap_terrain {
                let image = if self.minimap_blighted { MINIMAP_TERRAIN } else { "war3mapMap.blp" };
                out.push(Draw::Image { texture: image.into(), rect: r, uv: [0.0, 0.0, 1.0, 1.0], color: [1.0; 4], blend: w3_ui::Blend::Alpha, rotated: false });
            } else {
                out.push(Draw::Image { texture: String::new(), rect: r, uv: [0.0, 0.0, 1.0, 1.0], color: [0.0, 0.0, 0.0, 1.0], blend: w3_ui::Blend::Alpha, rotated: false });
            }
            if self.minimap_fog {
                out.push(Draw::Image { texture: MINIMAP_FOG.into(), rect: r, uv: [0.0, 0.0, 1.0, 1.0], color: [1.0; 4], blend: w3_ui::Blend::Alpha, rotated: false });
            }
            // The map's area outlined in MinimapColorBorder, two of the 256 texels wide
            // (measured from the original's minimap).
            if let (Some(a), Some((x0, y0)), Some((x1, y1))) = (self.minimap_area, self.minimap_area.and_then(|a| self.world_to_minimap(a.x0, a.y0)), self.minimap_area.and_then(|a| self.world_to_minimap(a.x1, a.y1))) {
                let _ = a;
                let (tx, ty) = (2.0 / 256.0 * r.w(), 2.0 / 256.0 * r.h());
                let c = self.minimap_border;
                for e in [
                    w3_ui::Rect { x0, x1: x0 + tx, y0, y1 },
                    w3_ui::Rect { x0: x1 - tx, x1, y0, y1 },
                    w3_ui::Rect { x0, x1, y0, y1: y0 + ty },
                    w3_ui::Rect { x0, x1, y0: y1 - ty, y1 },
                ] {
                    out.push(Draw::Image { texture: String::new(), rect: e, uv: [0.0, 0.0, 1.0, 1.0], color: c, blend: w3_ui::Blend::Alpha, rotated: false });
                }
            }
            // CMinimap's icons: MinimapResourceTexture for mines and MinimapNeutralTexture
            // for neutral buildings, 16 pixels of the 256-pixel image, centered.
            for (k, &(kind, x, y)) in self.minimap_icons.iter().enumerate() {
                if !self.minimap_icons_shown.get(k).copied().unwrap_or(true) {
                    continue;
                }
                let key = match kind {
                    0 => "MinimapResourceTexture",
                    1 => "MinimapNeutralTexture",
                    _ => continue,
                };
                let (cx, cy) = (r.x0 + x / 256.0 * (r.x1 - r.x0), r.y1 - y / 256.0 * (r.y1 - r.y0));
                let (hw, hh) = (8.0 / 256.0 * (r.x1 - r.x0), 8.0 / 256.0 * (r.y1 - r.y0));
                out.push(Draw::Image { texture: db.decorate(key), rect: w3_ui::Rect { x0: cx - hw, x1: cx + hw, y0: cy - hh, y1: cy + hh }, uv: [0.0, 0.0, 1.0, 1.0], color: [1.0; 4], blend: w3_ui::Blend::Alpha, rotated: false });
            }
            // Creep camps while the creep filter is on, as MinimapCreepCamp and
            // MinimapCreepCampTough draw them: a quad 0.012 (or the tough one's 0.016)
            // across of MinimapIconCreepLoc (or ...Loc2), additive, its alpha pulsing 1, 0.5,
            // 1 over two seconds (KMTA), in the camp's weak, middle, or tough color.
            if self.show_creeps {
                let t = (self.now * 1000.0).rem_euclid(2000.0);
                let pulse = if t < 960.0 { 1.0 - 0.5 * t / 960.0 } else { 0.5 + 0.5 * (t - 960.0) / 1040.0 };
                for &(x, y, color, scale) in &self.minimap_camps {
                    if let Some((ux, uy)) = self.world_to_minimap(x, y) {
                        let tough = scale < 0.0;
                        let (tex, half) = if tough { ("UI\\Minimap\\MinimapIconCreepLoc2.blp", 0.008) } else { ("UI\\Minimap\\MinimapIconCreepLoc.blp", 0.006 * scale) };
                        let c = [color[0], color[1], color[2], color[3] * pulse];
                        out.push(Draw::Image { texture: tex.into(), rect: w3_ui::Rect { x0: ux - half, x1: ux + half, y0: uy - half, y1: uy + half }, uv: [0.0, 0.0, 1.0, 1.0], color: c, blend: w3_ui::Blend::Add, rotated: false });
                    }
                }
            }
            for &(x, y, color, size) in &self.minimap_dots {
                if let Some((ux, uy)) = self.world_to_minimap(x, y) {
                    let s = size / 2.0;
                    out.push(Draw::Image { texture: String::new(), rect: w3_ui::Rect { x0: ux - s, y0: uy - s, x1: ux + s, y1: uy + s }, uv: [0.0, 0.0, 1.0, 1.0], color, blend: w3_ui::Blend::Alpha, rotated: false });
                }
            }
            // Units with the alternate icon show it, as large as the other icons.
            if let Some(icon) = &self.alt_icon {
                let (hw, hh) = (8.0 / 256.0 * (r.x1 - r.x0), 8.0 / 256.0 * (r.y1 - r.y0));
                for &(x, y) in &self.minimap_alt {
                    if let Some((ux, uy)) = self.world_to_minimap(x, y) {
                        out.push(Draw::Image { texture: icon.clone(), rect: w3_ui::Rect { x0: ux - hw, x1: ux + hw, y0: uy - hh, y1: uy + hh }, uv: [0.0, 0.0, 1.0, 1.0], color: [1.0; 4], blend: w3_ui::Blend::Alpha, rotated: false });
                    }
                }
            }
            // The camera's view on the ground, as an outline.
            let pts: Vec<(f32, f32)> = self.camera_box.iter().filter_map(|&(x, y)| self.world_to_minimap(x, y)).collect();
            if pts.len() == 4 {
                for i in 0..4 {
                    let (a, b) = (pts[i], pts[(i + 1) % 4]);
                    let t = 0.0012;
                    let (x0, x1, y0, y1) = (a.0.min(b.0), a.0.max(b.0), a.1.min(b.1), a.1.max(b.1));
                    let rect = if (x1 - x0) > (y1 - y0) {
                        w3_ui::Rect { x0, x1, y0: (y0 + y1) / 2.0 - t / 2.0, y1: (y0 + y1) / 2.0 + t / 2.0 }
                    } else {
                        w3_ui::Rect { x0: (x0 + x1) / 2.0 - t / 2.0, x1: (x0 + x1) / 2.0 + t / 2.0, y0, y1 }
                    };
                    let clip = r;
                    let rect = w3_ui::Rect { x0: rect.x0.max(clip.x0), x1: rect.x1.min(clip.x1), y0: rect.y0.max(clip.y0), y1: rect.y1.min(clip.y1) };
                    if rect.x1 > rect.x0 && rect.y1 > rect.y0 {
                        out.push(Draw::Image { texture: String::new(), rect, uv: [0.0, 0.0, 1.0, 1.0], color: [1.0; 4], blend: w3_ui::Blend::Alpha, rotated: false });
                    }
                }
            }
        }
        if self.replay.is_some() {
            crate::replaypanel::ReplayPanel::draws(&self.tree, db, &mut out);
        }
        for b in self.card.iter().filter(|_| self.replay.is_none()) {
            let icon = if b.enabled && !self.paused { b.icon.clone() } else { crate::card::disabled_icon(&b.icon) };
            out.push(Draw::Image {
                texture: icon,
                rect: self.slot_rect(b.x, b.y),
                uv: [0.0, 0.0, 1.0, 1.0],
                color: [1.0; 4],
                blend: w3_ui::Blend::Alpha,
                rotated: false,
            });
            // CCommandButton's active highlight (FUN_6f3ad760): over the whole button,
            // added.
            if b.active {
                out.push(Draw::Image { texture: db.decorate("CommandButtonActiveHighlight"), rect: self.slot_rect(b.x, b.y), uv: [0.0, 0.0, 1.0, 1.0], color: [1.0; 4], blend: w3_ui::Blend::Add, rotated: false });
            }
            if let Some(n) = b.number {
                Self::number_overlay(db, self.slot_rect(b.x, b.y), n, &mut out);
            }
        }
        out
    }
}

/// Draws with their alpha scaled.
fn faded(mut draws: Vec<Draw>, alpha: f32) -> Vec<Draw> {
    for d in &mut draws {
        match d {
            Draw::Image { color, .. } | Draw::Text { color, .. } => color[3] *= alpha,
            #[allow(unreachable_patterns)]
            _ => {}
        }
    }
    draws
}

/// Fills a C format's %s, %d, and %% in order (the tooltip formats in GlobalStrings).
fn c_format(fmt: &str, args: &[&str]) -> String {
    let mut out = String::new();
    let mut args = args.iter();
    let mut chars = fmt.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('%') => out.push('%'),
            Some('s') | Some('d') | Some('u') => out.push_str(args.next().copied().unwrap_or("")),
            Some(o) => {
                out.push('%');
                out.push(o);
            }
            None => out.push('%'),
        }
    }
    out
}
