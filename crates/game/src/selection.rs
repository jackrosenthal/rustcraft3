//! The original's synchronized selection (CSelectionWar3, CPlayerWar3 +0x34): what each
//! player has selected as every player's game keeps it, from the selection actions in
//! turn order, for the keepalive's game module.  The units selected (at most 12, a
//! CUnitSet kept in order: a unit goes before the first of its type with a higher slot,
//! else a hero first and another last; `FUN_6f28d7c0`), the subgroups (a type's units
//! each, a hero alone, ordered by priority, type, and slot; `FUN_6f28de20`) with the one
//! selected, the 10 control groups, and the active unit (the selection's highest
//! priority, the lower type code between equals; `FUN_6f28d660`, `FUN_6f292360`).  Each
//! action first drops units gone or dead (`FUN_6f28e960`).
//!
//! Not modeled yet: the filter that drops units the player may no longer select
//! (`FUN_6f391390`), and a hero's priority beyond its type's (its owner's +0x334 less
//! its hero data's +0x138, plus one, `FUN_6f691980`).

use serde::{Deserialize, Serialize};

use crate::keepalive::Selection;
use crate::world::World;

/// A unit in a selection list: its handle and its tag (slot, birth).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Member {
    pub unit: u32,
    pub oid: (u32, u32),
}

/// One player's synchronized selection.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SyncSelection {
    pub units: Vec<Member>,
    pub subgroups: Vec<Vec<Member>>,
    pub current: Option<usize>,
    pub groups: [Vec<Member>; 10],
    pub active: Option<Member>,
}

/// A selection action as the original dispatches it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum SelOp {
    /// 0x16 (DispatchUnitSelectionModify): 1 adds, 2 removes, 3 replaces.
    Modify { mode: u8, units: Vec<u32> },
    /// 0x17 (DispatchUnitDefineControlGroup).
    DefineGroup { group: u8, units: Vec<u32> },
    /// 0x18 (DispatchUnitSelectControlGroup): 1 adds, 3 replaces.
    SelectGroup { group: u8, mode: u8 },
    /// 0x19 (DispatchUnitSelectSubGroup): the subgroup led by a unit of this type.
    Subgroup { kind: u32 },
    /// 0x1a (DispatchUnitRefreshSubGroup): the subgroups made again.
    Refresh,
}

/// A unit's code as the original compares it (+0x30, the four characters big-endian).
fn code(id: crate::objects::Id) -> u32 {
    u32::from_be_bytes(id)
}

impl World {
    fn member(&self, h: u32) -> Option<Member> {
        self.unit(h).map(|u| Member { unit: h, oid: u.extra.oid })
    }

    fn sel_code(&self, m: Member) -> u32 {
        self.unit(m.unit).map_or(0, |u| code(u.type_id))
    }

    /// A hero's type code starts with a capital (`(+0x30 >> 24) - 'A' < 26`).
    fn sel_hero(&self, m: Member) -> bool {
        self.unit(m.unit).is_some_and(|u| u.type_id[0].is_ascii_uppercase())
    }

    /// A hero that is not an illusion (`FUN_6f68bdd0`).
    fn sel_true_hero(&self, m: Member) -> bool {
        self.unit(m.unit).is_some_and(|u| u.type_id[0].is_ascii_uppercase() && u.extra.illusion.is_none())
    }

    fn sel_illusion(&self, m: Member) -> bool {
        self.unit(m.unit).is_some_and(|u| u.extra.illusion.is_some())
    }

    /// Present and alive: what each dispatch keeps (`FUN_6f28e960`).
    fn sel_live(&self, m: Member) -> bool {
        self.unit(m.unit).is_some_and(|u| !u.dead && u.extra.oid == m.oid)
    }

    /// The selection priority (`FUN_6f691980`): the type's (UnitData prio), plus one for
    /// a hero (its further term not modeled).
    fn sel_priority(&self, m: Member) -> f32 {
        let Some(u) = self.unit(m.unit) else { return 0.0 };
        let prio = self.objects.units.get(u.type_id).map_or(0.0, |o| o.real("prio"));
        if u.type_id[0].is_ascii_uppercase() { prio + 1.0 } else { prio }
    }

    /// Whether `a` stays the active unit against `b`: not of lower priority, and of a
    /// type code not above `b`'s between equals.
    fn sel_keeps(&self, a: Member, b: Member) -> bool {
        let (pa, pb) = (self.sel_priority(a), self.sel_priority(b));
        pb < pa || (pb == pa && self.sel_code(a) <= self.sel_code(b))
    }

    /// CUnitSet add without repeats, in order (`FUN_6f28d7c0` with 0, 1).
    fn set_add(&self, list: &mut Vec<Member>, m: Member) -> bool {
        if list.iter().any(|x| x.unit == m.unit) {
            return false;
        }
        let c = self.sel_code(m);
        if let Some(k) = list.iter().position(|&x| self.sel_live(x) && self.sel_code(x) == c && m.oid.0 < x.oid.0) {
            list.insert(k, m);
        } else if self.sel_hero(m) {
            list.insert(0, m);
        } else {
            list.push(m);
        }
        true
    }

    fn sync_sel(&mut self, p: u8) -> &mut SyncSelection {
        let n = p as usize + 1;
        if self.sync_selections.len() < n {
            self.sync_selections.resize_with(n, Default::default);
        }
        &mut self.sync_selections[p as usize]
    }

    /// A unit into its subgroup (`FUN_6f28de20`).
    fn subgroup_add(&mut self, p: u8, m: Member) {
        let mut s = std::mem::take(self.sync_sel(p));
        if s.subgroups.is_empty() {
            let mut g = Vec::new();
            self.set_add(&mut g, m);
            s.subgroups.push(g);
            s.current = Some(0);
        } else {
            let hero = self.sel_true_hero(m);
            let like = s.subgroups.iter().position(|g| {
                g.first().is_some_and(|&f| !hero && !self.sel_true_hero(f) && self.sel_illusion(f) == self.sel_illusion(m) && self.sel_code(f) == self.sel_code(m))
            });
            if let Some(k) = like {
                let mut g = std::mem::take(&mut s.subgroups[k]);
                self.set_add(&mut g, m);
                s.subgroups[k] = g;
            } else {
                let (pm, cm) = (self.sel_priority(m), self.sel_code(m));
                let at = s.subgroups.iter().position(|g| {
                    let Some(&f) = g.first() else { return false };
                    let pf = self.sel_priority(f);
                    pf < pm || (pf == pm && !hero && (cm < self.sel_code(f) || (cm == self.sel_code(f) && m.oid.0 < f.oid.0)))
                });
                let mut g = Vec::new();
                self.set_add(&mut g, m);
                match at {
                    Some(k) => {
                        s.subgroups.insert(k, g);
                        if let Some(c) = s.current.as_mut().filter(|c| **c >= k) {
                            *c += 1;
                        }
                    }
                    None => s.subgroups.push(g),
                }
            }
        }
        *self.sync_sel(p) = s;
    }

    /// A unit out of its subgroup, the subgroup gone when empty (`FUN_6f292610`); the
    /// first becomes selected if it was.
    fn subgroup_remove(&mut self, p: u8, m: Member) {
        let mut s = std::mem::take(self.sync_sel(p));
        let k = if self.sel_true_hero(m) {
            s.subgroups.iter().position(|g| g.first() == Some(&m))
        } else {
            s.subgroups.iter().position(|g| g.first().is_some_and(|&f| self.sel_code(f) == self.sel_code(m)) && g.contains(&m))
        };
        if let Some(k) = k {
            s.subgroups[k].retain(|x| x.unit != m.unit);
            if s.subgroups[k].is_empty() {
                s.subgroups.remove(k);
                s.current = match s.current {
                    Some(c) if c == k => (!s.subgroups.is_empty()).then_some(0),
                    Some(c) if c > k => Some(c - 1),
                    c => c,
                };
            }
        }
        *self.sync_sel(p) = s;
    }

    /// The best unit of the selection to be active.
    fn sel_best(&self, units: &[Member]) -> Option<Member> {
        units.iter().copied().filter(|&m| self.sel_live(m)).fold(None, |best, m| match best {
            Some(b) if self.sel_keeps(b, m) => Some(b),
            _ => Some(m),
        })
    }

    /// Sync add (`FUN_6f28d660`).
    fn sync_add(&mut self, p: u8, m: Member) {
        if self.sync_sel(p).units.len() >= 12 {
            return;
        }
        let mut units = std::mem::take(&mut self.sync_sel(p).units);
        let added = self.set_add(&mut units, m);
        self.sync_sel(p).units = units;
        if added {
            self.subgroup_add(p, m);
        }
        let active = self.sync_sel(p).active;
        if !active.is_some_and(|a| self.sel_live(a) && self.sel_keeps(a, m)) {
            self.sync_sel(p).active = Some(m);
        }
    }

    /// Sync remove (`FUN_6f292360`).
    fn sync_remove(&mut self, p: u8, m: Member) {
        if !self.sync_sel(p).units.iter().any(|x| x.unit == m.unit) {
            return;
        }
        self.subgroup_remove(p, m);
        let s = self.sync_sel(p);
        s.units.retain(|x| x.unit != m.unit);
        if s.active.is_some_and(|a| a.unit == m.unit) {
            let units = s.units.clone();
            self.sync_sel(p).active = self.sel_best(&units);
        }
    }

    /// Clears the selection, leaving `keep` alone selected if given (`FUN_6f28f590`).
    fn sync_clear(&mut self, p: u8, keep: Option<Member>) {
        let s = self.sync_sel(p);
        s.units.clear();
        s.subgroups.clear();
        s.current = None;
        s.active = None;
        if let Some(m) = keep {
            s.units.push(m);
            self.subgroup_add(p, m);
            self.sync_sel(p).active = Some(m);
        }
    }

    /// Drops units gone or dead, the active one chosen again if among them
    /// (`FUN_6f28e960`).
    fn sync_drop_dead(&mut self, p: u8) {
        let dead: Vec<Member> = self.sync_sel(p).units.clone().into_iter().filter(|&m| !self.sel_live(m)).collect();
        if dead.is_empty() {
            return;
        }
        let was_active = dead.iter().any(|d| self.sync_sel(p).active.is_some_and(|a| a.unit == d.unit));
        for m in dead {
            self.subgroup_remove(p, m);
            self.sync_sel(p).units.retain(|x| x.unit != m.unit);
        }
        if was_active {
            let units = self.sync_sel(p).units.clone();
            self.sync_sel(p).active = self.sel_best(&units);
        }
    }

    /// Selectable by the selection actions: alive (the original's further flags not
    /// modeled).
    fn sel_selectable(&self, m: Member) -> bool {
        self.sel_live(m)
    }

    /// A selection action of player `p`'s, in turn order.
    pub fn sync_selection(&mut self, p: u8, op: SelOp) {
        self.sync_drop_dead(p);
        match op {
            SelOp::Modify { mode, units } => {
                let members: Vec<Member> = units.iter().filter_map(|&h| self.member(h)).filter(|&m| mode == 2 || self.sel_selectable(m)).collect();
                let count = self.sync_sel(p).units.len();
                let n = match mode {
                    1 => members.len().min(12 - count.min(12)),
                    3 if members.is_empty() => {
                        self.sync_clear(p, None);
                        return;
                    }
                    _ => members.len().min(12),
                };
                let special = mode == 3 && n == 1 && self.sync_sel(p).active == Some(members[0]);
                if mode == 3 {
                    self.sync_clear(p, special.then_some(members[0]));
                }
                for &m in &members[..n] {
                    match mode {
                        1 => self.sync_add(p, m),
                        3 if !special => self.sync_add(p, m),
                        2 => self.sync_remove(p, m),
                        _ => {}
                    }
                }
            }
            SelOp::DefineGroup { group, units } => {
                let g = group as usize;
                if g >= 10 {
                    return;
                }
                let mut list = Vec::new();
                for m in units.iter().filter_map(|&h| self.member(h)).take(12) {
                    self.set_add(&mut list, m);
                }
                self.sync_sel(p).groups[g] = list;
            }
            SelOp::SelectGroup { group, mode } => {
                if group >= 10 {
                    return;
                }
                let members: Vec<Member> = self.sync_sel(p).groups[group as usize].clone().into_iter().filter(|&m| self.sel_selectable(m)).collect();
                let limit = if mode == 1 { 12 - self.sync_sel(p).units.len().min(12) } else { 12 };
                if mode == 1 && members.is_empty() {
                    return;
                }
                if mode == 3 && members.is_empty() {
                    self.sync_clear(p, None);
                    return;
                }
                let n = members.len().min(limit);
                if n == 0 {
                    return;
                }
                let special = mode == 3 && n == 1 && self.sync_sel(p).active == Some(members[0]);
                if mode == 3 {
                    self.sync_clear(p, special.then_some(members[0]));
                }
                for &m in &members[..n] {
                    if mode == 1 || (mode == 3 && !special) {
                        self.sync_add(p, m);
                    }
                }
                let s = self.sync_sel(p);
                s.current = (!s.subgroups.is_empty()).then_some(0);
            }
            SelOp::Subgroup { kind } => {
                let subgroups = self.sync_sel(p).subgroups.clone();
                let k = subgroups.iter().position(|g| g.first().is_some_and(|&f| self.sel_code(f) == kind));
                if let Some(k) = k {
                    self.sync_sel(p).current = Some(k);
                }
            }
            SelOp::Refresh => {
                let units: Vec<Member> = self.sync_sel(p).units.clone();
                let s = self.sync_sel(p);
                let lead = s.current.and_then(|c| s.subgroups.get(c)).and_then(|g| g.first().copied());
                for &m in &units {
                    self.subgroup_remove(p, m);
                }
                for &m in &units {
                    self.subgroup_add(p, m);
                }
                if let Some(lead) = lead {
                    let s = self.sync_sel(p);
                    if let Some(k) = s.subgroups.iter().position(|g| g.contains(&lead)) {
                        s.current = Some(k);
                    }
                }
            }
        }
    }

    /// Player `p`'s selection as the game module hashes it.
    pub fn sync_selection_hash(&self, p: u8) -> u32 {
        let Some(s) = self.sync_selections.get(p as usize) else { return Selection::default().hash() };
        let x = |m: &Member| m.oid.0 ^ m.oid.1;
        let mut groups: [Vec<u32>; 10] = Default::default();
        for (g, src) in groups.iter_mut().zip(&s.groups) {
            *g = src.iter().map(x).collect();
        }
        Selection {
            units: s.units.iter().map(x).collect(),
            subgroups: s.subgroups.iter().map(|g| g.iter().map(x).collect()).collect(),
            current: s.current,
            groups,
            active: s.active.as_ref().map(x),
            other: None,
        }
        .hash()
    }
}
