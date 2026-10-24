//! Shops and hero revival: buying items and hiring units with ItemData and UnitData
//! stock, the Aneu activation radius of neutral buildings, and reviving heroes at
//! altars with the MiscGame revival factors.

use crate::collections::HashMap;

use w3_jass::Vm;

use crate::event_ids::*;
use crate::objects::{Id, id_from_str};
use crate::types::*;
use crate::world::World;

/// One shop's supply of one item or unit type.
#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
pub struct Stock {
    pub count: i32,
    pub max: i32,
    /// Seconds until the next one is added.
    pub timer: f32,
    pub regen: f32,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Shops {
    /// Supplies of a shop's own goods (Sellitems, Makeitems, Sellunits).
    pub stock: HashMap<(u32, Id), Stock>,
    /// Goods scripts added (AddItemToStock, AddUnitToStock), which RemoveItemFromStock
    /// takes away again.
    pub added: HashMap<(u32, Id), Stock>,
    /// How many item and unit types shops show (SetAllItemTypeSlots, SetItemTypeSlots).
    pub item_slots: Option<i32>,
    pub unit_slots: Option<i32>,
    pub shop_item_slots: HashMap<u32, i32>,
    pub shop_unit_slots: HashMap<u32, i32>,
}

impl World {
    /// Items (Sellitems, Makeitems) and units (Sellunits) a unit sells.
    pub fn shop_goods(&self, shop: u32) -> (Vec<Id>, Vec<Id>) {
        let Some(o) = self.unit(shop).and_then(|u| self.objects.units.get(u.type_id)) else { return (vec![], vec![]) };
        let ids = |k: &str| o.list(k).iter().map(|s| id_from_str(s.trim())).filter(|i| i[0] != 0).collect::<Vec<_>>();
        let mut items = ids("Sellitems");
        items.extend(ids("Makeitems"));
        let mut units = ids("Sellunits");
        let mut added: Vec<Id> = self.shops.added.keys().filter(|(s, _)| *s == shop).map(|(_, id)| *id).collect();
        added.sort();
        for id in added {
            if self.objects.items.get(id).is_some() {
                items.push(id);
            } else {
                units.push(id);
            }
        }
        if let Some(n) = self.shops.shop_item_slots.get(&shop).copied().or(self.shops.item_slots) {
            items.truncate(n.max(0) as usize);
        }
        if let Some(n) = self.shops.shop_unit_slots.get(&shop).copied().or(self.shops.unit_slots) {
            units.truncate(n.max(0) as usize);
        }
        (items, units)
    }

    fn stock_def(&self, id: Id) -> (i32, f32, f32) {
        // ItemData or UnitBalance stockMax, stockRegen, and stockStart.
        let o = self.objects.items.get(id).or_else(|| self.objects.units.get(id));
        o.map_or((1, 0.0, 0.0), |o| (o.int("stockMax").max(1), o.real("stockRegen"), o.real("stockStart")))
    }

    /// The current stock of one good in a shop, created on first use.
    pub fn stock(&mut self, shop: u32, id: Id) -> Stock {
        if let Some(s) = self.shops.added.get(&(shop, id)) {
            return *s;
        }
        if let Some(s) = self.shops.stock.get(&(shop, id)) {
            return *s;
        }
        let (max, regen, start) = self.stock_def(id);
        // A good with a start delay arrives after it; others start full.
        let s = if start > 0.0 { Stock { count: 0, max, timer: start, regen } } else { Stock { count: max, max, timer: regen, regen } };
        self.shops.stock.insert((shop, id), s);
        s
    }

    /// How many of a good a shop has now, without starting its stock.
    pub fn stock_count(&self, shop: u32, id: Id) -> i32 {
        if let Some(s) = self.shops.added.get(&(shop, id)) {
            return s.count;
        }
        match self.shops.stock.get(&(shop, id)) {
            Some(s) => s.count,
            None => {
                let (max, _, start) = self.stock_def(id);
                if start > 0.0 { 0 } else { max }
            }
        }
    }

    /// Replenishes stock over time.
    pub fn update_shops(&mut self, dt: f32) {
        for s in self.shops.stock.values_mut().chain(self.shops.added.values_mut()) {
            if s.count >= s.max {
                continue;
            }
            s.timer -= dt;
            if s.timer <= 0.0 {
                s.count += 1;
                s.timer = s.regen.max(0.01);
            }
        }
    }

    fn take_stock(&mut self, shop: u32, id: Id) {
        let s = match self.shops.added.get_mut(&(shop, id)) {
            Some(s) => s,
            None => match self.shops.stock.get_mut(&(shop, id)) {
                Some(s) => s,
                None => return,
            },
        };
        // Restocking starts over when a full supply is bought from.
        if s.count == s.max {
            s.timer = s.regen;
        }
        s.count -= 1;
    }

    /// The unit that would receive a purchase for `player`: for a neutral building, the
    /// player's nearest unit with an inventory inside its Aneu Activation Radius (an
    /// allied racial shop's Aall); for the player's own shop, its nearest such unit as
    /// well.
    pub fn shopper(&self, player: u8, shop: u32) -> Option<u32> {
        let s = self.unit(shop)?;
        // The neutral building ability (Aneu, Ane2, or Aall): its activation radius
        // (DataA) and interaction type (DataB, FUN_6f3fa620): 0 heroes, bit 1 units with
        // an inventory, bit 2 units not structures, otherwise any; never illusions.
        let neutral = self.ability_with_code(shop, "Aneu").or_else(|| self.ability_with_code(shop, "Ane2")).or_else(|| self.ability_with_code(shop, "Aall"));
        let data = |k: &str| neutral.and_then(|(a, l)| self.objects.abilities.get(a).map(|o| o.level_real(k, l)));
        let radius = data("DataA").filter(|&r| r > 0.0).unwrap_or(500.0);
        let kind = data("DataB").map_or(1, |v| v as i32);
        let (sx, sy) = (s.x, s.y);
        self.unit_order
            .iter()
            .copied()
            .filter(|&h| self.unit(h).is_some_and(|u| u.owner == player && !u.dead && u.extra.illusion.is_none() && (u.x - sx).hypot(u.y - sy) <= radius))
            .filter(|&h| {
                let Some(u) = self.unit(h) else { return false };
                (kind != 0 || u.is_hero) && (kind & 1 == 0 || self.inventory_size(h) > 0) && (kind & 2 == 0 || !u.is_building)
            })
            .min_by(|&a, &b| {
                let d = |h: u32| self.unit(h).map_or(f32::MAX, |u| (u.x - sx).hypot(u.y - sy));
                d(a).total_cmp(&d(b))
            })
    }

    /// Buys an item for a player from a shop.
    pub fn buy_item(&mut self, vm: &mut Vm, player: u8, shop: u32, id: Id) -> Result<(), &'static str> {
        let Some(buyer) = self.shopper(player, shop) else { return Err("Notinrange") };
        if self.stock(shop, id).count <= 0 {
            return Err("");
        }
        let (g, l) = self.objects.items.get(id).map_or((0, 0), |o| (o.int("goldcost"), o.int("lumbercost")));
        let p = &self.players[player as usize];
        if p.get_state(player_state::RESOURCE_GOLD) < g {
            return Err("Nogold");
        }
        if p.get_state(player_state::RESOURCE_LUMBER) < l {
            return Err("Nolumber");
        }
        let p = &mut self.players[player as usize];
        *p.state.entry(player_state::RESOURCE_GOLD).or_insert(0) -= g;
        *p.state.entry(player_state::RESOURCE_LUMBER).or_insert(0) -= l;
        self.take_stock(shop, id);
        let (x, y) = self.unit(buyer).map_or((0.0, 0.0), |u| (u.x, u.y));
        let item = crate::natives::objects::create_item(self, id, x, y);
        // A full inventory leaves the item on the ground at the buyer.
        let _ = self.give_item(vm, buyer, item, None);
        let ctx = EventCtx { unit: shop, item, other_unit: buyer, ..Default::default() };
        self.fire_unit_pair(vm, shop, EVENT_PLAYER_UNIT_SELL_ITEM, EVENT_UNIT_SELL_ITEM, ctx);
        Ok(())
    }

    /// Hires a unit (a tavern hero or mercenary) for a player.
    pub fn hire_unit(&mut self, vm: &mut Vm, player: u8, shop: u32, id: Id) -> Result<(), &'static str> {
        if self.shopper(player, shop).is_none() && self.unit(shop).is_none_or(|u| u.owner != player) {
            return Err("Notinrange");
        }
        if self.stock(shop, id).count <= 0 {
            return Err("");
        }
        self.spend(player, id, true)?;
        self.take_stock(shop, id);
        let radius = self.objects.units.get(id).map_or(16.0, |o| o.real("collision"));
        let (x, y) = self.spawn_point_pub(shop, radius);
        let u = crate::natives::units::create_unit(self, player, id, x, y, 270.0);
        if !self.unit(u).is_some_and(|u| u.is_hero) {
            self.score_mercenary(player);
        }
        let ctx = EventCtx { unit: shop, other_unit: u, ..Default::default() };
        self.fire_unit_pair(vm, shop, EVENT_PLAYER_UNIT_SELL, EVENT_UNIT_SELL, ctx);
        Ok(())
    }

    /// Gold, lumber, and seconds to revive a hero: its cost times ReviveBaseFactor plus
    /// ReviveLevelFactor per level past the first, capped by ReviveMaxFactor, rounded
    /// to a multiple of 5 and capped by the HeroMaxRevive* limits (FUN_6f3f99b0); time is
    /// its build time times level times ReviveTimeFactor.
    pub fn revive_cost(&self, hero: u32) -> (i32, i32, f32) {
        let Some(u) = self.unit(hero) else { return (0, 0, 0.0) };
        let Some(o) = self.objects.units.get(u.type_id) else { return (0, 0, 0.0) };
        let level = u.level.max(1) as f32;
        let m = |k: &str, d: f32| self.misc_f32(k, d);
        let (gold, lumber) = self.hero_cost(hero, "Revive", "HeroMaxRevive");
        let time = (o.real("bldtm") * level * m("ReviveTimeFactor", 0.65)).min(m("HeroMaxReviveTime", 150.0));
        (gold, lumber, time)
    }

    /// A revival's or awakening's gold and lumber: the hero's cost times
    /// <kind>BaseFactor plus <kind>LevelFactor per level past the first (and the lumber
    /// factors), at most <kind>MaxFactor, plus a half, cut to a multiple of 5, and at
    /// most the <max>CostGold and <max>CostLumber limits.
    fn hero_cost(&self, hero: u32, kind: &str, max: &str) -> (i32, i32) {
        let Some(u) = self.unit(hero) else { return (0, 0) };
        let Some(o) = self.objects.units.get(u.type_id) else { return (0, 0) };
        let level = u.level.max(1) as f32;
        let m = |k: &str| self.misc_f32(&format!("{kind}{k}"), 0.0);
        let limit = |k: &str| self.misc_f32(&format!("{max}{k}"), 0.0) as i32;
        let fmax = m("MaxFactor");
        let gf = (m("BaseFactor") + m("LevelFactor") * (level - 1.0)).min(fmax);
        let lf = (m("BaseLumberFactor") + m("LumberLevelFactor") * (level - 1.0)).min(fmax);
        let round = |cost: f32, f: f32| ((cost * f + 0.5) / 5.0) as i32 * 5;
        (round(o.real("goldcost"), gf).min(limit("CostGold")), round(o.real("lumbercost"), lf).min(limit("CostLumber")))
    }

    /// The heroes a tavern's Awaken (CAbilityAwaken) lists for a player: its first four.
    pub fn awaken_heroes(&self, player: u8) -> Vec<u32> {
        self.unit_order.iter().copied().filter(|&h| self.unit(h).is_some_and(|u| u.is_hero && u.owner == player)).take(4).collect()
    }

    /// Whether a unit awakens heroes (has Awaken, Aawa).
    pub fn awakens(&self, h: u32) -> bool {
        self.unit(h).is_some_and(|u| u.abilities.iter().any(|&(a, _)| self.ability_code(a) == "Aawa"))
    }

    /// Gold and lumber to awaken a hero (FUN_6f4187a0, FUN_6f421e80).
    pub fn awaken_cost(&self, hero: u32) -> (i32, i32) {
        self.hero_cost(hero, "Awaken", "HeroMaxAwaken")
    }

    /// A tavern awakens a player's dead hero at once (CAbilityAwaken::vf190 and
    /// FUN_6f3f8350): within reach of the tavern, with the gold, lumber, and food, it
    /// stands beside it with HeroAwakenLifeFactor of its life and, with
    /// HeroAwakenManaStart 0, HeroAwakenManaFactor of its mana.
    pub fn awaken(&mut self, player: u8, tavern: u32, hero: u32) -> Result<(), &'static str> {
        if !self.awakens(tavern) || !self.awaken_heroes(player).contains(&hero) {
            return Err("");
        }
        if self.shopper(player, tavern).is_none() && self.unit(tavern).is_none_or(|u| u.owner != player) {
            return Err("Notinrange");
        }
        if self.unit(hero).is_none_or(|u| !u.dead || u.reviving) {
            return Err("");
        }
        let (g, l) = self.awaken_cost(hero);
        let food = self.unit(hero).and_then(|u| self.objects.units.get(u.type_id)).map_or(0, |o| o.int("fused"));
        let p = &self.players[player as usize];
        if p.get_state(player_state::RESOURCE_GOLD) < g {
            return Err("Nogold");
        }
        if p.get_state(player_state::RESOURCE_LUMBER) < l {
            return Err("Nolumber");
        }
        if food > 0 && !self.cheat(crate::cheats::flag::FOOD) && p.get_state(player_state::RESOURCE_FOOD_USED) + food > p.get_state(player_state::RESOURCE_FOOD_CAP) {
            return Err("Nofood");
        }
        let p = &mut self.players[player as usize];
        *p.state.entry(player_state::RESOURCE_GOLD).or_insert(0) -= g;
        *p.state.entry(player_state::RESOURCE_LUMBER).or_insert(0) -= l;
        let radius = self.unit(hero).map_or(16.0, |u| u.collision);
        let (x, y) = self.spawn_point_pub(tavern, radius);
        self.revive_hero(hero, x, y);
        self.requests.push(crate::world::Request::UnitReady { unit: hero });
        let life_f = self.misc_f32("HeroAwakenLifeFactor", 0.5);
        let mana_start = self.misc_f32("HeroAwakenManaStart", 0.0);
        let mana_f = self.misc_f32("HeroAwakenManaFactor", 0.0);
        if let Some(u) = self.unit_mut(hero) {
            u.life = u.max_life * life_f;
            if mana_start == 0.0 {
                u.mana = u.max_mana * mana_f;
            }
        }
        Ok(())
    }

    /// Dead heroes an altar (a structure whose type has Revive) can bring back for its
    /// owner.
    pub fn revivable(&self, altar: u32) -> Vec<u32> {
        let Some(a) = self.unit(altar) else { return vec![] };
        if a.constructing.is_some() || !self.objects.units.get(a.type_id).is_some_and(|o| o.bool("Revive")) {
            return vec![];
        }
        let owner = a.owner;
        self.unit_order.iter().copied().filter(|&h| self.unit(h).is_some_and(|u| u.is_hero && u.dead && u.owner == owner && !u.reviving)).collect()
    }

    pub fn start_revive(&mut self, vm: &mut Vm, altar: u32, hero: u32) -> Result<(), &'static str> {
        if !self.revivable(altar).contains(&hero) {
            return Err("");
        }
        if self.unit(altar).is_some_and(|a| a.revive.is_some() || !a.queue.is_empty()) {
            return Err("");
        }
        let owner = self.unit(altar).unwrap().owner;
        let (g, l, t) = self.revive_cost(hero);
        let p = &self.players[owner as usize];
        if p.get_state(player_state::RESOURCE_GOLD) < g {
            return Err("Nogold");
        }
        if p.get_state(player_state::RESOURCE_LUMBER) < l {
            return Err("Nolumber");
        }
        let p = &mut self.players[owner as usize];
        *p.state.entry(player_state::RESOURCE_GOLD).or_insert(0) -= g;
        *p.state.entry(player_state::RESOURCE_LUMBER).or_insert(0) -= l;
        self.unit_mut(altar).unwrap().revive = Some((hero, 0.0, t.max(0.01)));
        self.unit_mut(hero).unwrap().reviving = true;
        let ctx = EventCtx { unit: hero, ..Default::default() };
        self.fire_unit_pair(vm, hero, EVENT_PLAYER_HERO_REVIVE_START, EVENT_UNIT_HERO_REVIVE_START, ctx);
        Ok(())
    }

    /// Advances an altar's revival; the hero returns beside it with HeroRevive* life
    /// and mana.
    pub fn update_revive(&mut self, vm: &mut Vm, altar: u32, dt: f32) {
        let Some((hero, done, total)) = self.unit(altar).and_then(|a| a.revive) else { return };
        let done = done + dt;
        if done < total {
            self.unit_mut(altar).unwrap().revive = Some((hero, done, total));
            return;
        }
        self.unit_mut(altar).unwrap().revive = None;
        let radius = self.unit(hero).map_or(16.0, |u| u.collision);
        let (x, y) = self.spawn_point_pub(altar, radius);
        self.revive_hero(hero, x, y);
        self.requests.push(crate::world::Request::UnitReady { unit: hero });
        let ctx = EventCtx { unit: hero, ..Default::default() };
        self.fire_unit_pair(vm, hero, EVENT_PLAYER_HERO_REVIVE_FINISH, EVENT_UNIT_HERO_REVIVE_FINISH, ctx);
    }

    /// Brings a dead hero back at a point (ReviveHero and altars).
    pub fn revive_hero(&mut self, hero: u32, x: f32, y: f32) {
        let life_f = self.misc_f32("HeroReviveLifeFactor", 1.0);
        let mana_start = self.misc_f32("HeroReviveManaStart", 1.0);
        let mana_f = self.misc_f32("HeroReviveManaFactor", 0.0);
        let Some(u) = self.unit_mut(hero) else { return };
        u.dead = false;
        u.extra.decaying = false;
        u.reviving = false;
        u.hidden = false;
        u.x = x;
        u.y = y;
        u.life = u.max_life * life_f;
        // HeroReviveManaStart 1 restores the factor of maximum mana; else mana stays.
        if mana_start != 0.0 {
            u.mana = u.max_mana * mana_f;
        }
        u.action = Action::Idle;
        u.anim = "stand".into();
        u.anim_serial = u.anim_serial.wrapping_add(1);
        if !self.unit_order.contains(&hero) {
            self.unit_order.push(hero);
        }
        self.update_food();
    }
}
