//! Items: inventories (the AInv ability), picking up and dropping, using items with
//! charges, powerups, and the bonuses held items' abilities give.

use w3_jass::Vm;

use crate::event_ids::*;
use crate::objects::{Id, id_from_str};
use crate::types::*;
use crate::world::World;

impl World {
    pub fn item(&self, h: u32) -> Option<&Item> {
        match self.handles.get(h) {
            Some(Obj::Item(i)) => Some(i),
            _ => None,
        }
    }

    pub fn item_mut(&mut self, h: u32) -> Option<&mut Item> {
        match self.handles.get_mut(h) {
            Some(Obj::Item(i)) => Some(i),
            _ => None,
        }
    }

    /// Inventory slots a unit has: AInv's Item Capacity (DataA).
    pub fn inventory_size(&self, h: u32) -> usize {
        self.ability_with_code(h, "AInv").map_or(0, |(a, l)| self.objects.abilities.get(a).map_or(0.0, |o| o.level_real("DataA", l)) as usize).min(6)
    }

    /// Fires a unit event and the matching player-unit event.
    pub fn fire_unit_pair(&mut self, vm: &mut Vm, unit: u32, player_event: i32, unit_event: i32, mut ctx: EventCtx) {
        let owner = self.unit(unit).map_or(0, |u| u.owner);
        ctx.unit = unit;
        ctx.player = owner;
        ctx.has_player = true;
        ctx.event_id = player_event;
        self.fire(vm, ctx.clone(), move |_, k| matches!(k, EventKind::PlayerUnit { player, event, .. } if *player == owner && *event == player_event));
        ctx.event_id = unit_event;
        self.fire(vm, ctx, move |_, k| matches!(k, EventKind::Unit { unit: u, event } if *u == unit && *event == unit_event));
    }

    /// The abilities an item grants (its abilList).
    pub fn item_abilities(&self, type_id: Id) -> Vec<Id> {
        self.objects.items.get(type_id).map(|o| o.list("abilList").iter().map(|s| id_from_str(s.trim())).filter(|a| a[0] != 0).collect()).unwrap_or_default()
    }

    /// Puts an item into a unit's inventory: a powerup is used at once instead.
    /// Returns whether the unit took it.
    pub fn give_item(&mut self, vm: &mut Vm, unit: u32, item: u32, slot: Option<usize>) -> bool {
        let Some(it) = self.item(item) else { return false };
        let type_id = it.type_id;
        let size = self.inventory_size(unit);
        if size == 0 || self.unit(unit).is_none_or(|u| u.dead) {
            return false;
        }
        let powerup = self.objects.items.get(type_id).is_some_and(|o| o.bool("powerup"));
        if powerup {
            self.item_effect(vm, unit, item, type_id, 0, 0.0, 0.0);
            let ctx = EventCtx { item, ..Default::default() };
            self.fire_unit_pair(vm, unit, EVENT_PLAYER_UNIT_PICKUP_ITEM, EVENT_UNIT_PICKUP_ITEM, ctx.clone());
            self.fire_unit_pair(vm, unit, EVENT_PLAYER_UNIT_USE_ITEM, EVENT_UNIT_USE_ITEM, ctx);
            self.remove_item(item);
            return true;
        }
        let u = self.unit(unit).unwrap();
        let free = match slot {
            Some(s) if s < size && u.inventory[s] == 0 => Some(s),
            _ => (0..size).find(|&s| u.inventory[s] == 0),
        };
        let Some(s) = free else { return false };
        let owner = u.owner;
        let u = self.unit_mut(unit).unwrap();
        u.inventory[s] = item;
        let it = self.item_mut(item).unwrap();
        it.owner_unit = unit;
        it.player = owner;
        it.visible = false;
        self.apply_item_bonuses(unit);
        let ctx = EventCtx { item, ..Default::default() };
        self.fire_unit_pair(vm, unit, EVENT_PLAYER_UNIT_PICKUP_ITEM, EVENT_UNIT_PICKUP_ITEM, ctx);
        true
    }

    /// Takes an item out of its holder's inventory and puts it on the ground.
    pub fn drop_item(&mut self, vm: &mut Vm, item: u32, x: f32, y: f32) {
        let Some(holder) = self.item(item).map(|i| i.owner_unit).filter(|&u| u != 0) else { return };
        if let Some(u) = self.unit_mut(holder) {
            for s in u.inventory.iter_mut() {
                if *s == item {
                    *s = 0;
                }
            }
        }
        if let Some(it) = self.item_mut(item) {
            it.owner_unit = 0;
            it.visible = true;
            it.x = x;
            it.y = y;
        }
        self.apply_item_bonuses(holder);
        let ctx = EventCtx { item, ..Default::default() };
        self.fire_unit_pair(vm, holder, EVENT_PLAYER_UNIT_DROP_ITEM, EVENT_UNIT_DROP_ITEM, ctx);
    }

    pub fn remove_item(&mut self, item: u32) {
        let holder = self.item(item).map_or(0, |i| i.owner_unit);
        if let Some(u) = self.unit_mut(holder) {
            for s in u.inventory.iter_mut() {
                if *s == item {
                    *s = 0;
                }
            }
        }
        self.handles.remove(item);
        self.item_order.retain(|&i| i != item);
        if holder != 0 {
            self.apply_item_bonuses(holder);
        }
    }

    /// A dying unit drops what it carries when AInv says so (Drop Items On Death, DataB).
    pub fn drop_on_death(&mut self, vm: &mut Vm, unit: u32) {
        let drops = self.ability_with_code(unit, "AInv").is_some_and(|(a, l)| self.objects.abilities.get(a).is_some_and(|o| o.level_real("DataB", l) != 0.0));
        let Some(u) = self.unit(unit) else { return };
        let (x, y) = (u.x, u.y);
        let items: Vec<u32> = u.inventory.iter().copied().filter(|&i| i != 0).collect();
        for (k, it) in items.into_iter().enumerate() {
            let keep = self.item(it).is_some_and(|i| i.drop_on_death) || drops;
            if keep {
                let a = k as f32 * std::f32::consts::TAU / 6.0;
                self.drop_item(vm, it, x + 40.0 * a.cos(), y + 40.0 * a.sin());
            }
        }
    }

    /// Uses the item in a slot: its ability's effect, then a charge if it has any.
    pub fn use_item(&mut self, vm: &mut Vm, unit: u32, slot: usize, target: u32, x: f32, y: f32) -> bool {
        let Some(item) = self.unit(unit).and_then(|u| u.inventory.get(slot).copied()).filter(|&i| i != 0) else { return false };
        let Some(type_id) = self.item(item).map(|i| i.type_id) else { return false };
        let usable = self.objects.items.get(type_id).is_some_and(|o| o.bool("usable"));
        if !usable {
            return false;
        }
        if !self.item_effect(vm, unit, item, type_id, target, x, y) {
            return false;
        }
        let ctx = EventCtx { item, ..Default::default() };
        self.fire_unit_pair(vm, unit, EVENT_PLAYER_UNIT_USE_ITEM, EVENT_UNIT_USE_ITEM, ctx);
        // Charged items spend one; perishable ones vanish when out.
        let perishable = self.objects.items.get(type_id).is_some_and(|o| o.bool("perishable"));
        if let Some(it) = self.item_mut(item) {
            if it.charges > 0 {
                it.charges -= 1;
                if it.charges == 0 && perishable {
                    self.remove_item(item);
                }
            }
        }
        true
    }

    /// What an item's abilities do when used (or picked up, for powerups).  Returns
    /// whether anything happened.
    fn item_effect(&mut self, vm: &mut Vm, unit: u32, _item: u32, type_id: Id, _target: u32, _x: f32, _y: f32) -> bool {
        let owner = self.unit(unit).map_or(0, |u| u.owner);
        let mut did = false;
        for a in self.item_abilities(type_id) {
            let code = self.ability_code(a);
            let data = |w: &World, f: &str| w.objects.abilities.get(a).map_or(0.0, |o| o.level_real(f, 1));
            let area = data(self, "Area");
            match code.as_str() {
                // Healing and mana potions, and restoration.
                "AIhe" => {
                    let v = data(self, "DataA");
                    if let Some(u) = self.unit_mut(unit) {
                        u.life = (u.life + v).min(u.max_life);
                    }
                    did = true;
                }
                "AIma" | "AImr" => {
                    let v = data(self, "DataA");
                    if let Some(u) = self.unit_mut(unit) {
                        u.mana = (u.mana + v).min(u.max_mana);
                    }
                    did = true;
                }
                "AIre" => {
                    let (hp, mp) = (data(self, "DataA"), data(self, "DataB"));
                    if let Some(u) = self.unit_mut(unit) {
                        u.life = (u.life + hp).min(u.max_life);
                        u.mana = (u.mana + mp).min(u.max_mana);
                    }
                    did = true;
                }
                // Area restoration: allies around the user.
                "AIha" | "AIra" => {
                    let (hp, mp) = (data(self, "DataA"), if code == "AIra" { data(self, "DataB") } else { 0.0 });
                    let (ux, uy) = self.unit(unit).map_or((0.0, 0.0), |u| (u.x, u.y));
                    let allies: Vec<u32> = self.unit_order.iter().copied().filter(|&h| self.unit(h).is_some_and(|t| !t.dead && (t.x - ux).hypot(t.y - uy) <= area && crate::natives::players::is_ally(self, owner, t.owner))).collect();
                    for h in allies {
                        if let Some(t) = self.unit_mut(h) {
                            t.life = (t.life + hp).min(t.max_life);
                            t.mana = (t.mana + mp).min(t.max_mana);
                        }
                    }
                    did = true;
                }
                // Tomes: permanent attribute gains.
                "AIsm" | "AIam" | "AIim" | "AIxm" | "AIab" => {
                    let (agi, int, str_) = (data(self, "DataA"), data(self, "DataB"), data(self, "DataC"));
                    if let Some(u) = self.unit_mut(unit).filter(|u| u.is_hero) {
                        u.agi += agi;
                        u.int += int;
                        u.str_ += str_;
                    }
                    self.apply_hero_stats(unit, false);
                    did = true;
                }
                "AIem" => {
                    let xp = data(self, "DataA") as i32;
                    self.add_hero_xp(unit, xp);
                    did = true;
                }
                "AIlm" => {
                    let levels = data(self, "DataA") as i32;
                    for _ in 0..levels {
                        let need = self.unit(unit).map_or(0, |u| self.hero_xp_needed(u.level + 1) - u.hero_xp);
                        self.add_hero_xp(unit, need.max(1));
                    }
                    did = true;
                }
                // Coins and lumber bundles.
                "AIgo" => {
                    let g = data(self, "DataA") as i32;
                    *self.players[owner as usize].state.entry(player_state::RESOURCE_GOLD).or_insert(0) += g;
                    did = true;
                }
                "AIlu" => {
                    let l = data(self, "DataA") as i32;
                    *self.players[owner as usize].state.entry(player_state::RESOURCE_LUMBER).or_insert(0) += l;
                    did = true;
                }
                _ => {}
            }
        }
        let _ = vm;
        did
    }

    /// Recomputes what held items add: attributes, life, mana, armor, damage, speed.
    pub fn apply_item_bonuses(&mut self, unit: u32) {
        let Some(u) = self.unit(unit) else { return };
        let items: Vec<Id> = u.inventory.iter().filter_map(|&i| self.item(i).map(|i| i.type_id)).collect();
        let mut b = ItemBonus::default();
        for t in items {
            for a in self.item_abilities(t) {
                let d = |f: &str| self.objects.abilities.get(a).map_or(0.0, |o| o.level_real(f, 1));
                match self.ability_code(a).as_str() {
                    "AIat" => b.damage += d("DataA"),
                    "AIde" => b.armor += d("DataA"),
                    "AIms" => b.move_speed += d("DataA"),
                    "AIas" => b.attack_speed += d("DataA"),
                    "AIml" | "AImi" => b.life += d("DataA"),
                    "AImm" => b.mana += d("DataA"),
                    "AIrm" => b.mana_regen += d("DataA"),
                    // Attribute items (not tomes, which are powerups).
                    "AIsm" | "AIam" | "AIim" | "AIxm" | "AIab" => {
                        b.agi += d("DataA");
                        b.int += d("DataB");
                        b.str_ += d("DataC");
                    }
                    _ => {}
                }
            }
        }
        if let Some(u) = self.unit_mut(unit) {
            u.item_bonus = b;
        }
        if self.unit(unit).is_some_and(|u| u.is_hero) {
            self.apply_hero_stats(unit, false);
        } else {
            // Non-heroes: life and mana bonuses apply with research to the base maximum.
            self.apply_upgrades(unit);
        }
    }
}
