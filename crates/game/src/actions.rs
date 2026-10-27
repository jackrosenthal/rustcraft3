//! The players' commands as the original's actions (`w3_net::action`), which network
//! games and replays carry: each player's selection as `Select` changes, an order to it
//! as the order actions, and training, building, cancelling, and the build and skill
//! menus as the original sends them for its subgroup's unit, naming objects by the
//! original's ids (their tags' slot and birth).  Measured in replays the original wrote
//! (`docs/reference/bootybay_orders.w3g`, `bootybay_commands.w3g`): orders at a point
//! go as OrderTarget with no object, a build as OrderPoint with flags 4, training as
//! Order with flags 0x42, a cancel as Dequeue by place and type, and a queued order with
//! flag 1.  A command with no action of the original's yet travels as a private action
//! holding it (0x7F).  Both sides keep each player's selection as the actions left it.

use crate::collections::HashMap;
use crate::replay::Command;
use crate::world::World;
use w3_net::action::{Action, ObjectId, Target};

/// The private action carrying a command (postcard), until it has the original's.
pub const COMMAND_ACTION: u8 = 0x7F;

#[derive(Clone, Debug, Default)]
pub struct Codec {
    /// Each player's selection, as its actions have left it.
    selection: HashMap<u8, Vec<u32>>,
    /// Each player's subgroup unit (the selection's first, as SelectSubgroup names it).
    subgroup: HashMap<u8, u32>,
}

impl World {
    /// The original's id for a unit, item, or destructable.
    pub fn oid(&self, h: u32) -> ObjectId {
        match self.handles.get(h) {
            Some(crate::types::Obj::Unit(u)) => ObjectId(u.extra.oid.0, u.extra.oid.1),
            Some(crate::types::Obj::Item(i)) => ObjectId(i.oid.0, i.oid.1),
            Some(crate::types::Obj::Destructable(d)) => ObjectId(d.oid.0, d.oid.1),
            _ => ObjectId::NONE,
        }
    }

    /// The unit, item, or destructable the original's id names.
    pub fn by_oid(&self, id: ObjectId) -> Option<u32> {
        let want = (id.0, id.1);
        self.unit_order
            .iter()
            .copied()
            .find(|&h| self.unit(h).is_some_and(|u| u.extra.oid == want))
            .or_else(|| self.item_order.iter().copied().find(|&h| self.item(h).is_some_and(|i| i.oid == want)))
            .or_else(|| self.destructable_order.iter().copied().find(|&h| matches!(self.handles.get(h), Some(crate::types::Obj::Destructable(d)) if d.oid == want)))
    }
}

impl Codec {
    /// A player's command as actions, the selection as the receiving side will have it.
    pub fn encode(&mut self, w: &World, player: u8, c: &Command) -> Vec<u8> {
        let mut acts = Vec::new();
        match c {
            Command::Select { units, .. } => {
                let old = self.selection.get(&player).cloned().unwrap_or_default();
                if &old == units {
                    return Vec::new();
                }
                if !old.is_empty() {
                    acts.push(Action::Select { add: false, objects: old.iter().map(|&h| w.oid(h)).collect() });
                }
                if let Some(&first) = units.first() {
                    acts.push(Action::Select { add: true, objects: units.iter().map(|&h| w.oid(h)).collect() });
                    acts.push(Action::PreSubselection);
                    let kind = w.unit(first).map_or(0, |u| crate::objects::id_to_int(u.type_id) as u32);
                    acts.push(Action::SelectSubgroup { kind, object: w.oid(first) });
                }
                self.selection.insert(player, units.clone());
                self.subgroup.insert(player, units.first().copied().unwrap_or(0));
            }
            Command::SelectionOrder { order, at, target, queue, formation, .. } => {
                let flags = (*queue as u16) | if *formation { 0x10 } else { 0 };
                let (ability, a, b) = (*order as u32, u32::MAX, u32::MAX);
                acts.push(match (at, *target) {
                    (_, t) if t != 0 => {
                        let (x, y) = w.widget_pos(t).unwrap_or((0.0, 0.0));
                        Action::OrderTarget { flags, ability, a, b, target: Target { x, y, object: w.oid(t) } }
                    }
                    (Some((x, y)), _) => Action::OrderTarget { flags, ability, a, b, target: Target { x: *x, y: *y, object: ObjectId::NONE } },
                    (None, _) => Action::Order { flags, ability, a, b },
                });
            }
            Command::Train { building, id } if self.subgroup.get(&player) == Some(building) => {
                acts.push(Action::Order { flags: 0x42, ability: crate::objects::id_to_int(*id) as u32, a: u32::MAX, b: u32::MAX });
            }
            Command::Build { worker, id, x, y } if self.subgroup.get(&player) == Some(worker) => {
                acts.push(Action::OrderPoint { flags: 4, ability: crate::objects::id_to_int(*id) as u32, a: u32::MAX, b: u32::MAX, x: *x, y: *y });
            }
            Command::CancelQueue { building, slot } if self.subgroup.get(&player) == Some(building) && *slot < 256 => {
                let kind = w.unit(*building).and_then(|u| u.queue.get(*slot)).map_or(0, |id| crate::objects::id_to_int(*id) as u32);
                acts.push(Action::Dequeue { slot: *slot as u8, kind });
            }
            Command::Submenu { build, .. } => acts.push(if *build { Action::BuildMenu } else { Action::SkillMenu }),
            // A skill learned: Order with flags 0x42 and the ability, as training is
            // (`docs/reference/bootybay_hero.w3g`).
            Command::LearnSkill { hero, id } if self.subgroup.get(&player) == Some(hero) => {
                acts.push(Action::Order { flags: 0x42, ability: crate::objects::id_to_int(*id) as u32, a: u32::MAX, b: u32::MAX });
            }
            // An item used: Order with flags 0x60, order useslot (852008 and its slot), and
            // the item's id in the order's two words (`docs/reference/bootybay_shop.w3g`).
            Command::UseItem { unit, slot } if self.subgroup.get(&player) == Some(unit) && *slot < 6 => {
                let item = w.unit(*unit).and_then(|u| u.inventory.get(*slot).copied()).unwrap_or(0);
                let o = w.oid(item);
                acts.push(Action::Order { flags: 0x60, ability: 852008 + *slot as u32, a: o.0, b: o.1 });
            }
            // A purchase from the shop selected: Order with flags 0x40 and what is bought.
            Command::BuyItem { shop, id, .. } | Command::HireUnit { shop, id, .. } if self.subgroup.get(&player) == Some(shop) => {
                acts.push(Action::Order { flags: 0x40, ability: crate::objects::id_to_int(*id) as u32, a: u32::MAX, b: u32::MAX });
            }
            // An item moved: OrderTarget with flags 0x42, order moveslot (852002 and the
            // slot it goes to), the item as target at the unit's place.
            Command::MoveItem { unit, from, to } if self.subgroup.get(&player) == Some(unit) && *to < 6 => {
                let item = w.unit(*unit).and_then(|u| u.inventory.get(*from).copied()).unwrap_or(0);
                let (x, y) = w.widget_pos(*unit).unwrap_or((0.0, 0.0));
                acts.push(Action::OrderTarget { flags: 0x42, ability: 852002 + *to as u32, a: u32::MAX, b: u32::MAX, target: Target { x, y, object: w.oid(item) } });
            }
            // An item dropped: OrderItem with flags 0x44, order dropitem (852001), the
            // point and the item.
            Command::DropItemAt { unit, slot, x, y } if self.subgroup.get(&player) == Some(unit) => {
                let item = w.unit(*unit).and_then(|u| u.inventory.get(*slot).copied()).unwrap_or(0);
                acts.push(Action::OrderItem { flags: 0x44, ability: 852001, a: u32::MAX, b: u32::MAX, target: Target { x: *x, y: *y, object: ObjectId::NONE }, item: w.oid(item) });
            }
            _ => return self.encode_private(c),
        }
        let mut wr = w3_net::wire::Writer::default();
        for a in &acts {
            a.write(&mut wr);
        }
        wr.0
    }

    /// A command as the private action holding it.
    pub fn encode_private(&self, c: &Command) -> Vec<u8> {
        let body = postcard::to_allocvec(c).unwrap_or_default();
        let mut out = vec![COMMAND_ACTION];
        out.extend((body.len() as u16).to_le_bytes());
        out.extend(body);
        out
    }

    /// A player's block of actions as commands.
    pub fn decode(&mut self, w: &World, player: u8, bytes: &[u8]) -> anyhow::Result<Vec<Command>> {
        let mut out = Vec::new();
        let mut r = w3_net::wire::Reader::new(bytes);
        let mut changed = false;
        let flush = |changed: &mut bool, sel: &Vec<u32>, out: &mut Vec<Command>| {
            if std::mem::take(changed) {
                out.push(Command::Select { player, units: sel.clone() });
            }
        };
        while !r.is_empty() {
            if bytes.get(bytes.len() - r.rest_len()) == Some(&COMMAND_ACTION) {
                let sel = self.selection.get(&player).cloned().unwrap_or_default();
                flush(&mut changed, &sel, &mut out);
                r.u8().map_err(anyhow::Error::msg)?;
                let n = r.u16().map_err(anyhow::Error::msg)? as usize;
                out.push(postcard::from_bytes(r.bytes(n).map_err(anyhow::Error::msg)?)?);
                continue;
            }
            let act = Action::read(&mut r).map_err(anyhow::Error::msg)?;
            let sel = self.selection.entry(player).or_default();
            match act {
                Action::Select { add, objects } => {
                    let hs: Vec<u32> = objects.iter().filter_map(|&o| w.by_oid(o)).collect();
                    if add {
                        sel.extend(hs);
                    } else {
                        sel.retain(|h| !hs.contains(h));
                    }
                    changed = true;
                }
                Action::PreSubselection => {}
                Action::SelectSubgroup { object, .. } => {
                    self.subgroup.insert(player, w.by_oid(object).unwrap_or(0));
                }
                other => {
                    let sel = sel.clone();
                    flush(&mut changed, &sel, &mut out);
                    let first = self.subgroup.get(&player).copied().unwrap_or(0);
                    let id = |v: u32| crate::objects::id_from_int(v as i32);
                    let order = |ability: u32, flags: u16, at: Option<(f32, f32)>, target: u32| Command::SelectionOrder { player, order: ability as i32, at, target, queue: flags & 1 != 0, formation: flags & 0x10 != 0 };
                    match other {
                        Action::Order { flags: 0x60, ability: a @ 852008..=852013, .. } => out.push(Command::UseItem { unit: first, slot: (a - 852008) as usize }),
                        Action::Order { flags: 0x40, ability, .. } if w.objects.units.get(id(ability)).is_some() => out.push(Command::HireUnit { player, shop: first, id: id(ability) }),
                        Action::Order { flags: 0x40, ability, .. } => out.push(Command::BuyItem { player, shop: first, id: id(ability) }),
                        Action::Order { flags: 0x42, ability, .. } if w.objects.abilities.get(id(ability)).is_some() => out.push(Command::LearnSkill { hero: first, id: id(ability) }),
                        Action::Order { flags: 0x42, ability, .. } => out.push(Command::Train { building: first, id: id(ability) }),
                        Action::OrderTarget { flags: 0x42, ability: a @ 852002..=852007, target, .. } => {
                            let item = w.by_oid(target.object).unwrap_or(0);
                            let from = w.unit(first).and_then(|u| u.inventory.iter().position(|&i| i == item && i != 0)).unwrap_or(6);
                            out.push(Command::MoveItem { unit: first, from, to: (a - 852002) as usize });
                        }
                        Action::OrderItem { flags: 0x44, ability: 852001, target, item, .. } if target.object == ObjectId::NONE => {
                            let h = w.by_oid(item).unwrap_or(0);
                            let slot = w.unit(first).and_then(|u| u.inventory.iter().position(|&i| i == h && i != 0)).unwrap_or(6);
                            out.push(Command::DropItemAt { unit: first, slot, x: target.x, y: target.y });
                        }
                        Action::Order { flags, ability, .. } => out.push(order(ability, flags, None, 0)),
                        Action::OrderPoint { flags: 4, ability, x, y, .. } => out.push(Command::Build { worker: first, id: id(ability), x, y }),
                        Action::OrderPoint { flags, ability, x, y, .. } => out.push(order(ability, flags, Some((x, y)), 0)),
                        Action::Dequeue { slot, .. } => out.push(Command::CancelQueue { building: first, slot: slot as usize }),
                        Action::BuildMenu => out.push(Command::Submenu { player, build: true }),
                        Action::SkillMenu => out.push(Command::Submenu { player, build: false }),
                        Action::OrderTarget { flags, ability, target, .. } if target.object == ObjectId::NONE => out.push(order(ability, flags, Some((target.x, target.y)), 0)),
                        Action::OrderTarget { flags, ability, target, .. } => out.push(order(ability, flags, None, w.by_oid(target.object).unwrap_or(0))),
                        a => log::warn!("an action this game does not take yet: {a:?}"),
                    }
                }
            }
        }
        let sel = self.selection.get(&player).cloned().unwrap_or_default();
        flush(&mut changed, &sel, &mut out);
        Ok(out)
    }
}
