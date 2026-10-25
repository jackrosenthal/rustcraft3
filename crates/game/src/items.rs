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
        if let Some(owner) = self.unit(unit).map(|u| u.owner) {
            self.score_item(owner);
        }
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

    /// A unit sells an item to a shop (pawns it): its gold and lumber cost times
    /// PawnItemRate, and times the charges left of those it began with (FUN_6f65a1b0),
    /// go to the unit's owner; the item is gone, and EVENT_PLAYER_UNIT_PAWN_ITEM fires.
    pub fn pawn_item(&mut self, vm: &mut Vm, unit: u32, item: u32, shop: u32) {
        let (gold, lumber) = self.pawn_value(item);
        let Some(owner) = self.unit(unit).map(|u| u.owner) else { return };
        let ctx = EventCtx { unit, other_unit: shop, item, ..Default::default() };
        self.fire_unit_pair(vm, unit, EVENT_PLAYER_UNIT_PAWN_ITEM, EVENT_UNIT_PAWN_ITEM, ctx);
        if let Some(p) = self.players.get_mut(owner as usize) {
            *p.state.entry(crate::types::player_state::RESOURCE_GOLD).or_insert(0) += gold;
            *p.state.entry(crate::types::player_state::RESOURCE_LUMBER).or_insert(0) += lumber;
        }
        // It leaves the inventory as a dropped item does (its bonuses go), then is gone.
        let (x, y) = self.unit(unit).map(|u| (u.x, u.y)).unwrap_or_default();
        self.drop_item(vm, item, x, y);
        self.remove_item(item);
    }

    /// What selling an item brings: its gold and lumber cost times PawnItemRate, and
    /// times the charges left of those it began with (FUN_6f65a1b0, FUN_6f65a230).
    pub fn pawn_value(&self, item: u32) -> (i32, i32) {
        let Some(it) = self.item(item) else { return (0, 0) };
        let Some(o) = self.objects.items.get(it.type_id) else { return (0, 0) };
        let uses = o.int("uses");
        let mut rate = self.misc_f32("PawnItemRate", 0.5);
        if uses > 0 {
            rate *= it.charges as f32 / uses as f32;
        }
        ((o.int("goldcost") as f32 * rate) as i32, (o.int("lumbercost") as f32 * rate) as i32)
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

    /// What the item in a slot is used on: its first ability's targeting.
    pub fn item_targeting(&self, unit: u32, slot: usize) -> crate::ability::Targeting {
        let t = self.unit(unit).and_then(|u| u.inventory.get(slot).copied()).and_then(|i| self.item(i)).map(|i| i.type_id);
        t.and_then(|t| self.item_abilities(t).into_iter().next()).map_or(crate::ability::Targeting::None, |a| self.targeting(a))
    }

    /// Uses the item in a slot: its ability's effect, then a charge if it has any.
    pub fn use_item(&mut self, vm: &mut Vm, unit: u32, slot: usize, target: u32, x: f32, y: f32) -> bool {
        let Some(item) = self.unit(unit).and_then(|u| u.inventory.get(slot).copied()).filter(|&i| i != 0) else { return false };
        let Some(type_id) = self.item(item).map(|i| i.type_id) else { return false };
        let usable = self.objects.items.get(type_id).is_some_and(|o| o.bool("usable"));
        if !usable {
            return false;
        }
        // An item's spell aimed at a unit or point is cast as the holder's own, and
        // its charge is spent when it takes effect (measured: a Wand of the Wind used
        // on a footman goes from 3 charges to 2 and the footman is cycloned).
        if target != 0 || x != 0.0 || y != 0.0 {
            let cast = self.item_abilities(type_id).into_iter().find(|&a| !matches!(self.targeting(a), crate::ability::Targeting::None | crate::ability::Targeting::Passive));
            if let Some(a) = cast {
                if target != 0 {
                    if let Some(e) = self.cast_target_error(unit, a, target) {
                        self.order_error = Some(e);
                        return false;
                    }
                }
                if let Some(u) = self.unit_mut(unit) {
                    u.action = Action::Cast { ability: a, target, x, y };
                    u.casting = None;
                    u.swing = None;
                    u.path.clear();
                    u.extra.casting_item = Some(item);
                }
                return true;
            }
        }
        // Used without a target, an item whose regeneration reaches a unit (a range
        // and no area: a Healing Salve's) does nothing (measured: UnitUseItem with a
        // salve answers false and the holder is not healed).
        if target == 0 && self.item_abilities(type_id).into_iter().any(|a| self.ability_code(a) == "AIrg" && self.data(a, "Rng", 1) > 0.0 && self.data(a, "Area", 1) <= 0.0) {
            return false;
        }
        if !self.item_effect(vm, unit, item, type_id, target, x, y) {
            return false;
        }
        self.spend_item_use(vm, unit, item, type_id);
        true
    }

    /// An item's use is spent: its use events, and a charge, the item going when a
    /// perishable one has none left.
    pub(crate) fn spend_item_use(&mut self, vm: &mut Vm, unit: u32, item: u32, type_id: Id) {
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
    }

    /// What an item's abilities do when used (or picked up, for powerups).  Returns
    /// whether anything happened.
    pub(crate) fn item_effect(&mut self, vm: &mut Vm, unit: u32, _item: u32, type_id: Id, _target: u32, _x: f32, _y: f32) -> bool {
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
                // Healing Salve, Clarity Potion, and the like: DataA life and DataB mana
                // over Dur, ended by damage when DataE is set.  The buff is the first of
                // BuffID for both, the second for life, the third for mana.
                // Regeneration: DataA life over Dur, once a second from a second after
                // (measured: a Scroll of Regeneration's 225 over 45 heals 5 at 1.1, 2.1,
                // and on, used at 0.1), and DataB mana; to friends within Area when it
                // has one (the scroll's 600), else to its target or holder.
                "AIrg" => {
                    let dur = data(self, "Dur").max(0.1);
                    let (life, mana) = (data(self, "DataA"), data(self, "DataB"));
                    let ids: Vec<Id> = self.objects.abilities.get(a).map(|o| o.level_str("BuffID", 1)).unwrap_or_default().split(',').map(|b| crate::objects::id_from_str(b.trim())).collect();
                    let k = if life > 0.0 && mana > 0.0 { 0 } else if life > 0.0 { 1 } else { 2 };
                    let id = ids.get(k).or(ids.first()).copied().unwrap_or([0; 4]);
                    let ends = data(self, "DataE") != 0.0;
                    let targets: Vec<u32> = if area > 0.0 {
                        let (ux, uy) = self.unit(unit).map(|u| (u.x, u.y)).unwrap_or_default();
                        self.allies_in(owner, ux, uy, area).into_iter().filter(|&t| self.unit(t).is_some_and(|u| !u.is_building)).collect()
                    } else {
                        vec![if _target != 0 { _target } else { unit }]
                    };
                    for t in targets {
                        self.apply_buff(t, Buff { id, ability: a, level: 1, caster: unit, remaining: dur, dps: -life / dur, dps_every: 1.0, dps_next: 1.0, mana_regen: mana / dur, ends_on_damage: ends, ..Default::default() });
                    }
                    did = true;
                }
                // Scroll of Protection (and the like): DataA armor for Dur to friends
                // within Area, who gain DataB life and DataC mana.
                "AIda" => {
                    let (ux, uy) = self.unit(unit).map(|u| (u.x, u.y)).unwrap_or_default();
                    let id = self.first_buff(a, 1);
                    let (armor, life, mana, dur) = (data(self, "DataA"), data(self, "DataB"), data(self, "DataC"), data(self, "Dur"));
                    for t in self.allies_in(owner, ux, uy, area) {
                        if let Some(u) = self.unit_mut(t) {
                            u.life = (u.life + life).min(u.max_life);
                            u.mana = (u.mana + mana).min(u.max_mana);
                        }
                        self.apply_buff(t, Buff { id, ability: a, level: 1, caster: unit, remaining: dur, armor, ..Default::default() });
                    }
                    did = true;
                }
                // Scroll of Speed: friends within Area move DataA faster for Dur.
                "AIsa" | "AIsp" => {
                    let (ux, uy) = self.unit(unit).map(|u| (u.x, u.y)).unwrap_or_default();
                    let id = self.first_buff(a, 1);
                    let (speed, dur) = (data(self, "DataA"), data(self, "Dur"));
                    let group = if code == "AIsa" { self.allies_in(owner, ux, uy, area) } else { vec![unit] };
                    for t in group {
                        self.apply_buff(t, Buff { id, ability: a, level: 1, caster: unit, remaining: dur, move_factor: speed, ..Default::default() });
                    }
                    did = true;
                }
                // Potions of Invisibility and Invulnerability.
                "AIvi" | "AIvu" => {
                    let id = self.first_buff(a, 1);
                    let dur = data(self, "Dur");
                    let b = Buff { id, ability: a, level: 1, caster: unit, remaining: dur, invisible: code == "AIvi", invulnerable: code == "AIvu", ..Default::default() };
                    self.apply_buff(unit, b);
                    did = true;
                }
                // Wand of Negation: magic dispelled within Area of the target, DataA mana
                // lost, and DataB damage to summoned units.
                "AIdi" => {
                    let (px, py) = if _target != 0 { self.widget_pos(_target).unwrap_or((_x, _y)) } else { (_x, _y) };
                    let (mana, summoned) = (data(self, "DataA"), data(self, "DataB"));
                    for t in self.units_in_pub(px, py, area) {
                        if let Some(u) = self.unit_mut(t) {
                            u.mana = (u.mana - mana).max(0.0);
                        }
                        self.dispel_pub(vm, unit, t, summoned, false);
                    }
                    did = true;
                }
                // Dust of Appearance: invisible units within Area show for Dur.
                "AItb" => {
                    let (ux, uy) = self.unit(unit).map(|u| (u.x, u.y)).unwrap_or_default();
                    let until = self.time + data(self, "Dur");
                    self.detect_areas.push((owner, ux, uy, area, until));
                    did = true;
                }
                // Scroll of Town Portal: the hero and up to DataA friends within Area go to
                // the target structure.
                "AItp" => {
                    // Aimed at a point it goes to the town hall nearest it: the owner's,
                    // else an ally's (FUN_6f40d3d0).
                    let mut _target = _target;
                    if _target == 0 && (_x != 0.0 || _y != 0.0) {
                        let hall = |w: &World, mine: bool| {
                            w.unit_order
                                .iter()
                                .copied()
                                .filter(|&t| w.unit(t).is_some_and(|u| !u.dead && u.constructing.is_none() && w.is_town_hall(u) && if mine { u.owner == owner } else { u.owner != owner && crate::natives::players::is_ally(w, owner, u.owner) }))
                                .min_by(|&a, &b| {
                                    let d = |t: u32| w.unit(t).map_or(f32::MAX, |u| (u.x - _x).hypot(u.y - _y));
                                    d(a).total_cmp(&d(b))
                                })
                        };
                        match hall(self, true).or_else(|| hall(self, false)) {
                            Some(t) => _target = t,
                            None => continue,
                        }
                    }
                    let Some((tx, ty)) = self.widget_pos(_target).or_else(|| (_x != 0.0 || _y != 0.0).then_some((_x, _y))) else { continue };
                    let (ux, uy) = self.unit(unit).map(|u| (u.x, u.y)).unwrap_or_default();
                    let n = data(self, "DataA") as usize;
                    let mut group: Vec<u32> = self.allies_in(owner, ux, uy, area).into_iter().filter(|&t| t != unit && self.unit(t).is_some_and(|u| u.owner == owner && !u.is_building)).take(n).collect();
                    group.push(unit);
                    for t in group {
                        let r = self.unit(t).map_or(16.0, |u| u.collision);
                        let (x, y) = if _target != 0 && self.unit(_target).is_some() { self.spawn_point_pub(_target, r) } else { self.free_point(tx, ty, r) };
                        if let Some(u) = self.unit_mut(t) {
                            u.x = x;
                            u.y = y;
                            u.path.clear();
                            u.action = Action::Idle;
                        }
                    }
                    did = true;
                }
                // Rune of Rebirth (CAbilityRebirth): a unit of the type whose death left
                // the rune (SetItemDropID) joins the one picking it up, for Dur when set,
                // not as a summon, with the BIrb buff's Effectart.
                "AIrb" => {
                    if let Some(t) = self.item(_item).and_then(|i| i.drop_type) {
                        let (x, y) = self.unit(unit).map_or((0.0, 0.0), |u| (u.x, u.y));
                        let born = crate::natives::units::create_unit(self, owner, t, x, y, 270.0);
                        let dur = data(self, "Dur");
                        if dur > 0.0 {
                            if let Some(u) = self.unit_mut(born) {
                                u.timed_life = Some(dur);
                            }
                        }
                        let buff = self.objects.abilities.get(a).map(|o| o.level_str("BuffID", 1)).unwrap_or_default();
                        let art = self.objects.buffs.get(crate::objects::id_from_str(buff.trim())).map(|o| o.str("Effectart")).unwrap_or_default();
                        self.spawn_effect(&art, x, y);
                    }
                    did = true;
                }
                // Soul Gem (CAbilitySoulTrap): on an enemy Hero, its Effectart there.
                "AIso" => {
                    if _target != 0 {
                        let model = self.profile_art(a, "Effectart");
                        if let Some((tx, ty)) = self.widget_pos(_target) {
                            self.spawn_effect(&model, tx, ty);
                        }
                        did = true;
                    }
                }
                // Moonstone: the time of day becomes DataA:DataB.
                "AIct" => {
                    self.time_of_day = data(self, "DataA") + data(self, "DataB") / 60.0;
                    did = true;
                }
                // Items that cast their spell's effect (see ability_code's aliases).
                "Arai" | "AUan" | "AHre" | "Aweb" | "Afla" => {
                    let t = if _target != 0 { _target } else { unit };
                    let at = if _target != 0 { self.widget_pos(_target).unwrap_or((_x, _y)) } else if _x != 0.0 || _y != 0.0 { (_x, _y) } else { self.unit(unit).map(|u| (u.x, u.y)).unwrap_or_default() };
                    self.ability_effect_pub(vm, unit, a, 1, t, at);
                    did = true;
                }
                // Figurines: DataA of DataC and DataB of DataD for Dur beside the hero.
                "AIfs" => {
                    let (ux, uy) = self.unit(unit).map(|u| (u.x, u.y)).unwrap_or_default();
                    let dur = data(self, "Dur");
                    for (count, kind) in [("DataA", "DataC"), ("DataB", "DataD")] {
                        let kind = crate::objects::id_from_str(self.objects.abilities.get(a).map(|o| o.level_str(kind, 1)).unwrap_or_default().trim());
                        let n = data(self, count) as i32;
                        if kind[0] != 0 && n > 0 {
                            self.summon(unit, owner, kind, n, ux, uy, dur);
                        }
                    }
                    did = true;
                }
                // Summon Headhunter (the Axe of the Headhunters' Berserkers): DataB of
                // UnitID for Dur.
                "AIsh" => {
                    let (ux, uy) = self.unit(unit).map(|u| (u.x, u.y)).unwrap_or_default();
                    let kind = crate::objects::id_from_str(self.objects.abilities.get(a).map(|o| o.level_str("UnitID", 1)).unwrap_or_default().trim());
                    self.summon(unit, owner, kind, data(self, "DataB") as i32, ux, uy, data(self, "Dur"));
                    did = true;
                }
                // Potion of Omniscience: the whole map shows for Dur.
                "AIrv" => {
                    let b = self.camera_bounds;
                    let (x0, x1) = (b.iter().step_by(2).copied().fold(f32::INFINITY, f32::min), b.iter().step_by(2).copied().fold(f32::NEG_INFINITY, f32::max));
                    let (y0, y1) = (b.iter().skip(1).step_by(2).copied().fold(f32::INFINITY, f32::min), b.iter().skip(1).step_by(2).copied().fold(f32::NEG_INFINITY, f32::max));
                    let r = (x1 - x0).hypot(y1 - y0);
                    self.reveal_area(owner, (x0 + x1) / 2.0, (y0 + y1) / 2.0, r, data(self, "Dur"), 0.0);
                    did = true;
                }
                // Goblin Land Mines: a mine (UnitID) at the point.
                "AIpm" => {
                    let kind = crate::objects::id_from_str(self.objects.abilities.get(a).map(|o| o.level_str("UnitID", 1)).unwrap_or_default().trim());
                    let (px, py) = if _x != 0.0 || _y != 0.0 { (_x, _y) } else { self.unit(unit).map(|u| (u.x, u.y)).unwrap_or_default() };
                    crate::natives::units::create_unit(self, owner, kind, px, py, 270.0);
                    did = true;
                }
                // Glyphs: a level of the research UnitID.
                "AIgl" => {
                    let up = crate::objects::id_from_str(self.objects.abilities.get(a).map(|o| o.level_str("UnitID", 1)).unwrap_or_default().trim());
                    let n = data(self, "DataA").max(1.0) as i32;
                    *self.players[owner as usize].tech_researched.entry(up).or_insert(0) += n;
                    self.research_changed(owner);
                    did = true;
                }
                // Potion of Vampirism: DataA more damage and DataB of it regained for Dur.
                "AIpv" => {
                    let id = self.first_buff(a, 1);
                    let (bonus, steal, dur) = (data(self, "DataA"), data(self, "DataB"), data(self, "Dur"));
                    self.apply_buff(unit, Buff { id, ability: a, level: 1, caster: unit, remaining: dur, damage_bonus: bonus, life_steal: steal, ..Default::default() });
                    did = true;
                }
                // Tiny buildings: the owner's race's structure (UnitID: Human, Orc,
                // Undead, Night Elf) at the point, going up over Dur.
                "AIbl" => {
                    let ids = self.objects.abilities.get(a).map(|o| o.level_str("UnitID", 1)).unwrap_or_default();
                    let ids: Vec<&str> = ids.split(',').map(str::trim).collect();
                    let race = self.players.get(owner as usize).map_or(1, |p| p.race).clamp(1, 4) as usize;
                    let kind = crate::objects::id_from_str(ids.get(race - 1).or(ids.first()).copied().unwrap_or(""));
                    if (_x != 0.0 || _y != 0.0) && self.can_place(kind, _x, _y) {
                        let angle = self.misc_f32("BuildingAngle", 270.0);
                        let b = crate::natives::units::create_unit(self, owner, kind, _x, _y, angle);
                        let dur = data(self, "Dur").max(0.1);
                        if let Some(bu) = self.unit_mut(b) {
                            bu.build_time = dur;
                            bu.constructing = Some(dur);
                            bu.life = bu.max_life * 0.1;
                            bu.anim = "birth".into();
                            bu.anim_serial = bu.anim_serial.wrapping_add(1);
                        }
                        did = true;
                    }
                }
                // Tome of Retraining: the hero unlearns its skills and has a point for
                // each level again.
                "Aret" => {
                    let Some(u) = self.unit(unit).filter(|u| u.is_hero) else { continue };
                    let skills: Vec<Id> = self.objects.units.get(u.type_id).map(|o| o.list("heroAbilList")).unwrap_or_default().iter().map(|s| crate::objects::id_from_str(s.trim())).collect();
                    let level = u.level;
                    let u = self.unit_mut(unit).unwrap();
                    u.abilities.retain(|(a, _)| !skills.contains(a));
                    u.skill_points = level;
                    self.apply_item_bonuses(unit);
                    did = true;
                }
                // Amulet of Recall and Dark Summoning: up to DataA of the player's units
                // within Area of the target come to the hero.
                "AIrt" | "AUds" => {
                    let Some((tx, ty)) = self.widget_pos(_target) else { continue };
                    let (ux, uy) = self.unit(unit).map(|u| (u.x, u.y)).unwrap_or_default();
                    let n = data(self, "DataA") as usize;
                    let group: Vec<u32> = self.units_in_pub(tx, ty, area).into_iter().filter(|&t| t != unit && self.unit(t).is_some_and(|u| u.owner == owner && !u.is_building && !u.dead)).take(n).collect();
                    for t in group {
                        let r = self.unit(t).map_or(16.0, |u| u.collision);
                        let (x, y) = self.free_point(ux, uy, r + 32.0);
                        if let Some(u) = self.unit_mut(t) {
                            u.x = x;
                            u.y = y;
                            u.path.clear();
                            u.action = Action::Idle;
                        }
                    }
                    did = true;
                }
                // Wand of Neutralization: magic dispelled from the target and up to DataC
                // in all within Area of it, nearest first, each losing DataA mana; summons
                // take DataB.
                "AIdc" => {
                    let Some((tx, ty)) = self.widget_pos(_target).or(Some((_x, _y))) else { continue };
                    let (mana, summoned, max) = (data(self, "DataA"), data(self, "DataB"), data(self, "DataC").max(1.0) as usize);
                    let mut hit = self.units_in_pub(tx, ty, area);
                    hit.sort_by(|&p, &q| {
                        let d = |t: u32| self.unit(t).map_or(f32::MAX, |u| (u.x - tx).hypot(u.y - ty));
                        d(p).total_cmp(&d(q))
                    });
                    for t in hit.into_iter().take(max) {
                        if let Some(u) = self.unit_mut(t) {
                            u.mana = (u.mana - mana).max(0.0);
                        }
                        self.dispel_pub(vm, unit, t, summoned, true);
                    }
                    did = true;
                }
                // Scepter of Mastery (ItemCommand): a non-hero of level DataA or less
                // becomes the user's.
                "AIco" => {
                    let max = data(self, "DataA") as i32;
                    let color = self.players.get(owner as usize).map_or(0, |p| p.color);
                    if let Some(t) = self.unit_mut(_target).filter(|t| !t.is_hero && !t.dead && t.level <= max) {
                        t.owner = owner;
                        t.color = color;
                        t.action = Action::Idle;
                        did = true;
                    }
                    self.update_food();
                }
                // Anti-magic Potion: magic immunity for Dur.
                "Aami" => {
                    let id = self.first_buff(a, 1);
                    self.apply_buff(unit, Buff { id, ability: a, level: 1, caster: unit, remaining: data(self, "Dur"), magic_immune: true, ..Default::default() });
                    did = true;
                }
                // Sacrificial Skull: blight within Area of the point.
                "Ablp" => {
                    let (px, py) = if _x != 0.0 || _y != 0.0 { (_x, _y) } else { self.unit(unit).map(|u| (u.x, u.y)).unwrap_or_default() };
                    self.blight_circle(px, py, area);
                    did = true;
                }
                // Wand of Mana Stealing: up to DataA of the target's mana to the user.
                "Aste" => {
                    let take = self.unit(_target).map_or(0.0, |t| t.mana.min(data(self, "DataA")));
                    if let Some(t) = self.unit_mut(_target) {
                        t.mana -= take;
                    }
                    if let Some(u) = self.unit_mut(unit) {
                        u.mana = (u.mana + take).min(u.max_mana);
                    }
                    did = take > 0.0;
                }
                // Monster Lure: DataA lures (UnitID) at the point for Dur.
                "AImo" => {
                    let kind = crate::objects::id_from_str(self.objects.abilities.get(a).map(|o| o.level_str("UnitID", 1)).unwrap_or_default().trim());
                    let (px, py) = if _x != 0.0 || _y != 0.0 { (_x, _y) } else { self.unit(unit).map(|u| (u.x, u.y)).unwrap_or_default() };
                    self.summon(unit, owner, kind, data(self, "DataA").max(1.0) as i32, px, py, data(self, "Dur"));
                    did = true;
                }
                // Wand of Illusion: an image of the target.
                "AIil" => {
                    let t = if _target != 0 { _target } else { unit };
                    let (dealt, taken) = (data(self, "DataA"), data(self, "DataB"));
                    if let Some((tx, ty)) = self.unit(t).map(|u| (u.x + 64.0, u.y)) {
                        did = self.make_illusion(t, owner, (tx, ty), a, 1, dealt, taken).is_some();
                    }
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
                // Manual of Health: maximum life for good (measured: a Paladin of 650
                // goes to 725 with it and a Tome of Strength).
                "AImi" => {
                    let gain = data(self, "DataA");
                    let hero = self.unit(unit).is_some_and(|u| u.is_hero);
                    if let Some(u) = self.unit_mut(unit) {
                        u.extra.permanent_life += gain;
                        if !hero {
                            u.max_life += gain;
                            u.life += gain;
                        }
                    }
                    if hero {
                        self.apply_hero_stats(unit, false);
                    }
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
        // A carried spell book's spells come and go with it.
        self.sync_spell_books(unit);
        let Some(u) = self.unit(unit) else { return };
        let items: Vec<Id> = u.inventory.iter().filter_map(|&i| self.item(i).map(|i| i.type_id)).collect();
        let mut b = ItemBonus::default();
        // Without MoveSpeedBonusesStack only the first movement speed bonus counts; the
        // others wait disabled (CAbilityMoveSpeedBonus::vf15).
        let stack = self.misc_f32("MoveSpeedBonusesStack", 0.0) != 0.0;
        let mut speed_bonuses = 0;
        for t in items {
            for a in self.item_abilities(t) {
                let d = |f: &str| self.objects.abilities.get(a).map_or(0.0, |o| o.level_real(f, 1));
                match self.ability_code(a).as_str() {
                    "AIat" => b.damage += d("DataA"),
                    "AIde" => b.armor += d("DataA"),
                    "AIms" => {
                        if stack || speed_bonuses == 0 {
                            b.move_speed += d("DataA");
                        }
                        speed_bonuses += 1;
                    }
                    "AIas" => b.attack_speed += d("DataA"),
                    "AIml" => b.life += d("DataA"),
                    "AImm" => b.mana += d("DataA"),
                    "AIrm" => b.mana_regen += d("DataA"),
                    "Arel" => b.life_regen += d("DataA"),
                    "AIsr" => {
                        b.damage += d("DataA");
                        b.spell_reduction += d("DataB");
                    }
                    "AIva" => b.life_steal += d("DataA"),
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
        // Attribute Bonus (Aamk): DataA agility, DataB intelligence, and DataC strength
        // at its level.
        let learned: Vec<(Id, i32)> = self.unit(unit).map(|u| u.abilities.iter().copied().filter(|&(a, l)| l > 0 && self.ability_code(a) == "Aamk").collect()).unwrap_or_default();
        for (a, l) in learned {
            let d = |f: &str| self.data(a, f, l as u32);
            b.agi += d("DataA");
            b.int += d("DataB");
            b.str_ += d("DataC");
        }
        if let Some(u) = self.unit_mut(unit) {
            u.item_bonus = b;
        }
        // Life keeps its fraction of the new maximum (apply_upgrades_keeping).
        self.apply_upgrades_keeping(unit, true);
    }
}
