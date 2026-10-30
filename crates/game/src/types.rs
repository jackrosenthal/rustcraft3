//! Script-visible game objects.

use crate::collections::{HashMap, HashSet};

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

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
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
    /// The original's tech tree registrations, in the order made (`crate::techreg`).
    #[serde(default)]
    pub tech_regs: Vec<crate::techreg::TechReg>,
    /// Abilities SetPlayerAbilityAvailable turned off.
    pub abilities_unavailable: crate::collections::HashSet<Id>,
    /// What the score screen and GetPlayerScore report.
    #[serde(default)]
    pub score: crate::score::Score,
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
            // Each player on a team of its own unless set (measured: GetPlayerTeam is 1 for
            // Player(1) on a map whose script sets none).
            team: id as i32,
            handicap: 1.0,
            handicap_xp: 1.0,
            on_score_screen: true,
            alliances: [[false; ALLIANCE_TYPES]; PLAYER_COUNT],
            state: HashMap::default(),
            tech_max: HashMap::default(),
            tech_researched: HashMap::default(),
            tech_regs: Vec::new(),
            abilities_unavailable: Default::default(),
            score: Default::default(),
            tax_rates: HashMap::default(),
            ai_script: None,
        }
    }
    pub fn get_state(&self, s: i32) -> i32 {
        self.state.get(&s).copied().unwrap_or(0)
    }
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct StartLocation {
    pub x: f32,
    pub y: f32,
    pub prio_count: i32,
    pub prios: HashMap<i32, (i32, i32)>,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Order {
    pub id: i32,
    pub target_x: f32,
    pub target_y: f32,
    pub target: u32,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
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
    /// The facing the model shows and GetUnitFacing reads: it follows `facing` at a
    /// third of its rate (measured: a footman's turns 0.6 radians a tenth of a second
    /// where its facing for movement and attacks turns 0.6 a thirtieth).
    #[serde(default)]
    pub shown_facing: Option<f32>,
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
    /// The whole footprint (the pathing texture's blue) a structure stamps unbuildable.
    #[serde(default)]
    pub build_cells: Vec<(i32, i32)>,
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
    /// SuspendHeroXP: the hero gains no experience from kills.
    pub xp_suspended: bool,
    /// Whether the first queued item has started (its start event has fired).
    pub queue_started: bool,
    pub extra: UnitExtra,
}

impl Unit {
    /// Its collision region's flags as others' searches and placings meet them: a
    /// unit's region holds 0x02 (+0x34 0x010000ca), cleared once a worker's walk ghosts
    /// (task 0xd0150 clears 0xd6, `FUN_6f699220`), and 0x10 once a worker stops at a mine
    /// until its next walk (0xd0151 sets it, `FUN_6f699260`).  Read: workers waiting at a
    /// mine after their first walk turned another's first walk aside, and one waiting
    /// after its arrival a ghosting way past its cells; a peon just out of a mine,
    /// turning to walk home, let one walking into its cell on.
    pub fn region_flags(&self) -> u32 {
        match self.extra.gold_run.as_ref() {
            None => 0x02,
            Some(r) => (if r.ghosted { 0 } else { 0x02 }) | (if r.at_mine { 0x10 } else { 0 }),
        }
    }

    /// The flags of others' regions that turn it aside or keep it off a place: 0x02,
    /// and 0x10 once its harvest has been in a mine (both tasks set it; 0xd0150 clears
    /// 0xc6, so a ghosting worker meets only workers stopped at a mine: its searches'
    /// mask 0x10000010).  Read: peons out of a mine on later trips staying where they
    /// went in by others walking past, where the first came out clear of all.
    pub fn region_mask(&self) -> u32 {
        match self.extra.gold_run.as_ref() {
            Some(r) if r.entered => (if r.ghosted { 0 } else { 0x02 }) | 0x10,
            _ => 0x02,
        }
    }

    /// Whether its walks are the engine's (a wanderer's, a player's move's, or a gold
    /// run's), whose ways are searched near at hand (`World::local_path`).
    pub fn engine_walker(&self) -> bool {
        self.extra.local_paths || self.extra.player_walk || self.extra.gold_run.is_some()
    }

    /// Whether it walks by the engine's movement steps with the others
    /// (`World::first_steps`, `World::step_walkers`): a wanderer or a unit a player sent
    /// to a point under a move order, or a worker on a gold run between the mine and
    /// home or reaching its tree.
    pub fn steps_walker(&self) -> bool {
        !self.dead
            && (((self.extra.local_paths || self.extra.player_walk) && matches!(self.action, Action::Move { .. }))
                || (matches!(self.action, Action::Harvest { .. }) && self.extra.gold_run.as_ref().is_some_and(|r| matches!(r.phase, crate::goldrun::Phase::ToMine | crate::goldrun::Phase::ToHall(_) | crate::goldrun::Phase::Reaching { .. }))))
    }
}

/// Unit settings scripts change.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct UnitExtra {
    /// A hero's attributes beyond its type's for its level (tomes, SetHeroStr).
    #[serde(default)]
    pub attr_bonus: [f32; 3],
    /// Gold and lumber pillaged but not yet paid, being less than a step.
    #[serde(default)]
    pub pillaged: [f32; 2],
    /// Seconds until a Kodo next digests what it has devoured.
    #[serde(default)]
    pub digest_in: f32,
    /// Seconds until a tornado's damage next strikes, once started.
    #[serde(default)]
    pub pulse_in: Option<f32>,
    /// When a shadowmelder last began to act, and whether it is acting.
    #[serde(default)]
    pub meld_active_since: Option<f32>,
    #[serde(default)]
    pub meld_acting: bool,
    /// A haunted mine's payout timer and the acolytes it last counted at work.
    #[serde(default)]
    pub payout: Option<(f32, u32)>,
    /// A gold mine's holder: the player whose workers are in it, until when (infinite
    /// while any is inside, GoldMineOwnDuration after the last leaves).
    #[serde(default)]
    pub mine_holder: Option<(u8, f32)>,
    /// A lumber swing under way, its chop not yet landed; and the time left of the
    /// backswing after a chop, which a full worker waits out before going home.
    #[serde(default)]
    pub chop_swinging: bool,
    #[serde(default)]
    pub chop_recover: f32,
    /// The item whose spell is being cast, spent when it takes effect.
    #[serde(default)]
    pub casting_item: Option<u32>,
    /// Maximum life gained for good (a Manual of Health).
    #[serde(default)]
    pub permanent_life: f32,
    /// The attack speed the running cooldown was set at.
    #[serde(default)]
    pub cooldown_speed: Option<f32>,
    /// Seconds since the unit began to bear an aura: its auras reach those around every
    /// half second of this and drop their lapsed buffs every two seconds of it.
    #[serde(default)]
    pub aura_clock: Option<f32>,
    /// Damage a structure took while under construction (CAbilityBuildInProgress
    /// +0x10c), which reduces the refund if it is cancelled.
    #[serde(default)]
    pub construction_damage: f32,
    /// Which of the production queue's entries a hero token paid for (refunded as a
    /// token when cancelled), kept beside `queue`.
    #[serde(default)]
    pub queue_tokens: Vec<bool>,
    /// Each chance ability's pseudo-random count: the attempts since it last went off,
    /// starting at 1 (see `World::prd_roll`).
    #[serde(default)]
    pub prd: Vec<(Id, u32)>,
    /// A computer player's unit sent to die (SuicideUnit): the player it goes after,
    /// or -1 for any enemy.
    #[serde(default)]
    pub ai_suicide: Option<i32>,
    /// SetUnitRescuable: players (bits) who rescue the unit by coming near; the range.
    pub rescuable_by: u16,
    pub rescue_range: f32,
    /// SetUnitPathing false: ignores collision.
    pub no_pathing: bool,
    /// SetUnitUseFood.
    pub use_food: bool,
    /// UnitShareVision: players (bits) who see what the unit sees.
    pub shared_vision: u16,
    pub suspend_decay: bool,
    /// UnitAddType and UnitRemoveType changes to the unit's classifications.
    pub added_types: Vec<i32>,
    pub removed_types: Vec<i32>,
    /// Creep sleep: may sleep at night, sleeps always, is asleep.
    pub can_sleep: bool,
    pub sleep_perm: bool,
    pub sleeping: bool,
    pub ignore_alarm: bool,
    pub timed_life_paused: bool,
    /// A waygate's destination and whether it is active.
    pub waygate: Option<(f32, f32, bool)>,
    pub creep_guard: bool,
    pub alt_icon: bool,
    /// A haunted gold mine's gold mine underneath.
    pub mine: Option<u32>,
    /// An acolyte's ring slot at the haunted mine it works.
    pub mine_slot: Option<usize>,
    /// Blight Growth's radius so far and time to its next step.
    pub blight_radius: f32,
    pub blight_timer: f32,
    /// A haunted mine's slot markers.
    pub slot_effects: Vec<u32>,
    /// A structure under construction that advances only while a worker builds it.
    pub needs_builder: bool,
    /// Weapons enabled by the unit's form (weapsOn bits), when not its type's.
    pub weapons: Option<i32>,
    /// Uprooted (an ancient walking), using its alternate animations.
    pub uprooted: bool,
    /// A root or uproot under way: seconds left, and the spot to root at.
    pub rooting: Option<(f32, Option<(f32, f32)>)>,
    /// Where an uprooted ancient goes to root.
    pub root_target: Option<(f32, f32)>,
    /// A defense type the unit's form gives it (UnitBalance defType names).
    pub defense: Option<String>,
    /// Units aboard this transport, and the transport this unit rides.
    pub cargo: Vec<u32>,
    pub transport: u32,
    /// A morph under way: seconds left, the type it becomes, and for a timed form how
    /// long it lasts and the type it returns to.
    pub morph: Option<(f32, crate::objects::Id, Option<(f32, crate::objects::Id)>)>,
    /// How long that morph takes in all, for its progress.
    #[serde(default)]
    pub morph_total: f32,
    /// A timed form's time left and the type it returns to.
    pub morph_revert: Option<(f32, crate::objects::Id)>,
    /// Seconds before the unit takes up an attack order it was given (a random share of
    /// ReactionDelay).
    #[serde(default)]
    pub reaction: f32,
    /// The height the model shows while it moves toward its flying height at
    /// `height_rate` a second (SetUnitFlyHeight with a rate); None once there.
    #[serde(default)]
    pub shown_height: Option<f32>,
    #[serde(default)]
    pub height_rate: f32,
    /// When the unit was last attacked (game time), for autocast.
    pub last_attacked: f32,
    /// The order an autocast interrupted, resumed after the cast.
    pub resume: Option<Action>,
    /// Gold and lumber owed for repair or powerbuild, paid in whole units.
    pub repair_debt: (f32, f32),
    /// A stasis trap: its ability, level, and when it was placed.
    pub trap: Option<(crate::objects::Id, u32, f32)>,
    /// Since when the unit has stood still (Shadowmeld).
    pub still_since: f32,
    /// Storm, Earth and Fire: time left and the spirits.
    pub split: Option<(f32, Vec<u32>)>,
    /// Units a Kodo has devoured.
    #[serde(default)]
    pub devoured: Vec<u32>,
    /// The unit whose spell raised this one (Vengeance), for its limit.
    #[serde(default)]
    pub summoner: u32,
    /// The corpses a meat wagon carries, by unit type.
    #[serde(default)]
    pub corpses: Vec<Id>,
    /// War Club: attacks left with the tree, and the ability.
    #[serde(default)]
    pub club: Option<(Id, i32)>,
    /// A creep's camp, whether it is heading back to it, and since when it has been
    /// beyond GuardDistance.
    #[serde(default)]
    pub home: Option<(f32, f32)>,
    #[serde(default)]
    pub returning: bool,
    #[serde(default)]
    pub strayed: Option<f32>,
    /// When the creep last stood idle away from home, if it has since.
    #[serde(default)]
    pub idle_away: Option<f32>,
    /// The training under way's `rcm and `rtl (taken as the order was given to an empty
    /// queue), released as it finishes; and whether its time has yet to start (it
    /// counts from the turn after the order's).
    #[serde(default)]
    pub training_tags: Option<(u32, u32)>,
    #[serde(default)]
    pub training_waits: bool,
    /// A wanderer's next walk (Awan): its timer's due time on the agent queue (its
    /// single's bits; `World::update_wander`).
    #[serde(default)]
    pub wander_at: Option<u32>,
    /// Cannibalize's own look for a corpse (every 2 seconds), and whether the unit
    /// stood idle when it last looked.
    #[serde(default)]
    pub cannibal_wait: Option<f32>,
    #[serde(default)]
    pub was_idle: bool,
    /// The turn a walker took to go around a unit standing in its way.
    #[serde(default)]
    pub detour: Option<(f32, f32)>,
    /// Its paths are searched near at hand only, without the accelerator's plan, 32
    /// nodes deep (a wanderer of Neutral Passive's: `World::local_path`).
    #[serde(default)]
    pub local_paths: bool,
    /// It walks to a point by a player's order with the engine's steps, its way
    /// searched 700 nodes deep (`World::start_player_walk`).
    #[serde(default)]
    pub player_walk: bool,
    /// The path of the cluster it walks in, sent with others (`crate::groupwalk`).
    #[serde(default)]
    pub group_walk: Option<[u32; 2]>,
    /// The cell where its way, short of the goal, ends: reaching it searches again.
    #[serde(default)]
    pub path_end_cell: Option<(i32, i32)>,
    /// Its walk has had a goal beyond reach (so it moves).
    #[serde(default)]
    pub walk_moved: bool,
    /// When its walk's order was given (the tags' birth count then): walkers take each
    /// step newest first.
    #[serde(default)]
    pub walk_serial: u32,
    /// Where it stood before its last movement step that moved it.
    #[serde(default)]
    pub step_from: Option<(f32, f32)>,
    /// Its way was dropped while blocked: it searches again as its next step starts
    /// (`FUN_6f168740`, then `FUN_6f167ce0`).
    #[serde(default)]
    pub walk_research: bool,
    /// The step of its last local search, while searches are spaced for it (the path's
    /// +0x7c): none once a search leaves its class's budget under 64 passes.
    #[serde(default)]
    pub search_at: Option<u64>,
    /// Steps it turns toward its goal before it is judged facing it, its walk's first
    /// search having found nothing nearer.
    #[serde(default)]
    pub face_hold: u8,
    /// Where its last walk left it, in cells (its single's bits), with the world point
    /// that was set from it: the engine keeps a unit's place in cells, so a walk from
    /// there starts from these, not from the world point converted back.
    #[serde(default)]
    pub cell_at: Option<(u32, u32, f32, f32)>,
    /// Steps it waits, held back by another walker (the path's +0x94).
    #[serde(default)]
    pub walk_wait: u32,
    /// Steps left before a walker blocked by a standing unit or a wall searches again
    /// (the path's +0x98).
    #[serde(default)]
    pub walk_blocked: u32,
    /// The walker it waits for (the path's +0xa8, +0xac).
    #[serde(default)]
    pub yields_to: Option<u32>,
    /// Its spells' listeners on its mana waiting for it to rise past their costs and
    /// 0.1 (CPoReThresholdLis, `` `rtl ``): each threshold with its slot.
    #[serde(default)]
    pub mana_listeners: Vec<(f32, u32)>,
    /// Ensnare's in-range object as its creep woke (`` `pil ``), and the listener made
    /// just before it.
    #[serde(default)]
    pub ensnare_pil: Option<(u32, u32)>,
    /// The step of this tick its walk begins with (its order given after the one before).
    #[serde(default)]
    pub walk_from: Option<usize>,
    /// Its search found no way: it turns toward its goal and is done (`Mover::face`).
    #[serde(default)]
    pub facing_goal: bool,
    /// Its walk by the original's movement steps (`motion`), while it walks.
    #[serde(default)]
    pub mover: Option<crate::motion::Mover>,
    /// The creep sleep effect over a sleeping unit's head.
    #[serde(default)]
    pub sleep_effect: u32,
    /// A creep fallen asleep shows it after a random wait of up to three seconds
    /// (FUN_6f524e00, event 0xd01c1 for FUN_6f524b40); what remains of it.
    #[serde(default)]
    pub sleep_show_wait: Option<f32>,
    /// Its creep spells have made their timers (`World::mana_changed`).
    #[serde(default)]
    pub mana_watched: bool,
    /// The slots of its spells' timers (`World::tag_unit`).
    #[serde(default)]
    pub spell_timers: Vec<u32>,
    /// The slots of its blight growth's objects (`World::tag_unit`).
    #[serde(default)]
    pub blight_slots: Vec<u32>,
    /// A structure's footprint's three regions (`World::tag_unit`), released as it is
    /// removed.
    #[serde(default)]
    pub footprint_slots: Vec<u32>,
    /// Its own objects as made (`World::tag_unit`), each slot with its birth: the ten
    /// first (itself, position, two regions, five reals, path), its modifiers and
    /// listener, and the rest kept (its abilities and theirs), for freeing as it is
    /// removed.
    #[serde(default)]
    pub object_slots: [Vec<(u32, u32)>; 3],
    /// Its object of the last day change (`World::time_of_day_moved`), while it lasts.
    #[serde(default)]
    pub day_rcm: Option<u32>,
    /// The slots of its current move order's objects (`Tags::move_issued`).
    #[serde(default)]
    pub order_tags: Vec<u32>,
    /// A worker's gold harvest as the original's engine carries it out
    /// (`crate::goldrun`).
    #[serde(default)]
    pub gold_run: Option<crate::goldrun::GoldRun>,
    /// A gold mine a worker has been in (its `+fgm` made; `crate::goldrun`).
    #[serde(default)]
    pub mine_shaft: bool,
    /// A gold mine's workers waiting at it for room, first come first (`crate::goldrun`).
    #[serde(default)]
    pub mine_queue: Vec<u32>,
    /// A gold mine's FloatModifier while workers queue at it (its `` `rcm ``) and when it
    /// ends (`crate::goldrun`).
    #[serde(default)]
    pub mine_busy: Option<(u32, f64)>,
    /// Its heading in the engine's movement when its last walk ended (`Mover::heading`),
    /// kept for the next walk (its facing in degrees loses the low bits).
    #[serde(default)]
    pub walk_heading: Option<u32>,
    /// The original's id for it: its tag's slot and birth count (`Tags::take_id`).
    #[serde(default)]
    pub oid: (u32, u32),
    /// The waygate whose box the unit is in after coming through one.
    #[serde(default)]
    pub warp_inside: u32,
    /// When the unit was made, and when a set-off land mine goes off.
    #[serde(default)]
    pub born: f32,
    #[serde(default)]
    pub mine_fuse: Option<f32>,
    /// A locust's caster, the damage it has dealt since it last came back, and whether
    /// it is on its way back.
    #[serde(default)]
    pub swarm: Option<(u32, f32, bool)>,
    /// Effects its abilities attach to it (Sphere, Tree of Life art).
    #[serde(default)]
    pub art: Vec<u32>,
    /// The art researched upgrades show through its abilities (Spiked Barricades,
    /// Reinforced Burrows): each ability and its effects.
    #[serde(default)]
    pub upgrade_art: Vec<(Id, Vec<u32>)>,
    /// A structure's fire: 0 none, 1 small, 2 medium, 3 large; the time to its next
    /// check, and its effects.
    #[serde(default)]
    pub fire: u8,
    #[serde(default)]
    pub fire_timer: f32,
    #[serde(default)]
    pub fire_started: bool,
    /// The corpse has begun to decay (EVENT_UNIT_DECAY fired).
    #[serde(default)]
    pub decaying: bool,
    #[serde(default)]
    pub fire_art: Vec<u32>,
    /// When a tornado last let the unit down.
    #[serde(default)]
    pub spun_at: f32,
    /// Animation properties scripts added (AddUnitAnimationProperties).
    #[serde(default)]
    pub anim_props: Vec<String>,
    /// Animations QueueUnitAnimation lined up after the one playing.
    #[serde(default)]
    pub anim_queue: Vec<String>,
    /// The unit burst as it died and left no corpse.
    #[serde(default)]
    pub exploded: bool,
    /// The formation group the unit moves with.
    #[serde(default)]
    pub formation: Option<u32>,
    /// The widget a structure's rally point is on (0 for the ground).
    #[serde(default)]
    pub rally_target: u32,
    /// Raised by Animate Dead as an invulnerable (the unit flag 0x80000000 of
    /// FUN_6f4e66f0).
    #[serde(default)]
    pub animated: bool,
    /// The target the unit last took up of its own accord.
    #[serde(default)]
    pub acquired: u32,
    /// A bone turned toward a unit, offset from it (SetUnitLookAt).
    #[serde(default)]
    pub look_at: Option<(String, u32, [f32; 3])>,
    /// An illusion's damage dealt and taken factors, and its buff.
    #[serde(default)]
    pub illusion: Option<(f32, f32, Id)>,
    /// A Pocket Factory: its ability, level, and seconds to its next goblin.
    #[serde(default)]
    pub factory: Option<(Id, u32, f32)>,
    /// The life and mana an ankh brings a hero back with (-1 mana keeps it).
    #[serde(default)]
    pub restore: Option<(f32, f32)>,
    /// Abilities Engineering Upgrade replaced: each original and what stands for it now.
    #[serde(default)]
    pub swapped: Vec<(Id, Id)>,
    /// Its spell books (Aspb, its own or a carried item's) and the spells each chose,
    /// which it has while it has the book.
    #[serde(default)]
    pub books: Vec<(Id, Vec<Id>)>,
    /// UnitMakeAbilityPermanent: abilities kept through morphs and upgrades, with the
    /// count of times made permanent (the ability's +0x38).
    #[serde(default)]
    pub permanent: Vec<(Id, i32)>,
    /// Abilities whose AI rests until a time (spellai.rs).
    #[serde(default)]
    pub ai_rest: Vec<(Id, f32)>,
    /// When the AI next looks about for its area spells (spellai.rs).
    #[serde(default)]
    pub ai_watch: f32,
    /// The units each watching ability saw inside its range last time.
    #[serde(default)]
    pub ai_inside: Vec<(Id, Vec<u32>)>,
    /// The spell AI's timers that run a hook's second step when they come due (their
    /// 0xd01bf events): the ability and when.
    #[serde(default)]
    pub ai_timers: Vec<(Id, f32)>,
    /// SetUnitBlendTime: seconds its animations cross-fade over, in place of its type's.
    #[serde(default)]
    pub blend_time: Option<f32>,
    /// Orders given with Shift, waiting for the ones before them: id, point, target.
    #[serde(default)]
    pub order_queue: std::collections::VecDeque<(i32, f32, f32, u32)>,
}

impl Default for UnitExtra {
    fn default() -> Self {
        UnitExtra {
            attr_bonus: [0.0; 3],
            pillaged: [0.0; 2],
            digest_in: 0.0,
            pulse_in: None,
            meld_active_since: None,
            meld_acting: false,
            aura_clock: None,
            payout: None,
            mine_holder: None,
            chop_swinging: false,
            chop_recover: 0.0,
            casting_item: None,
            permanent_life: 0.0,
            cooldown_speed: None,
            construction_damage: 0.0,
            queue_tokens: Vec::new(),
            prd: Vec::new(),
            formation: None,
            rally_target: 0,
            animated: false,
            ai_suicide: None,
            rescuable_by: 0,
            rescue_range: 0.0,
            no_pathing: false,
            use_food: true,
            shared_vision: 0,
            suspend_decay: false,
            added_types: Vec::new(),
            removed_types: Vec::new(),
            can_sleep: false,
            sleep_perm: false,
            sleeping: false,
            ignore_alarm: false,
            timed_life_paused: false,
            waygate: None,
            creep_guard: false,
            alt_icon: false,
            mine: None,
            mine_slot: None,
            blight_radius: 0.0,
            blight_timer: 0.0,
            slot_effects: Vec::new(),
            needs_builder: false,
            weapons: None,
            uprooted: false,
            rooting: None,
            root_target: None,
            defense: None,
            cargo: Vec::new(),
            transport: 0,
            morph: None,
            morph_total: 0.0,
            morph_revert: None,
            reaction: 0.0,
            shown_height: None,
            height_rate: 0.0,
            last_attacked: -100.0,
            devoured: Vec::new(),
            summoner: 0,
            corpses: Vec::new(),
            club: None,
            home: None,
            returning: false,
            strayed: None,
            idle_away: None,
            training_tags: None,
            training_waits: false,
            wander_at: None,
            cannibal_wait: None,
            was_idle: false,
            detour: None,
            local_paths: false,
            player_walk: false,
            group_walk: None,
            path_end_cell: None,
            walk_moved: false,
            walk_wait: 0,
            walk_research: false,
            search_at: None,
            face_hold: 0,
            cell_at: None,
            step_from: None,
            walk_blocked: 0,
            walk_serial: 0,
            yields_to: None,
            mover: None,
            facing_goal: false,
            walk_from: None,
            mana_listeners: Vec::new(),
            ensnare_pil: None,
            sleep_effect: 0,
            sleep_show_wait: None,
            warp_inside: 0,
            born: 0.0,
            mine_fuse: None,
            swarm: None,
            art: Vec::new(),
            upgrade_art: Vec::new(),
            fire: 0,
            fire_timer: 0.0,
            fire_started: false,
            decaying: false,
            fire_art: Vec::new(),
            spun_at: -100.0,
            anim_props: Vec::new(),
            anim_queue: Vec::new(),
            exploded: false,
            acquired: 0,
            look_at: None,
            illusion: None,
            factory: None,
            restore: None,
            swapped: Vec::new(),
            permanent: Vec::new(),
            books: Vec::new(),
            ai_rest: Vec::new(),
            ai_watch: 0.0,
            ai_inside: Vec::new(),
            ai_timers: Vec::new(),
            blend_time: None,
            order_queue: Default::default(),
            resume: None,
            repair_debt: (0.0, 0.0),
            trap: None,
            still_since: 0.0,
            split: None,
            mana_watched: false,
            spell_timers: Vec::new(),
            blight_slots: Vec::new(),
            footprint_slots: Vec::new(),
            object_slots: Default::default(),
            day_rcm: None,
            order_tags: Vec::new(),
            gold_run: None,
            mine_shaft: false,
            mine_queue: Vec::new(),
            mine_busy: None,
            walk_heading: None,
            oid: (0, 0),
        }
    }
}

/// Bonuses from held items' abilities.
#[derive(Clone, Copy, Debug, Default, serde::Serialize, serde::Deserialize)]
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
    /// Life a second (Ring of Regeneration), the share of spell damage taken away
    /// (Runed Bracers), and the share of attack damage regained (Mask of Death).
    #[serde(default)]
    pub life_regen: f32,
    #[serde(default)]
    pub spell_reduction: f32,
    #[serde(default)]
    pub life_steal: f32,
}

/// An effect on a unit for a while: from spells, auras, and passives.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Buff {
    /// Buff type from AbilityBuffData (e.g. BHtb).
    pub id: Id,
    /// An aura's: what is left of the 2 seconds each of its bearer's scans gives it; it
    /// ends only at one of the bearer's sweeps once this has run out.
    #[serde(default)]
    pub aura_left: Option<f32>,
    /// How many times it has been laid on (Incinerate).
    #[serde(default)]
    pub stacks: u32,
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
    /// Seconds between the damage's ticks (0: dealt every update), and until the next.
    #[serde(default)]
    pub dps_every: f32,
    #[serde(default)]
    pub dps_next: f32,
    pub mana_regen: f32,
    pub life_regen: f32,
    /// Its healing is magical: more on an ethereal unit (Rejuvenation, Eat Tree).
    #[serde(default)]
    pub ethereal_heal: bool,
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
    /// Ends when the unit is hurt (Healing Salve, Clarity Potion).
    #[serde(default)]
    pub ends_on_damage: bool,
    /// Fraction of attack damage regained as life (Vampiric Aura).
    pub life_steal: f32,
    /// Melee attackers take this damage (or this fraction when `return_pct`).
    pub return_damage: f32,
    pub return_pct: bool,
    /// Wind Walk's first strike bonus.
    pub backstab: f32,
    /// Seconds before an `invisible` buff hides its bearer (Wind Walk's transition).
    #[serde(default)]
    pub fade: f32,
    /// Fraction of attacks that miss (Curse).
    pub miss_chance: f32,
    /// Fraction more damage taken (Berserk).
    pub damage_taken: f32,
    /// Damage a second to units within `area` of the bearer (Lightning Shield).
    pub area_dps: f32,
    pub area: f32,
    /// A slow wearing off over this many seconds (Purge).
    pub decaying: f32,
    /// The bearer cannot be repaired (Liquid Fire).
    pub no_repair: bool,
    /// Life regenerated a second as a fraction of maximum life (Healing Ward).
    pub life_regen_pct: f32,
    /// Mana regained a second as a share of maximum mana (a fountain of mana).
    #[serde(default)]
    pub mana_regen_pct: f32,
    /// Fraction of piercing damage taken (Defend); zero when it does not apply.
    pub pierce_taken: f32,
    /// No spells (Silence, Doom, Soul Burn, Hex).
    pub silence: bool,
    /// Mana Shield: mana spent per point of damage, and the fraction of damage taken.
    pub mana_shield: (f32, f32),
    /// Attacks prevented (Cloud): 1 melee, 2 ranged.
    #[serde(default)]
    pub disarm: u8,
    /// The unit shows as this type meanwhile (Polymorph, Hex), and moves at this
    /// speed.
    #[serde(default)]
    pub shown_as: Option<crate::objects::Id>,
    #[serde(default)]
    pub move_set: f32,
    /// Spirit Link: the group the unit is linked in (0 for none) and the share of its
    /// damage the group takes.
    #[serde(default)]
    pub link_group: u32,
    #[serde(default)]
    pub link_share: f32,
}

/// A spell kept up by its caster: it ends when the time runs out or the caster stops.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
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
    /// A lightning tying the caster to its target while the channel lasts (Aerial
    /// Shackles), and the buff it holds on the target.
    #[serde(default)]
    pub link: u32,
    /// Stampede's lanes run (stampede.rs).
    #[serde(default)]
    pub lanes: u8,
    /// A computer's channel: its AI's re-checks failed in a row (spellai.rs).
    #[serde(default)]
    pub ai_fails: u8,
    /// Blizzard: seconds until each wave that has begun strikes, and the units its
    /// waves have struck, which no later wave strikes again.
    #[serde(default)]
    pub strikes: Vec<f32>,
    #[serde(default)]
    pub struck: Vec<u32>,
    #[serde(default)]
    pub held: Id,
}

/// A spell's lasting effect on the ground (Flame Strike, Earthquake).
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
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
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Action {
    #[default]
    Idle,
    Move { x: f32, y: f32 },
    Attack { target: u32 },
    AttackMove { x: f32, y: f32 },
    /// Attack ground: an artillery weapon shells a point, again and again.
    AttackGround { x: f32, y: f32 },
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
    /// Walking to a unit to hand it an item.
    GiveItem { item: u32, target: u32 },
    /// A worker constructing a structure.
    Construct { target: u32 },
    /// A transport going to take a unit aboard.
    Load { target: u32 },
    /// A unit going to board a transport.
    Board { transport: u32 },
    /// A transport going to unload at a point.
    Unload { x: f32, y: f32 },
    /// A worker repairing a structure or mechanical unit.
    Repair { target: u32 },
}

/// An attack in progress: damage lands at the damage point, then the backswing plays.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Swing {
    pub target: u32,
    /// The point an attack on the ground aims at.
    #[serde(default)]
    pub ground: Option<(f32, f32)>,
    pub elapsed: f32,
    pub done: bool,
    /// Weapon 1 or 2.
    pub weapon: u8,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Destructable {
    pub handle: u32,
    /// The original's id for it: its tag's slot and birth count (`Tags::take_id`).
    #[serde(default)]
    pub oid: (u32, u32),
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
    /// The animation a script asked for, changed with each request
    /// (SetDestructableAnimation), and its speed (SetDestructableAnimationSpeed).
    pub animation: Option<String>,
    #[serde(default)]
    pub anim_serial: u32,
    #[serde(default = "one")]
    pub anim_speed: f32,
    pub pathing_cells: Vec<(i32, i32)>,
    /// SetDestructableOccluderHeight: the occlusion height in place of the type's.
    #[serde(default)]
    pub occluder_height: Option<f32>,
}

fn one() -> f32 {
    1.0
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Item {
    pub handle: u32,
    /// The original's id for it: its tag's slot and birth count (`Tags::take_id`).
    #[serde(default)]
    pub oid: (u32, u32),
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
    /// The type of unit whose death left it (SetItemDropID; the Rune of Rebirth's).
    #[serde(default)]
    pub drop_type: Option<Id>,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Timer {
    /// Time left in the current run, while running (f64: f32 counting down from a
    /// large timeout drifts, 0.03 in 40 seconds from 100).
    pub remaining: f64,
    pub timeout: f32,
    pub periodic: bool,
    pub running: bool,
    pub callback: Option<FuncId>,
    /// The handler object (hdlr) it took with its first function, kept as it is
    /// started again with another (traced: the restock timer, started at 120 seconds by
    /// its own function with PerformStockUpdates, takes and frees nothing).
    #[serde(default)]
    pub handler: Option<u32>,
    /// The length of the current run: the timeout, or what was left when it was resumed.
    #[serde(default)]
    pub period: f64,
    /// What was left when last paused (0 until then); a stopped timer reads by it,
    /// stale or not (FUN_6f232500, FUN_6f233190).
    #[serde(default)]
    pub saved: f32,
    /// When its run was started, in order among timers' runs.
    #[serde(default)]
    pub seq: u64,
}

impl Timer {
    /// TimerGetRemaining: the run's time left while running, else what was left when
    /// last paused (measured: a one-shot timer that has fired reads the remaining of
    /// the pause before it).
    pub fn remaining_now(&self) -> f32 {
        if self.running { self.remaining as f32 } else { self.saved }
    }

    /// TimerGetElapsed: the run's time gone while running, else the timeout less what
    /// was left when last paused (measured: -0.83 for a timer paused with 0.83 left and
    /// then started for 0).
    pub fn elapsed(&self) -> f32 {
        if self.running { (self.period - self.remaining) as f32 } else { self.timeout - self.saved }
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
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
            wait_on_sleeps: false,
            events: Vec::new(),
            conditions: Vec::new(),
            actions: Vec::new(),
            eval_count: 0,
            exec_count: 0,
        }
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum BoolExpr {
    Func(FuncId),
    And(u32, u32),
    Or(u32, u32),
    Not(u32),
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Group {
    pub units: Vec<u32>,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Force {
    pub players: u32,
}

#[derive(Clone, Copy, Debug, Default, serde::Serialize, serde::Deserialize)]
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
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
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

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Effect {
    pub model: String,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    /// Attached to a unit or other widget at an attachment point.
    pub target: Option<(u32, String)>,
    pub destroyed: bool,
    /// Degrees counterclockwise from east.
    pub facing: f32,
}

/// A script dialog (DialogCreate): a message over a column of buttons.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Dialog {
    pub message: String,
    pub buttons: Vec<u32>,
    /// Players it is shown to, as a bit mask.
    pub shown: u32,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct DialogButton {
    pub dialog: u32,
    pub text: String,
    pub hotkey: i32,
    /// DialogAddQuitButton: clicking it ends the game, optionally to the score screen.
    pub quit: Option<bool>,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
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
    /// When StartSound last started it (moved by SetSoundPlayPosition).
    #[serde(default)]
    pub started: Option<f32>,
    /// SetSoundConeAngles: inside and outside angles in degrees and the volume
    /// outside (0 to 127); SetSoundConeOrientation: the way the cone points.
    #[serde(default)]
    pub cone: Option<(f32, f32, i32)>,
    #[serde(default)]
    pub cone_dir: Option<(f32, f32, f32)>,
    /// AttachSoundToUnit: the unit it goes with.
    #[serde(default)]
    pub unit: Option<u32>,
}

/// A place a stacked sound is heard from (RegisterStackedSound): a rect about a point,
/// or a unit.  One voice of each sound file plays, from the place nearest the listener
/// (FUN_6f340d10).
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct StackedSound {
    pub file: String,
    pub sound: u32,
    pub unit: Option<u32>,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
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
    /// An engine text tag, shown only to the local player.
    pub local: bool,
}

/// What a trigger event registration listens for.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
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

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct EventReg {
    pub trigger: u32,
    pub kind: EventKind,
}

/// Values the event-response natives (GetTriggerUnit and friends) read.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
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
#[derive(Clone, Copy, Debug, Default, serde::Serialize, serde::Deserialize)]
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

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Hashtable {
    pub ints: HashMap<(i32, i32), i32>,
    pub reals: HashMap<(i32, i32), f32>,
    pub bools: HashMap<(i32, i32), bool>,
    pub strings: HashMap<(i32, i32), String>,
    pub handles: HashMap<(i32, i32), u32>,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct GameCache {
    pub name: String,
    pub ints: HashMap<(String, String), i32>,
    pub reals: HashMap<(String, String), f32>,
    pub bools: HashMap<(String, String), bool>,
    pub strings: HashMap<(String, String), String>,
    /// Units kept by StoreUnit.
    pub units: HashMap<(String, String), StoredUnit>,
}

/// A unit as StoreUnit keeps it: type, hero progress, abilities, and items.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct StoredUnit {
    pub type_id: [u8; 4],
    pub level: i32,
    pub xp: i32,
    pub skill_points: i32,
    pub str_: f32,
    pub agi: f32,
    pub int: f32,
    pub proper_name: String,
    /// Abilities and their levels.
    pub abilities: Vec<([u8; 4], i32)>,
    /// Items by slot: type and charges.
    pub items: Vec<Option<([u8; 4], i32)>>,
    /// The original's record of it (`w3_formats::w3v::unit_record_len`), when it came
    /// from one: sent on as it came, its fields this game does not read kept.
    #[serde(default)]
    pub record: Vec<u8>,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct CameraSetup {
    pub fields: HashMap<i32, f32>,
    /// Each field's transition time (CameraSetupSetField's duration).
    pub durations: HashMap<i32, f32>,
    pub x: f32,
    pub y: f32,
}

/// A transmission on the cinematic panel (SetCinematicScene).
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
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
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
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
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
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
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
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
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct QuestItem {
    pub quest: u32,
    pub description: String,
    pub completed: bool,
}

/// A handle-table entry.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum Obj {
    Weather(Weather),
    Image(Image),
    Ubersplat(Ubersplat),
    Lightning(Lightning),
    /// Unit or item types with weights (CreateUnitPool, CreateItemPool).
    UnitPool(Vec<(Id, f32)>),
    ItemPool(Vec<(Id, f32)>),
    TimerDialog(TimerDialog),
    Leaderboard(Leaderboard),
    Multiboard(Multiboard),
    /// A multiboard cell handle: board, row, column.
    MultiboardItem(u32, usize, usize),
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
    Opaque(String),
}

/// A timer dialog (CTimerDialog): a title and its timer's remaining time.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct TimerDialog {
    pub timer: u32,
    /// None shows "DEFAULTTIMERDIALOGTEXT".
    pub title: Option<String>,
    pub title_color: Option<[u8; 4]>,
    pub time_color: Option<[u8; 4]>,
    pub speed: f32,
    pub displayed: bool,
    /// TimerDialogSetRealTimeRemaining: a countdown kept by the dialog itself.
    pub real_time: Option<f32>,
}

impl Default for TimerDialog {
    fn default() -> TimerDialog {
        TimerDialog { timer: 0, title: None, title_color: None, time_color: None, speed: 1.0, displayed: false, real_time: None }
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct LeaderboardItem {
    pub label: String,
    pub value: i32,
    pub player: u8,
    /// Show label, value, and icon.
    pub style: [bool; 3],
    /// None is the player's color.
    pub label_color: Option<[u8; 4]>,
    pub value_color: Option<[u8; 4]>,
}

impl Default for LeaderboardItem {
    fn default() -> LeaderboardItem {
        LeaderboardItem { label: String::new(), value: 0, player: 0, style: [true; 3], label_color: None, value_color: None }
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Leaderboard {
    pub label: String,
    pub items: Vec<LeaderboardItem>,
    /// Rows the board is sized for (LeaderboardSetSizeByItemCount).
    pub size: usize,
    pub displayed: bool,
    /// Show the label, player names, values, and icons.
    pub style: [bool; 4],
}

impl Default for Leaderboard {
    fn default() -> Leaderboard {
        Leaderboard { label: String::new(), items: Vec::new(), size: 0, displayed: false, style: [true; 4] }
    }
}

/// A multiboard cell (CMultiboardElementData): white text, value and icon shown, 0.01 wide.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct MultiboardItem {
    pub value: String,
    pub icon: String,
    pub color: [u8; 4],
    pub show_value: bool,
    pub show_icon: bool,
    pub width: f32,
}

impl Default for MultiboardItem {
    fn default() -> MultiboardItem {
        MultiboardItem { value: String::new(), icon: String::new(), color: [255; 4], show_value: true, show_icon: true, width: 0.01 }
    }
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Multiboard {
    pub title: String,
    pub title_color: Option<[u8; 4]>,
    pub rows: usize,
    pub cols: usize,
    pub items: Vec<Vec<MultiboardItem>>,
    pub displayed: bool,
    pub minimized: bool,
}

impl Multiboard {
    pub fn resize(&mut self, rows: usize, cols: usize) {
        self.rows = rows;
        self.cols = cols;
        self.items.resize_with(rows, Vec::new);
        for r in &mut self.items {
            r.resize_with(cols, MultiboardItem::default);
        }
    }

    pub fn each(&mut self, mut f: impl FnMut(&mut MultiboardItem)) {
        for r in &mut self.items {
            for it in r {
                f(it);
            }
        }
    }
}

/// An image (CreateImage): a texture laid on the ground.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Image {
    pub file: String,
    pub size: [f32; 3],
    pub pos: [f32; 3],
    pub origin: [f32; 3],
    /// IMAGE_TYPE: 1 selection, 2 indicator, 3 occlusion mask, 4 ubersplat.
    pub kind: i32,
    pub shown: bool,
    pub render: bool,
    pub render_always: bool,
    pub above_water: (bool, bool),
    pub constant_height: Option<f32>,
    pub color: [u8; 4],
}

/// An ubersplat (CreateUbersplat): a Splats\UberSplatData.slk ground texture.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Ubersplat {
    pub x: f32,
    pub y: f32,
    pub name: String,
    pub color: [u8; 4],
    pub paused: bool,
    pub no_birth: bool,
    pub shown: bool,
    pub render: bool,
    pub render_always: bool,
    /// Seconds since it started (ResetUbersplat starts over); None when finished.
    pub age: Option<f32>,
}

/// A lightning effect between two points (AddLightning): a Splats\LightningData.slk type.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Lightning {
    pub code: String,
    pub check_visibility: bool,
    pub from: [f32; 3],
    pub to: [f32; 3],
    pub color: [f32; 4],
}

/// A weather effect (AddWeatherEffect): a TerrainArt\\Weather.slk effect over a rect.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Weather {
    pub rect: Rect,
    pub id: crate::objects::Id,
    pub enabled: bool,
}
