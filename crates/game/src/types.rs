//! Script-visible game objects.

use std::collections::{HashMap, HashSet};

use w3_jass::FuncId;

use crate::objects::Id;

pub const PLAYER_COUNT: usize = 16;
pub const PLAYER_NEUTRAL_AGGRESSIVE: u8 = 12;
pub const PLAYER_NEUTRAL_PASSIVE: u8 = 15;

// common.j constant values.
pub mod player_state {
    pub const GAME_RESULT: i32 = 0;
    pub const RESOURCE_GOLD: i32 = 1;
    pub const RESOURCE_LUMBER: i32 = 2;
    pub const RESOURCE_HERO_TOKENS: i32 = 3;
    pub const RESOURCE_FOOD_CAP: i32 = 4;
    pub const RESOURCE_FOOD_USED: i32 = 5;
    pub const FOOD_CAP_CEILING: i32 = 6;
    pub const GIVES_BOUNTY: i32 = 7;
    pub const ALLIED_VICTORY: i32 = 8;
    pub const PLACED: i32 = 9;
    pub const OBSERVER_ON_DEATH: i32 = 10;
    pub const OBSERVER: i32 = 11;
    pub const UNFOLLOWABLE: i32 = 12;
    pub const GOLD_UPKEEP_RATE: i32 = 13;
    pub const LUMBER_UPKEEP_RATE: i32 = 14;
    pub const GOLD_GATHERED: i32 = 15;
    pub const LUMBER_GATHERED: i32 = 16;
    pub const NO_CREEP_SLEEP: i32 = 25;
}

pub mod unit_state {
    pub const LIFE: i32 = 0;
    pub const MAX_LIFE: i32 = 1;
    pub const MANA: i32 = 2;
    pub const MAX_MANA: i32 = 3;
}

pub mod map_control {
    pub const USER: i32 = 0;
    pub const COMPUTER: i32 = 1;
    pub const RESCUABLE: i32 = 2;
    pub const NEUTRAL: i32 = 3;
    pub const CREEP: i32 = 4;
    pub const NONE: i32 = 5;
}

pub mod slot_state {
    pub const EMPTY: i32 = 0;
    pub const PLAYING: i32 = 1;
    pub const LEFT: i32 = 2;
}

pub const ALLIANCE_TYPES: usize = 10;

#[derive(Clone, Debug)]
pub struct Player {
    pub id: u8,
    pub handle: u32,
    pub name: String,
    pub race: i32,
    pub race_preference: i32,
    pub race_selectable: bool,
    pub color: i32,
    pub controller: i32,
    pub slot_state: i32,
    pub start_location: i32,
    pub team: i32,
    pub handicap: f32,
    pub handicap_xp: f32,
    pub on_score_screen: bool,
    /// [other player][alliance type]
    pub alliances: [[bool; ALLIANCE_TYPES]; PLAYER_COUNT],
    pub state: HashMap<i32, i32>,
    pub tech_max: HashMap<Id, i32>,
    pub tech_researched: HashMap<Id, i32>,
    pub tax_rates: HashMap<(u8, i32), i32>,
    pub ai_script: Option<String>,
}

impl Player {
    pub fn new(id: u8, handle: u32) -> Player {
        Player {
            id,
            handle,
            name: String::new(),
            race: 0,
            race_preference: 0,
            race_selectable: true,
            color: id as i32,
            controller: map_control::NONE,
            slot_state: slot_state::EMPTY,
            start_location: -1,
            team: 0,
            handicap: 1.0,
            handicap_xp: 1.0,
            on_score_screen: true,
            alliances: [[false; ALLIANCE_TYPES]; PLAYER_COUNT],
            state: HashMap::new(),
            tech_max: HashMap::new(),
            tech_researched: HashMap::new(),
            tax_rates: HashMap::new(),
            ai_script: None,
        }
    }
    pub fn get_state(&self, s: i32) -> i32 {
        self.state.get(&s).copied().unwrap_or(0)
    }
}

#[derive(Clone, Debug, Default)]
pub struct StartLocation {
    pub x: f32,
    pub y: f32,
    pub prio_count: i32,
    pub prios: HashMap<i32, (i32, i32)>,
}

#[derive(Clone, Debug, Default)]
pub struct Order {
    pub id: i32,
    pub target_x: f32,
    pub target_y: f32,
    pub target: u32,
}

#[derive(Clone, Debug)]
pub struct Unit {
    pub handle: u32,
    pub type_id: Id,
    pub owner: u8,
    pub x: f32,
    pub y: f32,
    pub fly_height: f32,
    /// Degrees.
    pub facing: f32,
    pub life: f32,
    pub max_life: f32,
    pub mana: f32,
    pub max_mana: f32,
    pub color: i32,
    pub scale: f32,
    pub vertex_color: [u8; 4],
    pub user_data: i32,
    pub acquire_range: f32,
    pub move_speed: f32,
    pub turn_speed: f32,
    pub prop_window: f32,
    pub paused: bool,
    pub hidden: bool,
    pub invulnerable: bool,
    pub dead: bool,
    pub level: i32,
    pub hero_xp: i32,
    pub resource: i32,
    pub abilities: Vec<(Id, i32)>,
    pub order: Order,
    pub animation: Option<String>,
    pub time_scale: f32,
    pub inventory: [u32; 6],
    pub rally: Option<(f32, f32)>,
    pub explode_on_death: bool,
    pub is_building: bool,
    pub is_hero: bool,
    pub collision: f32,
    pub action: Action,
    pub path: Vec<(f32, f32)>,
    pub repath_timer: f32,
    pub cooldown: f32,
    pub swing: Option<Swing>,
    pub acquire_timer: f32,
    /// Animation the presentation layer should play; `anim_serial` changes whenever a new
    /// animation starts.
    pub anim: String,
    pub anim_serial: u32,
    pub death_time: f32,
    pub pathing_cells: Vec<(i32, i32)>,
    pub carry_gold: i32,
    pub carry_lumber: i32,
    pub work_timer: f32,
    /// Resource (mine or tree) the unit last harvested, for resuming after a return.
    pub harvest_source: u32,
    /// Units inside a gold mine (for mines) or the mine a worker is inside.
    pub inside: u32,
    /// Units, upgrades, or research queued at a structure, and progress on the first.
    pub queue: Vec<Id>,
    pub queue_progress: f32,
    /// Seconds of construction remaining, while a structure is being built.
    pub constructing: Option<f32>,
    pub build_time: f32,
    /// The structure a worker is building.
    pub building: u32,
    /// Hero attributes (base plus per-level growth) and unspent skill points.
    pub str_: f32,
    pub agi: f32,
    pub int: f32,
    pub skill_points: i32,
    pub regen_timer: f32,
    /// Remaining cooldown per ability.
    pub cooldowns: HashMap<Id, f32>,
    pub autocast: Vec<Id>,
    pub buffs: Vec<Buff>,
    /// A spell being cast: time since the cast began and whether its effect has fired.
    pub casting: Option<(f32, bool)>,
    /// Summoned units expire when this reaches zero.
    pub timed_life: Option<f32>,
    /// What carried items add.
    pub item_bonus: ItemBonus,
    /// An altar's revival: the hero, seconds done, and seconds needed.
    pub revive: Option<(u32, f32, f32)>,
    /// A dead hero being revived.
    pub reviving: bool,
    pub channel: Option<Channel>,
    /// Reincarnation: seconds until a dead hero rises again.
    pub reincarnate: Option<f32>,
    /// A hero's name from its type's Propernames.
    pub proper_name: String,
}

/// Bonuses from held items' abilities.
#[derive(Clone, Copy, Debug, Default)]
pub struct ItemBonus {
    pub damage: f32,
    pub armor: f32,
    pub move_speed: f32,
    /// Fraction added to attack rate.
    pub attack_speed: f32,
    pub life: f32,
    pub mana: f32,
    /// Fraction of normal mana regeneration added.
    pub mana_regen: f32,
    pub str_: f32,
    pub agi: f32,
    pub int: f32,
}

/// An effect on a unit for a while: from spells, auras, and passives.
#[derive(Clone, Debug, Default)]
pub struct Buff {
    /// Buff type from AbilityBuffData (e.g. BHtb).
    pub id: Id,
    pub ability: Id,
    pub level: u32,
    pub caster: u32,
    pub remaining: f32,
    pub stun: bool,
    pub move_factor: f32,
    pub attack_factor: f32,
    pub armor: f32,
    pub damage_factor: f32,
    pub invulnerable: bool,
    /// Damage (negative heals) per second.
    pub dps: f32,
    pub mana_regen: f32,
    pub life_regen: f32,
    pub effect: u32,
    /// Flat damage added to attacks.
    pub damage_bonus: f32,
    /// Extra maximum life while the buff lasts (Avatar).
    pub life_bonus: f32,
    pub invisible: bool,
    pub magic_immune: bool,
    /// Banished: cannot attack or be attacked physically; takes extra spell damage.
    pub ethereal: bool,
    /// Asleep: stunned until damaged.
    pub sleep: bool,
    /// Fraction of attack damage regained as life (Vampiric Aura).
    pub life_steal: f32,
    /// Melee attackers take this damage (or this fraction when `return_pct`).
    pub return_damage: f32,
    pub return_pct: bool,
    /// Wind Walk's first strike bonus.
    pub backstab: f32,
}

/// A spell kept up by its caster: it ends when the time runs out or the caster stops.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Channel {
    pub ability: Id,
    pub level: u32,
    pub target: u32,
    pub x: f32,
    pub y: f32,
    pub elapsed: f32,
    pub duration: f32,
    /// Seconds between the spell's waves or ticks, and until the next.
    pub interval: f32,
    pub next: f32,
    pub waves: u32,
}

/// A spell's lasting effect on the ground (Flame Strike, Earthquake).
#[derive(Clone, Debug, Default)]
pub struct GroundEffect {
    pub caster: u32,
    pub owner: u8,
    pub ability: Id,
    pub level: u32,
    pub x: f32,
    pub y: f32,
    pub elapsed: f32,
    pub duration: f32,
    pub interval: f32,
    pub next: f32,
    pub effect: u32,
}

/// What a unit is doing.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Action {
    #[default]
    Idle,
    Move { x: f32, y: f32 },
    Attack { target: u32 },
    AttackMove { x: f32, y: f32 },
    Hold,
    Patrol { a: (f32, f32), b: (f32, f32) },
    Follow { target: u32 },
    Harvest { target: u32 },
    ReturnResources,
    Build { id: Id, x: f32, y: f32 },
    Cast { ability: Id, target: u32, x: f32, y: f32 },
    /// Walk to an item and pick it up.
    PickUp { item: u32 },
    /// Carry an item to a point and drop it there.
    DropItem { item: u32, x: f32, y: f32 },
    /// A worker constructing a structure.
    Construct { target: u32 },
}

/// An attack in progress: damage lands at the damage point, then the backswing plays.
#[derive(Clone, Debug)]
pub struct Swing {
    pub target: u32,
    pub elapsed: f32,
    pub done: bool,
}

#[derive(Clone, Debug)]
pub struct Destructable {
    pub handle: u32,
    pub type_id: Id,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub facing: f32,
    pub scale: f32,
    pub variation: u32,
    pub life: f32,
    pub max_life: f32,
    pub invulnerable: bool,
    pub visible: bool,
    pub animation: Option<String>,
    pub pathing_cells: Vec<(i32, i32)>,
}

#[derive(Clone, Debug)]
pub struct Item {
    pub handle: u32,
    pub type_id: Id,
    pub x: f32,
    pub y: f32,
    pub life: f32,
    pub charges: i32,
    pub player: u8,
    pub owner_unit: u32,
    pub visible: bool,
    pub invulnerable: bool,
    pub user_data: i32,
    pub pawnable: bool,
    pub drop_on_death: bool,
    pub droppable: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Timer {
    pub remaining: f32,
    pub timeout: f32,
    pub periodic: bool,
    pub running: bool,
    pub callback: Option<FuncId>,
}

#[derive(Clone, Debug)]
pub struct Trigger {
    pub enabled: bool,
    pub wait_on_sleeps: bool,
    pub events: Vec<u32>,
    pub conditions: Vec<u32>,
    pub actions: Vec<u32>,
    pub eval_count: i32,
    pub exec_count: i32,
}

impl Default for Trigger {
    fn default() -> Self {
        Trigger {
            enabled: true,
            wait_on_sleeps: true,
            events: Vec::new(),
            conditions: Vec::new(),
            actions: Vec::new(),
            eval_count: 0,
            exec_count: 0,
        }
    }
}

#[derive(Clone, Debug)]
pub enum BoolExpr {
    Func(FuncId),
    And(u32, u32),
    Or(u32, u32),
    Not(u32),
}

#[derive(Clone, Debug, Default)]
pub struct Group {
    pub units: Vec<u32>,
}

#[derive(Clone, Debug, Default)]
pub struct Force {
    pub players: u32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Rect {
    pub min_x: f32,
    pub min_y: f32,
    pub max_x: f32,
    pub max_y: f32,
}

impl Rect {
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.min_x && x <= self.max_x && y >= self.min_y && y <= self.max_y
    }
}

/// Regions are sets of 32x32 cells.
#[derive(Clone, Debug, Default)]
pub struct Region {
    pub cells: HashSet<(i32, i32)>,
}

pub const REGION_CELL: f32 = 32.0;

impl Region {
    pub fn cell(x: f32, y: f32) -> (i32, i32) {
        ((x / REGION_CELL).floor() as i32, (y / REGION_CELL).floor() as i32)
    }
    pub fn contains(&self, x: f32, y: f32) -> bool {
        self.cells.contains(&Region::cell(x, y))
    }
}

#[derive(Clone, Debug)]
pub struct Effect {
    pub model: String,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    /// Attached to a unit or other widget at an attachment point.
    pub target: Option<(u32, String)>,
    pub destroyed: bool,
}

/// A script dialog (DialogCreate): a message over a column of buttons.
#[derive(Clone, Debug, Default)]
pub struct Dialog {
    pub message: String,
    pub buttons: Vec<u32>,
    /// Players it is shown to, as a bit mask.
    pub shown: u32,
}

#[derive(Clone, Debug, Default)]
pub struct DialogButton {
    pub dialog: u32,
    pub text: String,
    pub hotkey: i32,
    /// DialogAddQuitButton: clicking it ends the game, optionally to the score screen.
    pub quit: Option<bool>,
}

#[derive(Clone, Debug, Default)]
pub struct Sound {
    pub path: String,
    pub looping: bool,
    pub is_3d: bool,
    pub stop_when_out_of_range: bool,
    pub fade_in: i32,
    pub fade_out: i32,
    pub eax: String,
    pub label: Option<String>,
    pub volume: i32,
    pub pitch: f32,
    pub position: Option<(f32, f32, f32)>,
    pub duration_ms: i32,
    pub channel: i32,
    pub min_dist: f32,
    pub max_dist: f32,
    pub cutoff: f32,
    pub playing: bool,
}

#[derive(Clone, Debug, Default)]
pub struct TextTag {
    pub text: String,
    pub height: f32,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub unit: Option<(u32, f32)>,
    pub color: [u8; 4],
    pub velocity: (f32, f32),
    pub visible: bool,
    pub suspended: bool,
    pub permanent: bool,
    pub age: f32,
    pub lifespan: f32,
    pub fadepoint: f32,
}

/// What a trigger event registration listens for.
#[derive(Clone, Debug)]
pub enum EventKind {
    TimerExpire(u32),
    GameState { state: i32, op: i32, value: f32 },
    Game(i32),
    Player { player: u8, event: i32 },
    PlayerChat { player: u8, text: String, exact: bool },
    PlayerUnit { player: u8, event: i32, filter: Option<u32> },
    Unit { unit: u32, event: i32 },
    UnitState { unit: u32, state: i32, op: i32, value: f32 },
    UnitInRange { unit: u32, range: f32, filter: Option<u32> },
    EnterRegion { region: u32, filter: Option<u32> },
    LeaveRegion { region: u32, filter: Option<u32> },
    Variable { name: String, op: i32, value: f32 },
    Dialog(u32),
    DialogButton(u32),
    Death(u32),
    Trackable { trackable: u32, hit: bool },
    PlayerState { player: u8, state: i32, op: i32, value: f32 },
    Filter { event: i32 },
}

#[derive(Clone, Debug)]
pub struct EventReg {
    pub trigger: u32,
    pub kind: EventKind,
}

/// Values the event-response natives (GetTriggerUnit and friends) read.
#[derive(Clone, Debug, Default)]
pub struct EventCtx {
    pub trigger: u32,
    pub event_id: i32,
    pub unit: u32,
    pub target_unit: u32,
    pub player: u8,
    pub has_player: bool,
    pub timer: u32,
    pub region: u32,
    pub destructable: u32,
    pub item: u32,
    pub widget: u32,
    pub order_id: i32,
    pub order_x: f32,
    pub order_y: f32,
    pub spell_ability: i32,
    pub chat: String,
    pub matched_chat: String,
    pub dialog: u32,
    pub button: u32,
    pub damage: f32,
    pub other_unit: u32,
    pub research: i32,
    pub train_unit_type: i32,
    pub state: i32,
    pub trackable: u32,
}

/// Enumeration callback context (GetEnumUnit, GetFilterUnit, ...).
#[derive(Clone, Copy, Debug, Default)]
pub struct EnumCtx {
    pub enum_unit: u32,
    pub filter_unit: u32,
    pub enum_player: u8,
    pub filter_player: u8,
    pub enum_destructable: u32,
    pub filter_destructable: u32,
    pub enum_item: u32,
    pub filter_item: u32,
}

#[derive(Clone, Debug, Default)]
pub struct Hashtable {
    pub ints: HashMap<(i32, i32), i32>,
    pub reals: HashMap<(i32, i32), f32>,
    pub bools: HashMap<(i32, i32), bool>,
    pub strings: HashMap<(i32, i32), String>,
    pub handles: HashMap<(i32, i32), u32>,
}

#[derive(Clone, Debug, Default)]
pub struct GameCache {
    pub name: String,
    pub ints: HashMap<(String, String), i32>,
    pub reals: HashMap<(String, String), f32>,
    pub bools: HashMap<(String, String), bool>,
    pub strings: HashMap<(String, String), String>,
}

#[derive(Clone, Debug, Default)]
pub struct CameraSetup {
    pub fields: HashMap<i32, f32>,
    /// Each field's transition time (CameraSetupSetField's duration).
    pub durations: HashMap<i32, f32>,
    pub x: f32,
    pub y: f32,
}

/// A transmission on the cinematic panel (SetCinematicScene).
#[derive(Clone, Debug, Default)]
pub struct CineScene {
    pub unit_type: crate::objects::Id,
    pub color: i32,
    pub speaker: String,
    pub text: String,
    /// Game time when the scene and its voice end.
    pub ends: f32,
    pub voice_ends: f32,
}

/// A screen overlay that changes over time (the SetCineFilter natives).
#[derive(Clone, Debug)]
pub struct CineFilter {
    pub texture: String,
    pub blend: i32,
    pub tex_flags: i32,
    pub start_uv: [f32; 4],
    pub end_uv: [f32; 4],
    /// Red, green, blue, and alpha from 0 to 255.
    pub start_color: [i32; 4],
    pub end_color: [i32; 4],
    pub duration: f32,
    /// Game time the filter was displayed, while it is.
    pub shown: Option<f32>,
}

impl Default for CineFilter {
    fn default() -> Self {
        CineFilter { texture: String::new(), blend: 1, tex_flags: 0, start_uv: [0.0, 0.0, 1.0, 1.0], end_uv: [0.0, 0.0, 1.0, 1.0], start_color: [255; 4], end_color: [255; 4], duration: 0.0, shown: None }
    }
}

/// Cinematic presentation state for the local player.
#[derive(Clone, Debug)]
pub struct Cinematic {
    /// ShowInterface: the console is shown (false shows the cinematic panel), and the
    /// time the last change started and how long it fades.
    pub interface: bool,
    pub interface_changed: f32,
    pub interface_fade: f32,
    pub user_control: bool,
    pub user_ui: bool,
    pub scene: Option<CineScene>,
    pub subtitles: bool,
    pub filter: CineFilter,
}

impl Default for Cinematic {
    fn default() -> Self {
        Cinematic { interface: true, interface_changed: 0.0, interface_fade: 0.0, user_control: true, user_ui: true, scene: None, subtitles: false, filter: CineFilter::default() }
    }
}

/// A quest in the quest log (CreateQuest makes one required, discovered, and
/// enabled).
#[derive(Clone, Debug)]
pub struct Quest {
    pub title: String,
    pub description: String,
    pub icon: String,
    pub required: bool,
    pub completed: bool,
    pub failed: bool,
    pub discovered: bool,
    pub enabled: bool,
    pub items: Vec<u32>,
}

impl Default for Quest {
    fn default() -> Self {
        Quest { title: String::new(), description: String::new(), icon: String::new(), required: true, completed: false, failed: false, discovered: true, enabled: true, items: Vec::new() }
    }
}

/// One requirement line of a quest.
#[derive(Clone, Debug, Default)]
pub struct QuestItem {
    pub quest: u32,
    pub description: String,
    pub completed: bool,
}

/// A handle-table entry.
#[derive(Clone, Debug)]
pub enum Obj {
    Quest(Quest),
    QuestItem(QuestItem),
    DefeatCondition(String),
    Player(u8),
    Unit(Box<Unit>),
    Destructable(Box<Destructable>),
    Item(Box<Item>),
    Timer(Timer),
    Trigger(Trigger),
    TriggerCondition { expr: u32 },
    TriggerAction { func: FuncId },
    Event(EventReg),
    BoolExpr(BoolExpr),
    Group(Group),
    Force(Force),
    Rect(Rect),
    Region(Region),
    Location(f32, f32),
    Effect(Effect),
    Sound(Box<Sound>),
    TextTag(Box<TextTag>),
    Hashtable(Box<Hashtable>),
    GameCache(Box<GameCache>),
    CameraSetup(Box<CameraSetup>),
    FogModifier(crate::fog::FogModifier),
    Dialog(Dialog),
    DialogButton(DialogButton),
    /// Objects the simulation does not model yet; the handle stays valid.
    Opaque(&'static str),
}
