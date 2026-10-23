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
    pub x: f32,
    pub y: f32,
}

/// A handle-table entry.
#[derive(Clone, Debug)]
pub enum Obj {
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
    /// Objects the simulation does not model yet; the handle stays valid.
    Opaque(&'static str),
}
