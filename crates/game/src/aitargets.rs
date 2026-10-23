//! What a computer player attacks: its mega target (FUN_6f9c5e00), the enemy's
//! expansion (FUN_6f9c66d0), and whether a hall is towered (IsTowered).

use crate::natives::players::is_ally;
use crate::objects::{Id, id_from_str};
use crate::types::*;
use crate::world::World;

impl World {
    /// A unit's weight in the AI's tallies: a Hero (level + 1) / 2 + 4, any other its
    /// level (FUN_6f685150 and its kin).
    fn ai_strength(u: &Unit) -> i32 {
        if u.is_hero { (u.level + 1) / 2 + 4 } else { u.level }
    }

    /// Every 10 s while the player watches for mega targets (FUN_6f9c5e00): its side's
    /// strength, its own, and each enemy's are tallied, workers left out, enemy
    /// defenses counted (Watch, Guard, and Cannon Towers, Ancients of Protection, and
    /// Spirit Towers 3, Burrows 1, Halls of the Dead and Black Citadels 4), and each
    /// enemy's town hall noted.  When the enemies together are weaker than its side and
    /// it has more than 19 of its own, the first enemy with a hall and under half its
    /// strength is the mega target.
    pub(crate) fn ai_mega_tick(&mut self, p: u8, dt: f32) {
        let Some(st) = self.ai.get_mut(&p) else { return };
        st.mega_timer -= dt;
        if st.mega_timer > 0.0 {
            return;
        }
        st.mega_timer = 10.0;
        st.mega = 0;
        if !st.flags.get("watch_mega_targets").copied().unwrap_or(false) {
            return;
        }
        let (mut side, mut own, mut enemies) = (0, 0, 0);
        let mut enemy = [0i32; 12];
        let mut halls = [0u32; 12];
        let worth = |id: Id| -> Option<i32> {
            match &id {
                b"owtw" | b"hgtw" | b"etrp" | b"hctw" | b"uzg1" => Some(3),
                b"otrb" => Some(1),
                b"unp1" | b"unp2" => Some(4),
                _ => None,
            }
        };
        for &h in &self.unit_order {
            let Some(u) = self.unit(h) else { continue };
            if u.dead || u.owner > 11 || self.unit_class(u, "peon") {
                continue;
            }
            let o = u.owner as usize;
            let allied = is_ally(self, p, u.owner);
            let mut s = Self::ai_strength(u);
            if u.is_building {
                if allied {
                    continue;
                }
                if self.unit_class(u, "townhall") || u.type_id == id_from_str("ugol") {
                    halls[o] = h;
                }
                match worth(u.type_id) {
                    Some(w) => s = w,
                    None => continue,
                }
            }
            if u.owner == p {
                own += s;
                side += s;
            } else if allied {
                side += s;
            } else {
                enemy[o] += s;
                enemies += s;
            }
        }
        if enemies < side && own > 19 {
            let target = (0..12).find(|&i| i != p as usize && halls[i] != 0 && own > enemy[i] * 2).map(|i| halls[i]);
            if let (Some(t), Some(st)) = (target, self.ai.get_mut(&p)) {
                st.mega = t;
            }
        }
    }

    fn unit_class(&self, u: &Unit, class: &str) -> bool {
        self.objects.units.get(u.type_id).is_some_and(|o| o.list("type").iter().any(|t| t.trim().eq_ignore_ascii_case(class)))
    }

    /// GetMegaTarget: the mega target while it lives.
    pub(crate) fn mega_target(&self, p: u8) -> u32 {
        let t = self.ai.get(&p).map_or(0, |s| s.mega);
        if self.unit(t).is_some_and(|u| !u.dead) { t } else { 0 }
    }

    /// GetEnemyExpansion (FUN_6f9c66d0): for each enemy, of its structures within 1000
    /// of a gold mine, the one by the mine with the most gold (its town hall there, if
    /// any), when it has structures by more than one mine; of those the one the attack
    /// captain reaches soonest within a 5000 walk.
    pub(crate) fn enemy_expansion(&self, p: u8) -> u32 {
        #[derive(Clone, Copy, Default)]
        struct Rec {
            unit: u32,
            mine: u32,
            gold: i32,
            mines: i32,
        }
        let mut recs = [Rec::default(); 16];
        let mines: Vec<(u32, f32, f32, i32)> = self
            .unit_order
            .iter()
            .copied()
            .filter(|&h| self.ability_with_code(h, "Agld").is_some())
            .filter_map(|h| self.unit(h).filter(|u| !u.dead).map(|u| (h, u.x, u.y, u.resource)))
            .collect();
        for (m, mx, my, gold) in mines {
            for &h in &self.unit_order {
                let Some(u) = self.unit(h) else { continue };
                if u.dead || !u.is_building || is_ally(self, p, u.owner) || (u.owner as usize) >= 16 || (u.x - mx).hypot(u.y - my) > 1000.0 {
                    continue;
                }
                let r = &mut recs[u.owner as usize];
                if r.mine != m {
                    r.mine = m;
                    r.mines += 1;
                }
                if r.gold < gold {
                    r.gold = gold;
                    r.unit = h;
                } else if r.gold == gold && (self.unit_class(u, "townhall") || u.type_id == id_from_str("ugol")) {
                    r.unit = h;
                }
            }
        }
        let from = self.attack_position(p);
        let mut best: Option<(f32, u32)> = None;
        for r in recs.iter().filter(|r| r.mines > 1 && r.unit != 0) {
            let Some(u) = self.unit(r.unit) else { continue };
            let d = self.walk_to_pub(from, (u.x, u.y), u.collision);
            if d.is_finite() && d <= 5000.0 && best.is_none_or(|b| d < b.0) {
                best = Some((d, r.unit));
            }
        }
        best.map_or(0, |b| b.1)
    }

    /// Where the attack captain stands: amid its units, else the start.
    fn attack_position(&self, p: u8) -> (f32, f32) {
        let units: Vec<(f32, f32)> = self.ai.get(&p).map(|s| s.attack.units.iter().filter_map(|&h| self.unit(h)).filter(|u| !u.dead).map(|u| (u.x, u.y)).collect()).unwrap_or_default();
        if units.is_empty() {
            return self.start_of(p);
        }
        let n = units.len() as f32;
        (units.iter().map(|q| q.0).sum::<f32>() / n, units.iter().map(|q| q.1).sum::<f32>() / n)
    }

    /// IsTowered: more than one armed structure of the hall's owner within 1200 of it.
    pub(crate) fn is_towered(&self, hall: u32) -> bool {
        let Some(t) = self.unit(hall) else { return false };
        let (owner, x, y) = (t.owner, t.x, t.y);
        self.unit_order
            .iter()
            .filter_map(|&h| self.unit(h))
            .filter(|u| !u.dead && u.is_building && u.owner == owner && (u.x - x).hypot(u.y - y) <= 1200.0 && !crate::sim::unit_weapons(self, u).is_empty())
            .count()
            > 1
    }
}
