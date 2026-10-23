//! Players' scores (GetPlayerScore and the score screen).  As game.dll keeps them: per
//! unit type, how many the player made and how many of them it killed; the most food it
//! used and made; heroes and items; and resources gathered, lost, and traded.  The unit
//! score is each type's made and killed count times its gold and lumber cost
//! (FUN_6f24b380), the hero score ten times its heroes' experience (FUN_6f2667d0), the
//! resource score gold kept after upkeep plus lumber (FUN_6f269cf0), and the total their
//! sum (FUN_6f26b1d0).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::objects::Id;
use crate::world::World;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Score {
    /// Per unit type: made by the player, and killed by it.
    pub types: HashMap<Id, (i32, i32)>,
    pub food_max_used: i32,
    pub food_max_made: i32,
    pub heroes_killed: i32,
    pub items_gained: i32,
    pub mercs_hired: i32,
    /// Gold kept after upkeep, and lost to it.
    pub gold_mined: i32,
    pub gold_lost_upkeep: i32,
    pub gold_lost_tax: i32,
    pub gold_given: i32,
    pub gold_received: i32,
    pub lumber: i32,
    pub lumber_lost_upkeep: i32,
    pub lumber_lost_tax: i32,
    pub lumber_given: i32,
    pub lumber_received: i32,
    /// The heroes the player has had, in order, and the experience they gained.
    pub heroes: Vec<u32>,
    pub experience: i32,
}

/// PLAYER_SCORE_* (common.j).
pub mod kind {
    pub const UNITS_TRAINED: i32 = 0;
    pub const UNITS_KILLED: i32 = 1;
    pub const STRUCT_BUILT: i32 = 2;
    pub const STRUCT_RAZED: i32 = 3;
    pub const TECH_PERCENT: i32 = 4;
    pub const FOOD_MAXPROD: i32 = 5;
    pub const FOOD_MAXUSED: i32 = 6;
    pub const HEROES_KILLED: i32 = 7;
    pub const ITEMS_GAINED: i32 = 8;
    pub const MERCS_HIRED: i32 = 9;
    pub const GOLD_MINED_TOTAL: i32 = 10;
    pub const GOLD_MINED_UPKEEP: i32 = 11;
    pub const GOLD_LOST_UPKEEP: i32 = 12;
    pub const GOLD_LOST_TAX: i32 = 13;
    pub const GOLD_GIVEN: i32 = 14;
    pub const GOLD_RECEIVED: i32 = 15;
    pub const LUMBER_TOTAL: i32 = 16;
    pub const LUMBER_LOST_UPKEEP: i32 = 17;
    pub const LUMBER_LOST_TAX: i32 = 18;
    pub const LUMBER_GIVEN: i32 = 19;
    pub const LUMBER_RECEIVED: i32 = 20;
    pub const UNIT_TOTAL: i32 = 21;
    pub const HERO_TOTAL: i32 = 22;
    pub const RESOURCE_TOTAL: i32 = 23;
    pub const TOTAL: i32 = 24;
}

impl World {
    fn score_mut(&mut self, player: u8) -> Option<&mut Score> {
        self.players.get_mut(player as usize).map(|p| &mut p.score)
    }

    /// A unit made for a player.
    pub fn score_made(&mut self, player: u8, type_id: Id, unit: u32, hero: bool) {
        if let Some(s) = self.score_mut(player) {
            s.types.entry(type_id).or_default().0 += 1;
            if hero && !s.heroes.contains(&unit) {
                s.heroes.push(unit);
            }
        }
    }

    /// A unit killed by another player's.
    pub fn score_killed(&mut self, killer_owner: u8, victim_owner: u8, type_id: Id, hero: bool) {
        if killer_owner == victim_owner {
            return;
        }
        if let Some(s) = self.score_mut(killer_owner) {
            s.types.entry(type_id).or_default().1 += 1;
            if hero {
                s.heroes_killed += 1;
            }
        }
    }

    pub fn score_gathered(&mut self, player: u8, gold: (i32, i32), lumber: (i32, i32)) {
        if let Some(s) = self.score_mut(player) {
            s.gold_mined += gold.0;
            s.gold_lost_upkeep += gold.1;
            s.lumber += lumber.0;
            s.lumber_lost_upkeep += lumber.1;
        }
    }

    pub fn score_food(&mut self, player: u8, used: i32, made: i32) {
        if let Some(s) = self.score_mut(player) {
            s.food_max_used = s.food_max_used.max(used);
            s.food_max_made = s.food_max_made.max(made);
        }
    }

    pub fn score_item(&mut self, player: u8) {
        if let Some(s) = self.score_mut(player) {
            s.items_gained += 1;
        }
    }

    pub fn score_mercenary(&mut self, player: u8) {
        if let Some(s) = self.score_mut(player) {
            s.mercs_hired += 1;
        }
    }

    pub fn score_experience(&mut self, player: u8, xp: i32) {
        if let Some(s) = self.score_mut(player) {
            s.experience += xp;
        }
    }

    fn type_cost(&self, id: Id) -> i32 {
        self.objects.units.get(id).map_or(0, |o| o.int("goldcost") + o.int("lumbercost"))
    }

    fn is_structure_type(&self, id: Id) -> bool {
        self.objects.units.get(id).is_some_and(|o| o.bool("isbldg"))
    }

    /// The share of its race's research a player has made (FUN_6f24b240): the levels
    /// researched of the race's used upgrades over their maximum levels (FUN_6f6a6e40).
    fn tech_percent(&self, player: u8) -> i32 {
        let Some(p) = self.players.get(player as usize) else { return 0 };
        let race = match p.race {
            1 => "human",
            2 => "orc",
            3 => "undead",
            4 => "nightelf",
            _ => return 0,
        };
        let ours = |o: &crate::objects::Object| o.str("race").eq_ignore_ascii_case(race) && o.int("used") != 0;
        let total: i32 = self.objects.upgrades.objects.values().filter(|o| ours(o)).map(|o| o.int("maxlevel").max(0)).sum();
        if total == 0 {
            return 0;
        }
        let made: i32 = p.tech_researched.iter().filter(|(id, _)| self.objects.upgrades.get(**id).is_some_and(ours)).map(|(_, &l)| l.max(0)).sum();
        100 * made / total
    }

    /// GetPlayerScore.
    pub fn player_score(&self, player: u8, which: i32) -> i32 {
        use kind::*;
        let Some(s) = self.players.get(player as usize).map(|p| &p.score) else { return 0 };
        let sum = |structures: bool, killed: bool| -> i32 {
            s.types
                .iter()
                .filter(|(id, _)| self.is_structure_type(**id) == structures)
                // Structures built count only those that cost something.
                .filter(|(id, _)| killed || !structures || self.type_cost(**id) != 0)
                .map(|(_, &(made, k))| if killed { k } else { made })
                .sum()
        };
        match which {
            UNITS_TRAINED => sum(false, false),
            UNITS_KILLED => sum(false, true),
            STRUCT_BUILT => sum(true, false),
            STRUCT_RAZED => sum(true, true),
            TECH_PERCENT => self.tech_percent(player),
            FOOD_MAXPROD => s.food_max_made,
            FOOD_MAXUSED => s.food_max_used,
            HEROES_KILLED => s.heroes_killed,
            ITEMS_GAINED => s.items_gained,
            MERCS_HIRED => s.mercs_hired,
            GOLD_MINED_TOTAL => s.gold_mined + s.gold_lost_upkeep,
            GOLD_MINED_UPKEEP => s.gold_mined,
            GOLD_LOST_UPKEEP => s.gold_lost_upkeep,
            GOLD_LOST_TAX => s.gold_lost_tax,
            GOLD_GIVEN => s.gold_given,
            GOLD_RECEIVED => s.gold_received,
            LUMBER_TOTAL => s.lumber,
            LUMBER_LOST_UPKEEP => s.lumber_lost_upkeep,
            LUMBER_LOST_TAX => s.lumber_lost_tax,
            LUMBER_GIVEN => s.lumber_given,
            LUMBER_RECEIVED => s.lumber_received,
            UNIT_TOTAL => s.types.iter().map(|(id, &(made, killed))| (made + killed) * self.type_cost(*id)).sum(),
            HERO_TOTAL => 10 * s.heroes.iter().filter_map(|&h| self.unit(h)).map(|u| u.hero_xp).sum::<i32>(),
            RESOURCE_TOTAL => s.gold_mined + s.lumber,
            TOTAL => self.player_score(player, UNIT_TOTAL) + self.player_score(player, HERO_TOTAL) + self.player_score(player, RESOURCE_TOTAL),
            _ => 0,
        }
    }
}
