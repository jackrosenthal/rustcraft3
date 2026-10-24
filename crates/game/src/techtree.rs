//! Tech requirements (the Requires fields): units, structures, research levels, and
//! abilities that need other structures or research first.  A requirement on a unit
//! type is met by a finished unit of that type, of a type it upgrades into (a Keep
//! for a Town Hall), or of a type its DependencyOr lists; one on research by its
//! level (Requiresamount).

use crate::objects::{Id, id_from_str};
use crate::world::World;

impl World {
    /// What `id` requires before it can be made, researched (at research level
    /// `level`, from 1), or used: (requirement, amount) pairs.
    pub fn requirements(&self, id: Id, level: u32) -> Vec<(Id, i32)> {
        let (list, amounts) = if let Some(o) = self.objects.upgrades.get(id) {
            // Requires for the first level, Requires1 for the second, and so on.
            let key = if level <= 1 { "Requires".to_string() } else { format!("Requires{}", level - 1) };
            (o.list(&key), Vec::new())
        } else if let Some(o) = self.objects.abilities.get(id) {
            (o.list("Requires"), o.list("Requiresamount"))
        } else if let Some(o) = self.objects.units.get(id) {
            // Requires for the first level, Requires1 for the second, and so on, up to
            // Requirescount (FUN_6f344380 clamps to the last).
            let count = o.int("Requirescount").max(1) as u32;
            let k = level.max(1).min(count) - 1;
            if k == 0 { (o.list("Requires"), o.list("Requiresamount")) } else { (o.list(&format!("Requires{k}")), Vec::new()) }
        } else {
            return Vec::new();
        };
        list.iter()
            .enumerate()
            .filter_map(|(k, r)| {
                let r = r.trim();
                (!r.is_empty() && r != "_").then(|| (id_from_str(r), amounts.get(k).and_then(|a| a.trim().parse().ok()).unwrap_or(1)))
            })
            .collect()
    }

    /// The requirements `owner` does not meet for `id`.
    pub fn missing_requirements(&self, owner: u8, id: Id, level: u32) -> Vec<(Id, i32)> {
        self.requirements(id, level).into_iter().filter(|&(r, n)| !self.tech_met(owner, r, n)).collect()
    }

    /// Whether `owner` has requirement `req` (`amount` research levels or units).
    pub fn tech_met(&self, owner: u8, req: Id, amount: i32) -> bool {
        // synergy.
        if self.cheat(crate::cheats::flag::TECH_TREE) {
            return true;
        }
        if let Some(v) = self.tech_cache.borrow().get(&(owner, req, amount)) {
            return *v;
        }
        let v = self.compute_tech_met(owner, req, amount);
        self.tech_cache.borrow_mut().insert((owner, req, amount), v);
        v
    }

    fn compute_tech_met(&self, owner: u8, req: Id, amount: i32) -> bool {
        self.tech_count(owner, req, true) >= amount.max(1)
    }

    /// Research levels of `id`, or the finished units of type `id` (with the types it
    /// upgrades into and its DependencyOr types when `equivalents`).
    pub fn tech_count(&self, owner: u8, id: Id, equivalents: bool) -> i32 {
        let Some(p) = self.players.get(owner as usize) else { return 0 };
        if self.objects.upgrades.get(id).is_some() {
            return p.tech_researched.get(&id).copied().unwrap_or(0);
        }
        let types = if equivalents { self.equivalent_types(id) } else { vec![id] };
        self.unit_order
            .iter()
            .filter_map(|&h| self.unit(h))
            .filter(|u| u.owner == owner && !u.dead && u.constructing.is_none() && types.contains(&u.type_id))
            .count() as i32
    }

    /// A dependency group named in requirements (TALT, TWN1 to TWN9): a section of
    /// Units\ItemFunc.txt with its DependencyOr list and an ItemStrings Name, kept
    /// among the items under its lower-cased name.
    pub fn dependency_group(&self, id: Id) -> Option<&crate::objects::Object> {
        self.objects.items.get(id).or_else(|| self.objects.items.get(id.map(|c| c.to_ascii_lowercase())))
    }

    /// A type, what it upgrades into (transitively), and its DependencyOr types.
    fn equivalent_types(&self, req: Id) -> Vec<Id> {
        let mut types = vec![req];
        let mut k = 0;
        while k < types.len() {
            if let Some(o) = self.objects.units.get(types[k]).or_else(|| self.dependency_group(types[k])) {
                for next in o.list("Upgrade").iter().chain(o.list("DependencyOr").iter()) {
                    let next = id_from_str(next.trim());
                    if !types.contains(&next) {
                        types.push(next);
                    }
                }
            }
            k += 1;
        }
        types
    }

    /// Whether an ability works for its owner: available (SetPlayerAbilityAvailable)
    /// and its research done.
    pub fn ability_enabled(&self, owner: u8, a: Id) -> bool {
        if self.players.get(owner as usize).is_some_and(|p| p.abilities_unavailable.contains(&a)) {
            return false;
        }
        self.objects.abilities.get(a).is_none_or(|o| o.list("Requires").is_empty()) || self.missing_requirements(owner, a, 1).is_empty()
    }

    /// SetPlayerTechMaxAllowed's limit for `id` (unset is -1), plus what research adds
    /// to it (CUpgradeAddTechMaxAllowed, rtma), if any (negative is none).
    fn tech_max(&self, owner: u8, id: Id) -> Option<i32> {
        let set = self.players.get(owner as usize)?.tech_max.get(&id).copied().unwrap_or(-1);
        Some(set + self.research_tech_max(owner, id)).filter(|&m| m >= 0)
    }

    /// The player's heroes, as their limit counts them: alive, dead awaiting revival, or
    /// in training.
    pub fn hero_count(&self, owner: u8) -> i32 {
        let living = self.unit_order.iter().filter_map(|&h| self.unit(h)).filter(|u| u.owner == owner && u.is_hero).count();
        let queued: usize = self.unit_order.iter().filter_map(|&h| self.unit(h)).filter(|u| u.owner == owner).map(|u| u.queue.iter().filter(|q| q[0].is_ascii_uppercase() && self.objects.units.get(**q).is_some()).count()).sum();
        (living + queued) as i32
    }

    /// The requirement level of making `id`: for a hero, one more than the player's
    /// heroes, so the second and third take Requires1 and Requires2 (FUN_6f419b40 with
    /// the HERO count); else the first.
    pub fn requirement_level(&self, owner: u8, id: Id) -> u32 {
        if id[0].is_ascii_uppercase() && self.objects.units.get(id).is_some() {
            self.hero_count(owner) as u32 + 1
        } else {
            1
        }
    }

    /// Whether `owner` may make another `id` (or research level `level` of it) under
    /// SetPlayerTechMaxAllowed: units count those alive, being built, or in training;
    /// Heroes also count against the HERO limit.
    pub fn tech_allowed(&self, owner: u8, id: Id, level: u32) -> bool {
        if self.objects.upgrades.get(id).is_some() {
            return self.tech_max(owner, id).is_none_or(|m| level as i32 <= m);
        }
        let made = |types: &dyn Fn(Id) -> bool| -> i32 {
            let living = self.unit_order.iter().filter_map(|&h| self.unit(h)).filter(|u| u.owner == owner && !(u.dead && !u.is_hero) && types(u.type_id)).count();
            let queued: usize = self.unit_order.iter().filter_map(|&h| self.unit(h)).filter(|u| u.owner == owner).map(|u| u.queue.iter().filter(|&&q| types(q)).count()).sum();
            (living + queued) as i32
        };
        if self.tech_max(owner, id).is_some_and(|m| made(&|t| t == id) >= m) {
            return false;
        }
        let hero = id[0].is_ascii_uppercase();
        if hero {
            if let Some(m) = self.tech_max(owner, *b"HERO") {
                if made(&|t| t[0].is_ascii_uppercase()) >= m {
                    return false;
                }
            }
        }
        true
    }

    /// Hidden from the command card: SetPlayerTechMaxAllowed of 0.
    pub fn tech_hidden(&self, owner: u8, id: Id) -> bool {
        self.tech_max(owner, id) == Some(0)
    }
}
