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
    /// Scene time, for message expiry.
    pub now: f32,
    /// WorldFrameUnitMessage: trigger text, oldest first.
    pub unit_messages: Vec<Message>,
    /// The unit message frame's placement from the last DisplayTextToPlayer x and y.
    pub unit_message_offset: (f32, f32),
    /// WorldFrameMessage: the error line above the console.
    pub error_message: Option<Message>,
    pub dialog: Option<DialogView>,
    /// The game menu (F10), built by CEscMenu: EscMenuBackdrop centered 0.06 above the
    /// middle, holding EscMenuMainPanel.
    pub menu: Option<FrameTree>,
    /// The selected unit's inventory: icon and charges per slot it has.
    pub inventory: Vec<Option<(String, i32)>>,
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
    pub cinematic: bool,
    filter: Option<(String, [f32; 4], [f32; 4], bool)>,
    pub minimap_area: Option<MinimapArea>,
    pub minimap_dots: Vec<(f32, f32, [f32; 4], f32)>,
    /// war3map.mmp: icons stamped on the minimap image (kind, x and y on its 256-pixel
    /// grid): 0 gold mine, 1 neutral building, 2 start location.
    pub minimap_icons: Vec<(i32, f32, f32)>,
    /// Whether the "@minimapfog" overlay texture is current.
    pub minimap_fog: bool,
    pub camera_box: Vec<(f32, f32)>,
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
            // The building detail shares the unit detail's place.
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
            minimap_fog: false,
            minimap_icons: Vec::new(),
            now: 0.0,
            unit_messages: Vec::new(),
            unit_message_offset: (0.0, 0.0),
            error_message: None,
            dialog: None,
            menu: None,
            inventory: Vec::new(),
            level_bar: None,
            buff_icons: None,
            building,
            production: None,
            group: Vec::new(),
            pointer: (-1.0, -1.0),
            cine: None,
            heroes: Vec::new(),
            cinematic: false,
            filter: None,
            camera_box: Vec::new(),
            card: Vec::new(),
            page: crate::card::Page::Main,
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
        // Food provided, beside the armor (or the attack when there is one).
        if s.hero.is_none() && s.food_made > 0 {
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
                self.group.push(GroupSlot { unit: h, icon, life: u.life / u.max_life.max(1.0), mana, active: k == 0 });
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
        self.cinematic = !c.interface;
        self.filter = c.filter.shown.and_then(|shown| {
            let f = &c.filter;
            let t = if f.duration > 0.0 { ((world.time - shown) / f.duration).clamp(0.0, 1.0) } else { 1.0 };
            let lerp = |a: f32, b: f32| a + (b - a) * t;
            let col: Vec<f32> = (0..4).map(|i| lerp(f.start_color[i] as f32, f.end_color[i] as f32) / 255.0).collect();
            let uv: Vec<f32> = (0..4).map(|i| lerp(f.start_uv[i], f.end_uv[i])).collect();
            (!f.texture.is_empty()).then(|| (f.texture.clone(), [uv[0], uv[1], uv[2], uv[3]], [col[0], col[1], col[2], col[3]], f.blend == 2))
        });
        if !self.cinematic {
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
        let line_h = desc_h * 1.2;
        let costs: Vec<(usize, i32)> = b.costs.iter().copied().enumerate().filter(|&(_, c)| c != 0).collect();
        let cost_row = if costs.is_empty() { 0.0 } else { 0.013375 };
        let uber_h = if lines.is_empty() { 0.0 } else { lines.len() as f32 * line_h + 0.005 };
        let w = name_w.max(0.21) + 0.01;
        let h = name_h + cost_row + uber_h + 0.01;
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
        if !lines.is_empty() {
            y -= 0.005;
            text(out, &lines.join("\n"), x0, y, desc_h, 0.21 + 0.005, [1.0; 4]);
        }
    }

    /// Follows the local player's heroes for the hero bar, in the order they came.
    pub fn sync_heroes(&mut self, world: &World) {
        self.heroes = world
            .unit_order
            .iter()
            .filter_map(|&h| world.unit(h).map(|u| (h, u)))
            .filter(|(_, u)| u.is_hero && u.owner == world.local_player && !u.hidden)
            .take(7)
            .map(|(h, u)| {
                let icon = world.objects.units.get(u.type_id).map(|o| o.str("Art")).unwrap_or_default();
                let icon = if u.dead { crate::card::disabled_icon(&icon) } else { icon };
                (h, icon, if u.dead { 0.0 } else { u.life / u.max_life.max(1.0) }, (u.max_mana > 0.0).then(|| u.mana / u.max_mana), u.skill_points > 0)
            })
            .collect();
    }

    /// The hero bar's button `k`: CHeroBar's 7-row grid (0.038 x 0.35825) from 0.00375
    /// right of and 0.02625 below the interface's top left.
    pub fn hero_button_rect(&self, k: usize) -> w3_ui::Rect {
        let root = self.tree.frames[0].rect.unwrap_or_default();
        let cell = 0.35825 / 7.0;
        let (x0, y1) = (root.x0 + 0.00375, root.y1 - 0.02625 - cell * k as f32);
        w3_ui::Rect { x0, x1: x0 + 0.038, y0: y1 - 0.038, y1 }
    }

    pub fn hero_at(&self, x: f32, y: f32) -> Option<u32> {
        (0..self.heroes.len()).find(|&k| self.hero_button_rect(k).contains(x, y)).map(|k| self.heroes[k].0)
    }

    /// Each hero's icon with its life and mana bars (CStatBar's small kinds, 0.003125
    /// tall, 0.00125 apart).
    fn hero_bar_draws(&self, db: &FrameDb, out: &mut Vec<Draw>) {
        let image = |texture: String, rect: w3_ui::Rect, color: [f32; 4]| Draw::Image { texture, rect, uv: [0.0, 0.0, 1.0, 1.0], color, blend: w3_ui::Blend::Alpha, rotated: false };
        for (k, (_, icon, life, mana, _)) in self.heroes.iter().enumerate() {
            let r = self.hero_button_rect(k);
            out.push(image(icon.clone(), r, [1.0; 4]));
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
            out.push(image(s.icon.clone(), icon, [1.0; 4], w3_ui::Blend::Alpha));
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

    /// The building detail while a structure produces (FUN_6f3ccfd0's modes): the
    /// action label, the queue backdrop when training or reviving, and the queue.
    fn show_production(&mut self, db: &FrameDb, world: &World, h: u32, name: &str) {
        use w3_game::info::ProductionKind as K;
        self.production = world.production(h);
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
        if let Some(i) = self.info {
            self.tree.frames[i].visible = u.is_some();
        }
        if let Some(b) = self.building {
            self.tree.frames[b].visible = false;
        }
        self.production = None;
        let Some(u) = u else {
            self.portrait_stats = None;
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
        // Buff icons for units (structures have no status bar), at most the bar's eight.
        self.buff_icons = (!u.is_building).then(|| {
            let mut icons: Vec<String> = Vec::new();
            for b in &u.buffs {
                let art = world.objects.buffs.get(b.id).map(|o| o.str("Buffart")).unwrap_or_default();
                if !art.is_empty() && !icons.contains(&art) {
                    icons.push(art);
                }
            }
            icons.truncate(8);
            icons
        });
        // The hero level bar: experience through the current level.
        self.level_bar = u.is_hero.then(|| {
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
        self.tree.set_enabled("UpperButtonBarMenuButton", true);
        self.tree.set_enabled("UpperButtonBarAlliesButton", false);
        self.tree.set_enabled("UpperButtonBarQuestsButton", !world.quests.is_empty());
        if let Some(f) = self.tree.get("UpperButtonBarChatButton") {
            self.tree.frames[f].text = Some(db.string("KEY_LOG"));
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

    /// The card button at an interface position.
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
    }

    /// Shows an error line for 10 seconds, replacing the previous one (FUN_6f36de80).
    pub fn error(&mut self, text: &str) {
        self.error_message = Some(Message { text: text.to_string(), until: self.now + 10.0 });
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
        tree.set_point("ScriptDialog", w3_ui::FramePoint::Center, "", w3_ui::FramePoint::Center, 0.0, 0.0);
        tree.bind_points();
        tree.layout();
        self.dialog = Some(DialogView { dialog: h, content, tree, buttons });
    }

    pub fn open_menu(&mut self, db: &FrameDb, aspect: f32) {
        let mut tree = FrameTree::new(aspect);
        let Some(_) = tree.create(db, "EscMenuBackdrop", 0) else { return };
        if let Some(b) = tree.get("EscMenuBackdrop") {
            tree.create(db, "EscMenuMainPanel", b);
        }
        use w3_ui::FramePoint as P;
        tree.set_point("EscMenuBackdrop", P::Center, "", P::Center, 0.0, 0.06);
        Self::menu_panel(&mut tree, "MainPanel");
        tree.bind_points();
        tree.layout();
        self.menu = Some(tree);
    }

    /// Shows one of the menu's panels.
    pub fn menu_panel(tree: &mut FrameTree, panel: &str) {
        for p in ["MainPanel", "EndGamePanel", "ConfirmQuitPanel", "HelpPanel", "TipsPanel", "EscMenuOptionsPanel", "EscMenuSaveGamePanel"] {
            tree.set_visible(p, p == panel);
        }
    }

    /// Drops expired messages.
    pub fn tick(&mut self, now: f32) {
        self.now = now;
        self.unit_messages.retain(|m| m.until > now);
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
        if let Some(m) = &self.error_message {
            let size = height("WorldFrameMessage");
            out.push(Draw::Text { font: font.clone(), size, rect: msg, text: m.text.clone(), color: [1.0; 4], justify_h: w3_ui::Justify::Middle, justify_v: w3_ui::Justify::End, shadow });
        }
        // WorldFrameUnitMessage: 0.5 x 0.25 with its bottom center on the message frame's
        // top, 0.098125 left of center and 0.0005 up at offset (0, 0); offsets of 1 move
        // it to x 0.8 and y 0.452 (FUN_6f374d20).
        if !self.unit_messages.is_empty() {
            let size = height("WorldFrameUnitMessage");
            let (ox, oy) = self.unit_message_offset;
            let ax0 = cx - 0.098125;
            let ay0 = msg.y1 + 0.0005;
            let ax = ax0 + (root.x0 + 0.8 - ax0) * ox;
            let ay = ay0 + (root.y0 + 0.452 - ay0) * oy;
            let rect = w3_ui::Rect { x0: ax - 0.25, x1: ax + 0.25, y0: ay, y1: ay + 0.25 };
            let text = self.unit_messages.iter().map(|m| m.text.replace("|n", "\n")).collect::<Vec<_>>().join("\n");
            out.push(Draw::Text { font, size, rect, text, color: [1.0; 4], justify_h: w3_ui::Justify::Start, justify_v: w3_ui::Justify::End, shadow });
        }
    }

    pub fn draws(&self, db: &FrameDb) -> Vec<Draw> {
        // While the interface is hidden only the cinematic panel, text messages, and the
        // filter show.
        if self.cinematic {
            let mut out = Vec::new();
            self.filter_draws(&mut out);
            if let Some(c) = &self.cine {
                out.extend(c.draw(db));
            }
            self.message_draws(db, &mut out);
            return out;
        }
        let mut out = self.tree.draw(db);
        self.filter_draws(&mut out);
        self.hero_bar_draws(db, &mut out);
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
        self.group_draws(db, &mut out);
        let tip = self.button_at(self.pointer.0, self.pointer.1).filter(|b| !b.tip.is_empty()).cloned();
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
                let font = db.decorate("MasterFont");
                let cr = w3_ui::Rect { x0: r.x1 - 0.012, x1: r.x1 - 0.001, y0: r.y0 + 0.001, y1: r.y0 + 0.011 };
                out.push(Draw::Text { font, size: 0.009, rect: cr, text: charges.to_string(), color: [1.0; 4], justify_h: w3_ui::Justify::End, justify_v: w3_ui::Justify::End, shadow: Some(([0.0, 0.0, 0.0, 1.0], 0.0005, -0.0005)) });
            }
        }
        if let Some(d) = &self.dialog {
            out.extend(d.tree.draw(db));
        }
        if let Some(m) = &self.menu {
            out.extend(m.draw(db));
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
            out.push(Draw::Image { texture: "war3mapMap.blp".into(), rect: r, uv: [0.0, 0.0, 1.0, 1.0], color: [1.0; 4], blend: w3_ui::Blend::Alpha, rotated: false });
            if self.minimap_fog {
                out.push(Draw::Image { texture: MINIMAP_FOG.into(), rect: r, uv: [0.0, 0.0, 1.0, 1.0], color: [1.0; 4], blend: w3_ui::Blend::Alpha, rotated: false });
            }
            // CMinimap's icons: MinimapResourceTexture for mines and MinimapNeutralTexture
            // for neutral buildings, 16 pixels of the 256-pixel image, centered.
            for &(kind, x, y) in &self.minimap_icons {
                let key = match kind {
                    0 => "MinimapResourceTexture",
                    1 => "MinimapNeutralTexture",
                    _ => continue,
                };
                let (cx, cy) = (r.x0 + x / 256.0 * (r.x1 - r.x0), r.y1 - y / 256.0 * (r.y1 - r.y0));
                let (hw, hh) = (8.0 / 256.0 * (r.x1 - r.x0), 8.0 / 256.0 * (r.y1 - r.y0));
                out.push(Draw::Image { texture: db.decorate(key), rect: w3_ui::Rect { x0: cx - hw, x1: cx + hw, y0: cy - hh, y1: cy + hh }, uv: [0.0, 0.0, 1.0, 1.0], color: [1.0; 4], blend: w3_ui::Blend::Alpha, rotated: false });
            }
            for &(x, y, color, size) in &self.minimap_dots {
                if let Some((ux, uy)) = self.world_to_minimap(x, y) {
                    let s = size / 2.0;
                    out.push(Draw::Image { texture: String::new(), rect: w3_ui::Rect { x0: ux - s, y0: uy - s, x1: ux + s, y1: uy + s }, uv: [0.0, 0.0, 1.0, 1.0], color, blend: w3_ui::Blend::Alpha, rotated: false });
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
