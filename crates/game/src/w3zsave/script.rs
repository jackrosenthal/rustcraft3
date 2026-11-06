//! The script's objects the original saves that the port names by their handles:
//! regions and their enter and leave registrations, trigger conditions, chat
//! registrations, leaderboards, camera setups, and a computer's guard posts (read
//! against Mafarazzo TD's at 30 seconds, `w3zdiff`).

use w3_formats::w3z::{Field, First, Handlers, Link, Ref, Second, Tagged, Timing};

use super::{agent_first, game_handlers, listed_property_base, rf, NONE};
use crate::codes;
use crate::types::{EventKind, Obj};
use crate::world::World;

/// A camera setup's reals (CCameraSetup, `FUN_6f2477e0` by CameraSetupSetField's
/// field): its eight values, then their transitions' durations, then its place's,
/// seventeen properties in the second table.
pub(super) const CAMERA_SETUP_REALS: u32 = 17;

/// The second table's first slot after the camera's objects (`World::camera_records`,
/// 1077 to 1095).
const AFTER_CAMERA: u32 = 1096;

/// The camera setup's eight values by the order its properties are made, each its
/// CameraSetupSetField field: the distance, the far clip, the near clip (7), the field
/// of view, the roll, the rotation, the angle of attack, the height offset.  (Read: the
/// set ones in Mafarazzo TD's two; the roll's and height offset's places, both 0,
/// provisional.)
pub(crate) const CAMERA_SETUP_FIELDS: [i32; 8] = [0, 1, 7, 3, 4, 5, 2, 6];

impl World {
    /// The regions (CTriggerRegion, `+agr`, `FUN_6f05fea0`: no own fields, the agent
    /// base's handlers without the 1024 lists; the lists of events 0x401900c8 and
    /// 0x401900c9, its enter and leave registrations, each with its event; its pathing
    /// region), their pathing regions (`^pmr` on the pathing map: its region its owner,
    /// no kind, its bounds none, the cells listing it, 0x10000000; listed in each of its
    /// cells, `saved_pathing_cells`), and their registrations (CRegionEventReg, `+rev`,
    /// `FUN_6f27eb00`: the base's words, its region, its filter, its timer and queue of
    /// units met but not yet told, `FUN_6f277bd0`, none between turns, and the lists of
    /// events 0x80205 and 0x80206, its trigger on the one it hears).
    pub(super) fn region_records(&self) -> Vec<(u32, First, Option<Second>)> {
        let mut out = Vec::new();
        let game = self.game_object();
        let pathing = self.pathing_map_objects()[1];
        for id in self.handles.ids() {
            let Some(Obj::Region(g)) = self.handles.get(id) else { continue };
            let Some([agr, pmr]) = g.objects else { continue };
            let mut lists = [Vec::new(), Vec::new()];
            for &e in &self.events {
                let Some(Obj::Event(r)) = self.handles.get(e) else { continue };
                let (region, enter) = match r.kind {
                    EventKind::EnterRegion { region, .. } => (region, true),
                    EventKind::LeaveRegion { region, .. } => (region, false),
                    _ => continue,
                };
                if region != id {
                    continue;
                }
                let Some(&reg) = self.tags.handle_ids.get(&e) else { continue };
                let (k, word) = if enter { (0, 0x4019_00c8) } else { (1, 0x4019_00c9) };
                lists[k].push(vec![Field::Ref(rf(reg)), Field::Word(word)]);
            }
            let [enter, leave] = lists;
            let handlers = Handlers { events: Vec::new(), ..game_handlers(game) };
            out.push((agr.0, agent_first(agr, codes::class::REGION, &[]), Some(Second::Agent { own: Vec::new(), handlers, tail: vec![Field::List(enter), Field::List(leave), Field::Ref(rf(pmr))] })));
            let cells = self.region_pathing_cells(g).len() as u32;
            out.push((pmr.0, First::Region { refs: [rf(pathing), rf(agr)], word: 0, bounds: [u32::MAX; 4], words: [0, cells, 0x1000_0000] }, None));
        }
        out
    }

    /// A region's cells on the pathing map (its cells of 32 from the map's origin), those
    /// inside the map.
    pub(crate) fn region_pathing_cells(&self, g: &crate::types::Region) -> Vec<(u32, u32)> {
        let Some(p) = self.pathing.as_ref() else { return Vec::new() };
        let (ox, oy) = ((p.origin[0] / 32.0).floor() as i64, (p.origin[1] / 32.0).floor() as i64);
        let mut cells: Vec<(u32, u32)> = g.cells.iter().map(|&(x, y)| (x as i64 - ox, y as i64 - oy)).filter(|&(x, y)| x >= 0 && y >= 0 && x < p.width as i64 && y < p.height as i64).map(|(x, y)| (x as u32, y as u32)).collect();
        cells.sort_by_key(|&(x, y)| (y, x));
        cells
    }

    /// A region registration's record (`+rev`): `reg` its own, the region's and the
    /// filter's objects, and its trigger, enter or leave.
    pub(super) fn region_event_record(&self, own: (u32, u32), region: u32, filter: Option<u32>, trig: (u32, u32), enter: bool) -> Option<(u32, First, Option<Second>)> {
        let Some(Obj::Region(g)) = self.handles.get(region) else { return None };
        let agr = g.objects?[0];
        let filter = filter.and_then(|f| self.tags.handle_slots.get(&f)).map_or(NONE, |&s| rf(self.tagged(s)));
        let list = |on: bool, e: u32| Field::List(if on { vec![vec![Field::Ref(rf(trig)), Field::Word(e)]] } else { Vec::new() });
        let words = [0, u32::MAX, u32::MAX, 0, 0].map(Field::Word).to_vec();
        let tail = vec![Field::Ref(rf(agr)), Field::Ref(filter), Field::Timer(NONE, 0, Timing(None)), Field::List(Vec::new()), list(enter, 0x8_0205), list(!enter, 0x8_0206)];
        Some((own.0, agent_first(own, codes::class::REGION_EVENT, &[]), Some(Second::Agent { own: words, handlers: game_handlers(self.game_object()), tail })))
    }

    /// A chat registration's record (CPlayerChatEventReg, `pcvt`, `FUN_6f27e820`): the
    /// player event's words (event 0x80260, its player) and whether the text must match
    /// whole; its filter (none), its trigger on event 0x80260, its text, and its trigger
    /// on the lists of events 0x80210 and 0x80260.
    pub(super) fn chat_event_record(&self, own: (u32, u32), player: (u32, u32), text: &str, exact: bool, trig: (u32, u32)) -> (u32, First, Option<Second>) {
        let list = |e: u32| Field::List(vec![vec![Field::Ref(rf(trig)), Field::Word(e)]]);
        let words = [0, u32::MAX, u32::MAX, 0, 0, 0x8_0260, player.0, player.1, 0, 0, u32::from(exact)].map(Field::Word).to_vec();
        let tail = vec![Field::Ref(NONE), list(0x8_0260), Field::Text(text.to_string()), list(0x8_0210), list(0x8_0260)];
        (own.0, agent_first(own, codes::class::PLAYER_CHAT_EVENT, &[]), Some(Second::Agent { own: words, handlers: game_handlers(self.game_object()), tail }))
    }

    /// The trigger conditions (CTriggerCondition, `tcnd`, `FUN_6f299690`): each its
    /// expression's object (+0x24).
    pub(super) fn condition_records(&self) -> Vec<(u32, First, Option<Second>)> {
        let mut out = Vec::new();
        for id in self.handles.ids() {
            let Some(Obj::TriggerCondition { expr }) = self.handles.get(id) else { continue };
            let Some(&slot) = self.tags.handle_slots.get(&id) else { continue };
            let own = self.tagged(slot);
            let filter = self.tags.handle_slots.get(expr).map_or(NONE, |&s| rf(self.tagged(s)));
            out.push((own.0, agent_first(own, codes::class::TRIGGER_CONDITION, &[]), Some(Second::Agent { own: vec![Field::Word(0)], handlers: game_handlers(self.game_object()), tail: vec![Field::Ref(filter)] })));
        }
        out
    }

    /// A player's color as the leaderboard's items take it (`FUN_6f343df0`: ARGB), its
    /// team color's texture's (provisional: the original's table read for red alone,
    /// 0xffff0303).
    pub(crate) fn player_argb(&self, player: u8) -> u32 {
        let color = self.players.get(player as usize).map_or(player as i32, |p| p.color).clamp(0, 15);
        let path = format!("ReplaceableTextures\\TeamColor\\TeamColor{color:02}.blp");
        let rgb = self.chain.read(&path).ok().and_then(|b| w3_formats::blp::decode_texture(&path, &b).ok()).and_then(|v| v.into_iter().next()).map_or([255, 255, 255], |img| [img.rgba[0], img.rgba[1], img.rgba[2]]);
        u32::from_be_bytes([0xff, rgb[0], rgb[1], rgb[2]])
    }

    /// The leaderboards (CLeaderboardWar3, `+ldb`, `FUN_6f246530`): its frame's
    /// (`FUN_6f3cb7a0`: its size from the count on, the items, each `FUN_6f3a6b00`'s
    /// label, value, whether its label, value, and icon show, its player, and its
    /// label's and value's colors as ARGB, the player's where none was set; whether the
    /// frame shows); its size by item count (the frame's +0x194,
    /// LeaderboardSetSizeByItemCount's); whether displayed (+0x28); and its title.
    /// (Provisional: the frame shown as the board is the local player's and displayed.)
    pub(super) fn leaderboard_records(&self) -> Vec<(u32, First, Option<Second>)> {
        let mut out = Vec::new();
        for id in self.handles.ids() {
            let Some(Obj::Leaderboard(b)) = self.handles.get(id) else { continue };
            let Some(&slot) = self.tags.handle_slots.get(&id) else { continue };
            let own = self.tagged(slot);
            let mut size = 2 + 4;
            let items: Vec<Vec<Field>> = b
                .items
                .iter()
                .map(|it| {
                    size += it.label.len() + 1 + 7 * 4;
                    let pc = self.player_argb(it.player);
                    let argb = |c: Option<[u8; 4]>| c.map_or(pc, |[r, g, b, a]| u32::from_be_bytes([a, r, g, b]));
                    let w = Field::Word;
                    vec![Field::Text(it.label.clone()), w(it.value as u32), w(u32::from(it.style[0])), w(u32::from(it.style[1])), w(u32::from(it.style[2])), w(it.player as u32), w(argb(it.label_color)), w(argb(it.value_color))]
                })
                .collect();
            let shown = b.displayed && self.player_leaderboards.get(&self.local_player) == Some(&id);
            let tail = vec![Field::Word(size as u32), Field::List(items), Field::Word(u32::from(shown)), Field::Word(b.size as u32), Field::Word(u32::from(b.displayed)), Field::Text(b.label.clone())];
            out.push((own.0, agent_first(own, codes::class::LEADERBOARD, &[]), Some(Second::Agent { own: vec![Field::Word(0)], handlers: game_handlers(self.game_object()), tail })));
        }
        out
    }

    /// The leaderboard a player is shown (PlayerSetLeaderboard; CPlayerWar3 +0x2d4).
    pub(super) fn player_leaderboard_object(&self, player: u8) -> Option<(u32, u32)> {
        let h = self.player_leaderboards.get(&player)?;
        self.tags.handle_slots.get(h).map(|&s| self.tagged(s))
    }

    /// The camera setups (CCameraSetup, `+cst`, `FUN_6f246030`), in the order made: each
    /// its seventeen reals (in the second table, after the camera's: its values as set,
    /// in degrees, a value or duration of none -0, the near clip's 100 unless set), and
    /// its own record: its place and a real (0).  The first table's records, then the
    /// second's.  (Provisional: an unset value's default but the near clip's, and the
    /// place's duration, -0.)
    pub(super) fn camera_setup_records(&self) -> (Vec<(u32, First, Option<Second>)>, Vec<(Tagged, First)>) {
        let (mut first, mut second) = (Vec::new(), Vec::new());
        let mut setups: Vec<((u32, u32), &crate::types::CameraSetup)> = self.handles.ids().into_iter().filter_map(|id| match self.handles.get(id) {
            Some(Obj::CameraSetup(c)) => self.tags.handle_slots.get(&id).map(|&s| (self.tagged(s), c.as_ref())),
            _ => None,
        }).collect();
        setups.sort_by_key(|s| s.0 .1);
        let zero = |v: f32| if v == 0.0 { -0.0 } else { v };
        for (k, (own, c)) in setups.into_iter().enumerate() {
            let at = AFTER_CAMERA + CAMERA_SETUP_REALS * k as u32;
            let props: Vec<(u32, u32)> = (0..CAMERA_SETUP_REALS).map(|j| (0x8000_0000 | (at + j), at + j)).collect();
            let value = |f: i32| c.fields.get(&f).copied().unwrap_or(if f == 7 { 100.0 } else { 0.0 });
            let duration = |f: i32| c.durations.get(&f).copied().unwrap_or(0.0);
            let values: Vec<f32> = CAMERA_SETUP_FIELDS.iter().map(|&f| value(f)).chain(CAMERA_SETUP_FIELDS.iter().map(|&f| duration(f))).chain([0.0]).collect();
            for (j, v) in values.into_iter().enumerate() {
                let entry = Tagged { family: u32::from_be_bytes(*codes::class::PROPERTY_FAMILY), kind: u32::from_be_bytes(*codes::class::REAL), tag: props[j].0, birth: props[j].1 };
                second.push((entry, First::Real { base: listed_property_base(&props, j, own), values: [zero(v), 0.0, 0.0, f32::MAX] }));
            }
            let mut agent = agent_first(own, codes::class::CAMERA_SETUP, &props);
            if let First::Agent { links, .. } = &mut agent {
                links[1] = Link(rf(*props.last().unwrap_or(&own)), 0);
            }
            let tail = vec![Field::Real(c.x), Field::Real(c.y), Field::Real(0.0)];
            first.push((own.0, agent, Some(Second::Agent { own: vec![Field::Word(0)], handlers: game_handlers(self.game_object()), tail })));
        }
        (first, second)
    }

    /// The camera setups' entries in the second table.
    pub fn camera_setup_entries(&self) -> Vec<Tagged> {
        self.camera_setup_records().1.into_iter().map(|(e, _)| e).collect()
    }

    /// A computer's guard posts (CGuardPost, `AIgp`, own `FUN_6f9befe0`), the ones its
    /// units' making made: its place, the unit type, the unit keeping it, 0, the units it
    /// has had, and the post made before it (none for the first); and the newest post,
    /// which the player's AI names (CCompAI's own).
    pub(super) fn guard_post_records(&self) -> Vec<(u32, First, Option<Second>)> {
        let mut out = Vec::new();
        for p in 0..self.players.len() as u8 {
            let mut prev = NONE;
            for g in self.posts(p) {
                let Some(me) = g.object.filter(|&(s, b)| self.tags.live.get(&s).is_some_and(|e| e.1 == b)) else { continue };
                let unit = self.unit(g.unit).and_then(|u| u.extra.object_slots[0].first().copied()).map_or(NONE, rf);
                let w = Field::Word;
                let own = vec![w(0), Field::Real(g.x), Field::Real(g.y), w(u32::from_be_bytes(g.id)), w(unit.0), w(unit.1), w(0), w(g.filled as u32), w(prev.0), w(prev.1), w(0)];
                out.push((me.0, agent_first(me, codes::ability::id::GUARD_POST, &[]), Some(Second::Agent { own, handlers: Handlers { keyed: Vec::new(), fixed: Default::default(), events: vec![Vec::new(); 0x400] }, tail: Vec::new() })));
                prev = rf(me);
            }
        }
        out
    }

    /// The newest of a player's guard posts' objects (its AI's list's head).
    pub(super) fn last_guard_post(&self, player: u8) -> Ref {
        self.posts(player).iter().filter_map(|g| g.object).filter(|&(s, b)| self.tags.live.get(&s).is_some_and(|e| e.1 == b)).max_by_key(|o| o.1).map_or(NONE, rf)
    }

    /// The guard post a unit keeps (its AI's +0x1c).
    pub(super) fn unit_guard_post(&self, owner: u8, h: u32) -> Ref {
        self.posts(owner).iter().filter(|g| g.unit == h).filter_map(|g| g.object).filter(|&(s, b)| self.tags.live.get(&s).is_some_and(|e| e.1 == b)).last().map_or(NONE, rf)
    }
}
