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
            (o.list("Requires"), o.list("Requiresamount"))
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
        if let Some(v) = self.tech_cache.borrow().get(&(owner, req, amount)) {
            return *v;
        }
        let v = self.compute_tech_met(owner, req, amount);
        self.tech_cache.borrow_mut().insert((owner, req, amount), v);
        v
    }

    fn compute_tech_met(&self, owner: u8, req: Id, amount: i32) -> bool {
        let Some(p) = self.players.get(owner as usize) else { return false };
        if self.objects.upgrades.get(req).is_some() {
            return p.tech_researched.get(&req).copied().unwrap_or(0) >= amount.max(1);
        }
        // The type, what it upgrades into (transitively), and its DependencyOr types.
        let mut types = vec![req];
        let mut k = 0;
        while k < types.len() {
            if let Some(o) = self.objects.units.get(types[k]) {
                for next in o.list("Upgrade").iter().chain(o.list("DependencyOr").iter()) {
                    let next = id_from_str(next.trim());
                    if !types.contains(&next) {
                        types.push(next);
                    }
                }
            }
            k += 1;
        }
        let have = self
            .unit_order
            .iter()
            .filter_map(|&h| self.unit(h))
            .filter(|u| u.owner == owner && !u.dead && u.constructing.is_none() && types.contains(&u.type_id))
            .count() as i32;
        have >= amount.max(1)
    }

    /// Whether an ability works for its owner: its research is done.
    pub fn ability_enabled(&self, owner: u8, a: Id) -> bool {
        self.objects.abilities.get(a).is_none_or(|o| o.list("Requires").is_empty()) || self.missing_requirements(owner, a, 1).is_empty()
    }
}
