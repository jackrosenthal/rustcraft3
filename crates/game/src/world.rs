//! Game state and the script host.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use w3_jass::{FuncId, Host, NativeResult, Outcome, Thread, Value, Vm};
use w3_mpq::Chain;

use crate::handles::Handles;
use crate::objects::ObjectDb;
use crate::types::*;

pub type NativeFn = fn(&mut World, &mut Vm, &[Value]) -> NativeResult;

/// Messages for the presentation layer (renderer, audio, interface).
#[derive(Clone, Debug)]
pub enum Request {
    DisplayText { players: u32, x: f32, y: f32, text: String, duration: f32 },
    ClearText { players: u32 },
    PlaySound(u32),
    StopSound { sound: u32, fade: bool },
    PlayMusic(String),
    StopMusic(bool),
    SetCameraPosition { players: u32, x: f32, y: f32 },
    PanCamera { players: u32, x: f32, y: f32, z: Option<f32>, duration: f32 },
    SetCameraField { players: u32, field: i32, value: f32, duration: f32 },
    ResetCamera { players: u32, duration: f32 },
    CinematicMode(bool),
    Fade { players: u32, out: bool, duration: f32 },
    EndGame,
}

/// A suspended trigger thread and the event context it resumes with.
pub struct Sleeper {
    pub wake_at: f32,
    pub thread: Thread,
    pub ctx: EventCtx,
}

pub struct World {
    pub chain: Arc<Chain>,
    pub objects: ObjectDb,
    pub handles: Handles<Obj>,
    pub players: Vec<Player>,
    pub start_locations: Vec<StartLocation>,
    pub events: Vec<u32>,
    pub natives: Vec<Option<NativeFn>>,
    pub native_names: Vec<String>,
    warned: HashSet<u32>,
    /// Game time in seconds.
    pub time: f32,
    pub ctx: EventCtx,
    pub enum_ctx: EnumCtx,
    pub sleepers: Vec<Sleeper>,
    /// Threads started by natives (TriggerExecute, ExecuteFunc) that must run once the
    /// current native returns.
    pub pending: Vec<(Thread, EventCtx)>,
    pub requests: Vec<Request>,
    pub map_name: String,
    pub map_description: String,
    pub player_count: i32,
    pub team_count: i32,
    pub placement: i32,
    pub game_type_selected: i32,
    pub game_types_supported: i32,
    pub map_flags: i32,
    pub game_speed: i32,
    pub difficulty: i32,
    pub camera_bounds: [f32; 8],
    pub camera_margins: [f32; 4],
    pub map_bounds: Rect,
    pub playable: Rect,
    pub time_of_day: f32,
    pub time_of_day_scale: f32,
    pub day_night_models: (String, String),
    pub fog_enabled: bool,
    pub fog_mask_enabled: bool,
    pub terrain_heights: Option<TerrainHeights>,
    pub global_variables_changed: HashMap<String, f32>,
    /// Units\\MiscData.txt merged with Units\\MiscGame.txt: gameplay constants.
    pub misc: w3_formats::profile::Profile,
    pub rng: crate::rng::Rng,
    /// `TRIGSTR_n` strings from war3map.wts.
    pub trigger_strings: HashMap<u32, String>,
    /// Localized interface strings (FDF StringList entries).
    pub strings: HashMap<String, String>,
    pub local_player: u8,
    /// VERSION_REIGN_OF_CHAOS (0) or VERSION_FROZEN_THRONE (1).
    pub version: i32,
    /// Live units in creation order, the order enumerations visit them.
    pub unit_order: Vec<u32>,
    pub selection: HashMap<u8, Vec<u32>>,
    pub destructable_order: Vec<u32>,
    pub item_order: Vec<u32>,
    /// Effects destroyed by script, with the time they were destroyed; the presentation
    /// layer plays their death animation before the handle is freed.
    pub dying_effects: Vec<(u32, f32)>,
    pub pathing: Option<crate::pathing::PathingMap>,
    pub missiles: Vec<crate::sim::Missile>,
    pathing_textures: HashMap<String, Option<w3_formats::blp::Image>>,
}

/// Ground heights for GetLocationZ and unit placement.
pub struct TerrainHeights {
    pub width: u32,
    pub height: u32,
    pub offset: [f32; 2],
    pub heights: Vec<f32>,
}

impl TerrainHeights {
    pub fn at(&self, x: f32, y: f32) -> f32 {
        let fx = ((x - self.offset[0]) / 128.0).clamp(0.0, (self.width - 1) as f32);
        let fy = ((y - self.offset[1]) / 128.0).clamp(0.0, (self.height - 1) as f32);
        let (ix, iy) = (fx.floor() as u32, fy.floor() as u32);
        let (tx, ty) = (fx - ix as f32, fy - iy as f32);
        let at = |x: u32, y: u32| self.heights[(y.min(self.height - 1) * self.width + x.min(self.width - 1)) as usize];
        let a = at(ix, iy) * (1.0 - tx) + at(ix + 1, iy) * tx;
        let b = at(ix, iy + 1) * (1.0 - tx) + at(ix + 1, iy + 1) * tx;
        a * (1.0 - ty) + b * ty
    }
}

impl World {
    pub fn new(chain: Arc<Chain>, objects: ObjectDb) -> World {
        let mut misc = w3_formats::profile::Profile::default();
        for f in ["UI\\MiscData.txt", "Units\\MiscData.txt", "Units\\MiscGame.txt"] {
            if let Ok(s) = chain.read_string(f) {
                misc.merge(&s);
            }
        }
        let mut handles = Handles::default();
        let mut players = Vec::new();
        // The sixteen player handles are created first, so Player(0) is the first handle.
        for i in 0..PLAYER_COUNT as u8 {
            let h = handles.insert(Obj::Player(i));
            players.push(Player::new(i, h));
        }
        for (i, p) in players.iter_mut().enumerate() {
            for j in 0..PLAYER_COUNT {
                if i == j {
                    p.alliances[j] = [true; ALLIANCE_TYPES];
                }
            }
        }
        World {
            chain,
            objects,
            handles,
            players,
            start_locations: Vec::new(),
            events: Vec::new(),
            natives: Vec::new(),
            native_names: Vec::new(),
            warned: HashSet::new(),
            time: 0.0,
            ctx: EventCtx::default(),
            enum_ctx: EnumCtx::default(),
            sleepers: Vec::new(),
            pending: Vec::new(),
            requests: Vec::new(),
            map_name: String::new(),
            map_description: String::new(),
            player_count: 0,
            team_count: 0,
            placement: 0,
            game_type_selected: 0,
            game_types_supported: 0,
            map_flags: 0,
            game_speed: 2,
            difficulty: 1,
            camera_bounds: [0.0; 8],
            camera_margins: [0.0; 4],
            map_bounds: Rect::default(),
            playable: Rect::default(),
            time_of_day: 8.0,
            time_of_day_scale: 1.0,
            day_night_models: (String::new(), String::new()),
            fog_enabled: true,
            fog_mask_enabled: true,
            terrain_heights: None,
            global_variables_changed: HashMap::new(),
            misc,
            rng: crate::rng::Rng::new(0),
            trigger_strings: HashMap::new(),
            strings: HashMap::new(),
            local_player: 0,
            version: 1,
            unit_order: Vec::new(),
            selection: HashMap::new(),
            destructable_order: Vec::new(),
            item_order: Vec::new(),
            dying_effects: Vec::new(),
            pathing: None,
            missiles: Vec::new(),
            pathing_textures: HashMap::new(),
        }
    }

    /// Resolves `TRIGSTR_n` references and interface string keys; other text is returned
    /// unchanged.
    pub fn localized(&self, s: &str) -> String {
        if let Some(n) = s.strip_prefix("TRIGSTR_") {
            let digits: String = n.chars().take_while(|c| c.is_ascii_digit()).collect();
            if let Some(t) = digits.parse().ok().and_then(|n: u32| self.trigger_strings.get(&n)) {
                return t.clone();
            }
        }
        self.strings.get(s).cloned().unwrap_or_else(|| s.to_string())
    }

    /// A gameplay constant from Units\\MiscData.txt or Units\\MiscGame.txt.
    pub fn misc_f32(&self, key: &str, default: f32) -> f32 {
        self.misc.get_str("Misc", key).and_then(|v| v.trim().parse().ok()).unwrap_or(default)
    }

    /// Resolves each declared native to its implementation.
    pub fn bind_natives(&mut self, vm: &Vm) {
        let table = crate::natives::table();
        self.natives = vm.module.natives.iter().map(|n| table.get(&*n.name).copied()).collect();
        self.native_names = vm.module.natives.iter().map(|n| n.name.to_string()).collect();
        let missing = self.natives.iter().filter(|n| n.is_none()).count();
        log::info!("{} natives bound, {missing} not yet implemented", self.natives.len() - missing);
    }

    // ---- handle accessors ----

    pub fn unit(&self, h: u32) -> Option<&Unit> {
        match self.handles.get(h) {
            Some(Obj::Unit(u)) => Some(u),
            _ => None,
        }
    }
    pub fn unit_mut(&mut self, h: u32) -> Option<&mut Unit> {
        match self.handles.get_mut(h) {
            Some(Obj::Unit(u)) => Some(u),
            _ => None,
        }
    }
    pub fn player_of(&self, h: u32) -> Option<u8> {
        match self.handles.get(h) {
            Some(Obj::Player(p)) => Some(*p),
            _ => None,
        }
    }
    pub fn player_handle(&self, id: u8) -> Value {
        self.players.get(id as usize).map_or(Value::Null, |p| Value::Handle(p.handle))
    }
    pub fn timer_mut(&mut self, h: u32) -> Option<&mut Timer> {
        match self.handles.get_mut(h) {
            Some(Obj::Timer(t)) => Some(t),
            _ => None,
        }
    }
    pub fn trigger(&self, h: u32) -> Option<&Trigger> {
        match self.handles.get(h) {
            Some(Obj::Trigger(t)) => Some(t),
            _ => None,
        }
    }
    pub fn trigger_mut(&mut self, h: u32) -> Option<&mut Trigger> {
        match self.handles.get_mut(h) {
            Some(Obj::Trigger(t)) => Some(t),
            _ => None,
        }
    }
    pub fn group_mut(&mut self, h: u32) -> Option<&mut Group> {
        match self.handles.get_mut(h) {
            Some(Obj::Group(g)) => Some(g),
            _ => None,
        }
    }
    pub fn group(&self, h: u32) -> Option<&Group> {
        match self.handles.get(h) {
            Some(Obj::Group(g)) => Some(g),
            _ => None,
        }
    }
    pub fn force(&self, h: u32) -> Option<&Force> {
        match self.handles.get(h) {
            Some(Obj::Force(f)) => Some(f),
            _ => None,
        }
    }
    pub fn force_mut(&mut self, h: u32) -> Option<&mut Force> {
        match self.handles.get_mut(h) {
            Some(Obj::Force(f)) => Some(f),
            _ => None,
        }
    }
    pub fn rect(&self, h: u32) -> Option<Rect> {
        match self.handles.get(h) {
            Some(Obj::Rect(r)) => Some(*r),
            _ => None,
        }
    }
    pub fn location(&self, h: u32) -> Option<(f32, f32)> {
        match self.handles.get(h) {
            Some(Obj::Location(x, y)) => Some((*x, *y)),
            _ => None,
        }
    }
    pub fn region(&self, h: u32) -> Option<&Region> {
        match self.handles.get(h) {
            Some(Obj::Region(r)) => Some(r),
            _ => None,
        }
    }

    pub fn ground_z(&self, x: f32, y: f32) -> f32 {
        self.terrain_heights.as_ref().map_or(0.0, |t| t.at(x, y))
    }

    // ---- boolean expressions and triggers ----

    /// Evaluates a boolexpr handle; `null` evaluates to true.
    pub fn eval_boolexpr(&mut self, vm: &mut Vm, h: u32) -> bool {
        let e = match self.handles.get(h) {
            Some(Obj::BoolExpr(e)) => e.clone(),
            _ => return true,
        };
        match e {
            BoolExpr::Func(f) => match vm.call(self, f, &[]) {
                Outcome::Finished(v) => v.bool(),
                Outcome::Aborted(err) => {
                    log::warn!("condition aborted: {err}");
                    false
                }
                Outcome::Sleeping(_) => false,
            },
            BoolExpr::And(a, b) => self.eval_boolexpr(vm, a) && self.eval_boolexpr(vm, b),
            BoolExpr::Or(a, b) => self.eval_boolexpr(vm, a) || self.eval_boolexpr(vm, b),
            BoolExpr::Not(a) => !self.eval_boolexpr(vm, a),
        }
    }

    pub fn trigger_evaluate(&mut self, vm: &mut Vm, trig: u32) -> bool {
        let conds = match self.trigger_mut(trig) {
            Some(t) => {
                t.eval_count += 1;
                t.conditions.clone()
            }
            None => return false,
        };
        for c in conds {
            let expr = match self.handles.get(c) {
                Some(Obj::TriggerCondition { expr }) => *expr,
                _ => continue,
            };
            if !self.eval_boolexpr(vm, expr) {
                return false;
            }
        }
        true
    }

    /// Runs a trigger's actions on a new thread that may sleep.
    pub fn trigger_execute(&mut self, vm: &mut Vm, trig: u32, ctx: EventCtx) {
        let actions = match self.trigger_mut(trig) {
            Some(t) => {
                t.exec_count += 1;
                t.actions.clone()
            }
            None => return,
        };
        for a in actions {
            let f = match self.handles.get(a) {
                Some(Obj::TriggerAction { func }) => *func,
                _ => continue,
            };
            let mut ctx = ctx.clone();
            ctx.trigger = trig;
            let t = vm.new_thread(f, &[], true);
            self.run_thread(vm, t, ctx);
        }
    }

    /// Runs a thread with an event context until it finishes or sleeps.
    pub fn run_thread(&mut self, vm: &mut Vm, mut t: Thread, ctx: EventCtx) {
        let saved = std::mem::replace(&mut self.ctx, ctx);
        match vm.run(self, &mut t) {
            Outcome::Finished(_) => {}
            Outcome::Sleeping(secs) => {
                let ctx = self.ctx.clone();
                self.sleepers.push(Sleeper { wake_at: self.time + secs.max(0.0), thread: t, ctx });
            }
            Outcome::Aborted(e) => log::warn!("thread aborted: {e}"),
        }
        self.ctx = saved;
        self.run_pending(vm);
    }

    fn run_pending(&mut self, vm: &mut Vm) {
        while !self.pending.is_empty() {
            let batch = std::mem::take(&mut self.pending);
            for (t, ctx) in batch {
                self.run_thread(vm, t, ctx);
            }
        }
    }

    /// Fires every enabled trigger registered for events matching `pred`, evaluating
    /// conditions first.
    pub fn fire(&mut self, vm: &mut Vm, ctx: EventCtx, pred: impl Fn(&World, &EventKind) -> bool) {
        let regs: Vec<(u32, u32)> = self
            .events
            .iter()
            .filter_map(|&e| match self.handles.get(e) {
                Some(Obj::Event(r)) if pred(self, &r.kind) => Some((e, r.trigger)),
                _ => None,
            })
            .collect();
        for (_event, trig) in regs {
            if !self.trigger(trig).is_some_and(|t| t.enabled) {
                continue;
            }
            let mut c = ctx.clone();
            c.trigger = trig;
            let saved = std::mem::replace(&mut self.ctx, c.clone());
            let pass = self.trigger_evaluate(vm, trig);
            self.ctx = saved;
            if pass {
                self.trigger_execute(vm, trig, c);
            }
        }
    }

    // ---- time ----

    /// Advances game time: timers, sleeping threads, and time-based events.
    pub fn tick(&mut self, vm: &mut Vm, dt: f32) {
        self.time += dt;
        let day_hours = self.misc_f32("DayHours", 24.0);
        let day_length = self.misc_f32("DayLength", 480.0);
        let hours_per_second = day_hours / day_length * self.time_of_day_scale;
        self.time_of_day = (self.time_of_day + dt * hours_per_second) % day_hours;

        self.update_food();
        self.update_units(vm, dt);

        // Timers.
        let mut expired = Vec::new();
        for id in self.handles.ids() {
            if let Some(Obj::Timer(t)) = self.handles.get_mut(id) {
                if !t.running {
                    continue;
                }
                t.remaining -= dt;
                if t.remaining <= 0.0 {
                    if t.periodic && t.timeout > 0.0 {
                        t.remaining += t.timeout;
                    } else {
                        t.running = false;
                        t.remaining = 0.0;
                    }
                    expired.push((id, t.callback));
                }
            }
        }
        for (timer, callback) in expired {
            let ctx = EventCtx { timer, ..Default::default() };
            if let Some(f) = callback {
                let t = vm.new_thread(f, &[], false);
                self.run_thread(vm, t, ctx.clone());
            }
            self.fire(vm, ctx, |_, k| matches!(k, EventKind::TimerExpire(t) if *t == timer));
        }

        // Sleeping threads.
        let now = self.time;
        let mut i = 0;
        while i < self.sleepers.len() {
            if self.sleepers[i].wake_at <= now {
                let s = self.sleepers.swap_remove(i);
                self.run_thread(vm, s.thread, s.ctx);
            } else {
                i += 1;
            }
        }

        // Elapsed-time game state events.
        let t = self.time;
        self.fire(vm, EventCtx::default(), |_, k| match k {
            EventKind::GameState { state: 2, op, value } => compare(*op, t, *value) && !compare(*op, t - dt, *value),
            _ => false,
        });
    }

    /// Cells blocked by a pathing texture (PathTextures\\*.tga) centered at a position.
    /// Red marks unwalkable cells; one texel is one 32x32 cell.  The texture turns with the
    /// object in quarter turns.
    pub fn pathing_cells(&mut self, path_tex: &str, x: f32, y: f32, facing_deg: f32) -> Vec<(i32, i32)> {
        if path_tex.is_empty() || path_tex == "_" || path_tex.eq_ignore_ascii_case("none") {
            return Vec::new();
        }
        let key = path_tex.to_ascii_lowercase();
        if !self.pathing_textures.contains_key(&key) {
            let img = self.chain.read(path_tex).ok().and_then(|b| w3_formats::blp::decode_texture(path_tex, &b).ok()).and_then(|v| v.into_iter().next());
            self.pathing_textures.insert(key.clone(), img);
        }
        let (Some(Some(img)), Some(p)) = (self.pathing_textures.get(&key), self.pathing.as_ref()) else { return Vec::new() };
        let (w, h) = (img.width as i32, img.height as i32);
        let quarter = ((facing_deg / 90.0).round() as i32).rem_euclid(4);
        let (cx, cy) = p.cell_of(x, y);
        let mut out = Vec::new();
        for ty in 0..h {
            for tx in 0..w {
                let i = ((ty * w + tx) * 4) as usize;
                if img.rgba[i] == 0 {
                    continue;
                }
                // Texel offsets from the center, with texture rows running north to south.
                let (ox, oy) = (tx - w / 2, (h - 1 - ty) - h / 2);
                let (rx, ry) = match quarter {
                    0 => (ox, oy),
                    1 => (-oy - 1, ox),
                    2 => (-ox - 1, -oy - 1),
                    _ => (oy, -ox - 1),
                };
                out.push((cx + rx, cy + ry));
            }
        }
        out
    }

    /// Recomputes each player's food used and food cap from their living units.
    pub fn update_food(&mut self) {
        let mut used = [0i32; PLAYER_COUNT];
        let mut made = [0i32; PLAYER_COUNT];
        for &h in &self.unit_order {
            let Some(u) = self.unit(h) else { continue };
            if u.dead {
                continue;
            }
            if let Some(o) = self.objects.units.get(u.type_id) {
                used[u.owner as usize % PLAYER_COUNT] += o.int("fused");
                if u.constructing.is_none() {
                    made[u.owner as usize % PLAYER_COUNT] += o.int("fmade");
                }
                // Queued units hold their food while training.
                for q in &u.queue {
                    used[u.owner as usize % PLAYER_COUNT] += self.objects.units.get(*q).map_or(0, |o| o.int("fused"));
                }
            }
        }
        for (i, p) in self.players.iter_mut().enumerate() {
            p.state.insert(player_state::RESOURCE_FOOD_USED, used[i]);
            let ceiling = p.get_state(player_state::FOOD_CAP_CEILING);
            let cap = if ceiling > 0 { made[i].min(ceiling) } else { made[i] };
            p.state.insert(player_state::RESOURCE_FOOD_CAP, cap);
        }
    }

    pub fn warn_unimplemented(&mut self, native: u32) {
        if self.warned.insert(native) {
            log::warn!(
                "native {} is not implemented yet",
                self.native_names.get(native as usize).map_or("?", |s| s.as_str())
            );
        }
    }
}

/// LIMIT_OP comparison (common.j: LESS_THAN = 0 ... NOT_EQUAL = 5).
pub fn compare(op: i32, a: f32, b: f32) -> bool {
    match op {
        0 => a < b,
        1 => a <= b,
        2 => a == b,
        3 => a >= b,
        4 => a > b,
        _ => a != b,
    }
}

impl Host for World {
    fn call_native(&mut self, vm: &mut Vm, native: u32, args: &[Value]) -> NativeResult {
        log::trace!("{}{:?}", self.native_names.get(native as usize).map_or("?", |s| s.as_str()), args);
        match self.natives.get(native as usize).copied().flatten() {
            Some(f) => f(self, vm, args),
            None => {
                self.warn_unimplemented(native);
                let ret = vm.module.natives[native as usize].ret.clone();
                NativeResult::Return(match ret {
                    None => Value::Null,
                    Some(t) => vm.module.base_type(&t).default_value(),
                })
            }
        }
    }
}

pub fn func_of(v: &Value) -> Option<FuncId> {
    v.code()
}
