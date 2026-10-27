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
            _ => self.tags.handle_ids.get(&h).map_or(ObjectId::NONE, |&(a, b)| ObjectId(a, b)),
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
            .or_else(|| self.tags.handle_ids.iter().find(|&(&h, &v)| v == want && self.handles.get(h).is_some()).map(|(&h, _)| h))
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
            // Research: Order with flags 0x46 and the research; an upgrade (a town hall's
            // to a keep): Order with flags 0x40 and what it becomes
            // (`docs/reference/bootybay_research.w3g`).
            Command::Research { building, id } if self.subgroup.get(&player) == Some(building) => {
                acts.push(Action::Order { flags: 0x46, ability: crate::objects::id_to_int(*id) as u32, a: u32::MAX, b: u32::MAX });
            }
            Command::Upgrade { building, id } if self.subgroup.get(&player) == Some(building) => {
                acts.push(Action::Order { flags: 0x40, ability: crate::objects::id_to_int(*id) as u32, a: u32::MAX, b: u32::MAX });
            }
            // A revival: OrderTarget with flags 0x46, revive, and the dead hero where it
            // lies (`docs/reference/bootybay_revive.w3g`).
            Command::StartRevive { altar, hero } if self.subgroup.get(&player) == Some(altar) => {
                let (x, y) = w.unit(*hero).map(|u| (u.x, u.y)).unwrap_or((0.0, 0.0));
                acts.push(Action::OrderTarget { flags: 0x46, ability: 852027, a: u32::MAX, b: u32::MAX, target: Target { x, y, object: w.oid(*hero) } });
            }
            // A purchase from the shop selected: Order with flags 0x40 and what is bought.
            Command::BuyItem { shop, id, .. } | Command::HireUnit { shop, id, .. } if self.subgroup.get(&player) == Some(shop) => {
                acts.push(Action::Order { flags: 0x40, ability: crate::objects::id_to_int(*id) as u32, a: u32::MAX, b: u32::MAX });
            }
            // An item given: a drop (OrderItem, flags 0x44, dropitem) on the unit taking it;
            // an item used on a target: OrderTarget with flags 0x44, useslot, the item's
            // id in the order's two words, and the target (`docs/reference/bootybay_give.w3g`).
            Command::GiveItemTo { unit, slot, target } if self.subgroup.get(&player) == Some(unit) => {
                let item = w.unit(*unit).and_then(|u| u.inventory.get(*slot).copied()).unwrap_or(0);
                let (x, y) = w.widget_pos(*target).unwrap_or((0.0, 0.0));
                acts.push(Action::OrderItem { flags: 0x44, ability: 852001, a: u32::MAX, b: u32::MAX, target: Target { x, y, object: w.oid(*target) }, item: w.oid(item) });
            }
            Command::UseItemAt { unit, slot, target, x, y } if self.subgroup.get(&player) == Some(unit) && *slot < 6 => {
                let item = w.unit(*unit).and_then(|u| u.inventory.get(*slot).copied()).unwrap_or(0);
                let o = w.oid(item);
                let (tx, ty) = if *target != 0 { w.widget_pos(*target).unwrap_or((*x, *y)) } else { (*x, *y) };
                let object = if *target != 0 { w.oid(*target) } else { ObjectId::NONE };
                acts.push(Action::OrderTarget { flags: 0x44, ability: 852008 + *slot as u32, a: o.0, b: o.1, target: Target { x: tx, y: ty, object } });
            }
            // A minimap signal: its point and its 5 seconds (measured from the original
            // joined to a hosted game).
            Command::Signal { x, y, .. } => acts.push(Action::Signal { x: *x, y: *y, a: 5.0f32.to_bits() }),
            // Alliances toward a player: 50, the player, and the alliance types' bits
            // (measured: Ally, Share Vision, and Allied Victory ticked gave 0x43f, bit 10
            // the allied victory); resources sent: 51, the player, gold, and lumber
            // (measured: 200 gold to player 0, after the alliances in the same block).
            Command::Alliance { other, flags, .. } => acts.push(Action::Alliance { player: *other, flags: *flags }),
            Command::Trade { other, gold, lumber, .. } => acts.push(Action::Trade { player: *other, gold: *gold as u32, lumber: *lumber as u32 }),
            // A chat line: 60 for each chat event registration it matches, in their order,
            // naming it (measured with one and with three); the line itself goes as the
            // game's chat (`crate::world::Request::Chat`, W3GS chat to all as scope 0).
            Command::Chat { text, .. } => {
                for e in w.chat_hearers(player, text) {
                    acts.push(Action::TriggerChat { event: w.oid(e), text: text.clone() });
                }
                if acts.is_empty() {
                    return Vec::new();
                }
            }
            // A trackable clicked (64) or moved onto (65): one for each registration of
            // its event, naming it (measured with one and with two).
            Command::Trackable { trackable, hit, .. } => {
                for e in w.trackable_hearers(*trackable, *hit) {
                    acts.push(if *hit { Action::TrackableHit(w.oid(e)) } else { Action::TrackableTrack(w.oid(e)) });
                }
                if acts.is_empty() {
                    return Vec::new();
                }
            }
            // A dialog's button: 6A with the dialog and the button, then 69 with the button
            // and the dialog (measured).
            Command::DialogButton { button, .. } => {
                let Some(crate::types::Obj::DialogButton(b)) = w.handles.get(*button) else { return Vec::new() };
                let (dialog, button) = (w.oid(b.dialog), w.oid(*button));
                acts.push(Action::DialogClick { dialog, button });
                acts.push(Action::DialogButton { button, dialog });
            }
            // Escape: 61, registered or not; an arrow key's press or release: 75 and its
            // number (left, right, down, up, a press then a release each), only when the
            // player has its event registered (measured).
            Command::PlayerEvent { event, .. } if *event == crate::event_ids::EVENT_PLAYER_END_CINEMATIC => acts.push(Action::Escape),
            Command::PlayerEvent { event, .. } if (crate::event_ids::EVENT_PLAYER_ARROW_LEFT_DOWN..=crate::event_ids::EVENT_PLAYER_ARROW_UP_UP).contains(event) => {
                if !w.hears_player_event(player, *event) {
                    return Vec::new();
                }
                acts.push(Action::ArrowKey((event - crate::event_ids::EVENT_PLAYER_ARROW_LEFT_DOWN) as u8));
            }
            // A game cache value: 6B to 6E with the file, mission, and key and the value
            // (an integer, a real, a boolean as a word, a unit's record); a missing integer
            // 70 with none; a string a bare 6F (measured: SyncStoredString sends nothing
            // more).
            Command::CacheValue { cache, mission, key, value } => {
                use crate::replay::CacheValue as V;
                let entry = |id: u8, value: Vec<u8>| Action::Cache { id, file: cache.clone(), mission: mission.clone(), key: key.clone(), value };
                acts.push(match value {
                    V::Int(Some(v)) => entry(0x6B, v.to_le_bytes().to_vec()),
                    V::Real(Some(v)) => entry(0x6C, v.to_le_bytes().to_vec()),
                    V::Bool(Some(v)) => entry(0x6D, (*v as u32).to_le_bytes().to_vec()),
                    V::Unit(Some(u)) => entry(0x6E, unit_record(w, u)),
                    V::Int(None) => entry(0x70, Vec::new()),
                    V::Str(_) => Action::CacheString,
                    _ => return self.encode_private(c),
                });
            }
            // A network game saved under a name (measured: 06 and the name).
            Command::Save { name } => acts.push(Action::SaveGame(name.clone())),
            // A pause (measured: 01 as the original's game ends) and its resume (02).
            Command::Pause { on, .. } => acts.push(if *on { Action::Pause } else { Action::Resume }),
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
                        Action::OrderTarget { flags: 0x46, ability: 852027, target, .. } => out.push(Command::StartRevive { altar: first, hero: w.by_oid(target.object).unwrap_or(0) }),
                        Action::Order { flags: 0x46, ability, .. } => out.push(Command::Research { building: first, id: id(ability) }),
                        Action::Order { flags: 0x40, ability, .. } if w.unit(first).and_then(|u| w.objects.units.get(u.type_id)).is_some_and(|o| o.list("Upgrade").iter().any(|s| crate::objects::id_from_str(s.trim()) == id(ability))) => out.push(Command::Upgrade { building: first, id: id(ability) }),
                        Action::Order { flags: 0x40, ability, .. } if w.objects.units.get(id(ability)).is_some() => out.push(Command::HireUnit { player, shop: first, id: id(ability) }),
                        Action::Order { flags: 0x40, ability, .. } => out.push(Command::BuyItem { player, shop: first, id: id(ability) }),
                        Action::Order { flags: 0x42, ability, .. } if w.objects.abilities.get(id(ability)).is_some() => out.push(Command::LearnSkill { hero: first, id: id(ability) }),
                        Action::Order { flags: 0x42, ability, .. } => out.push(Command::Train { building: first, id: id(ability) }),
                        Action::OrderTarget { flags: 0x42, ability: a @ 852002..=852007, target, .. } => {
                            let item = w.by_oid(target.object).unwrap_or(0);
                            let from = w.unit(first).and_then(|u| u.inventory.iter().position(|&i| i == item && i != 0)).unwrap_or(6);
                            out.push(Command::MoveItem { unit: first, from, to: (a - 852002) as usize });
                        }
                        Action::OrderItem { flags: 0x44, ability: 852001, target, item, .. } if target.object != ObjectId::NONE => {
                            let h = w.by_oid(item).unwrap_or(0);
                            let slot = w.unit(first).and_then(|u| u.inventory.iter().position(|&i| i == h && i != 0)).unwrap_or(6);
                            out.push(Command::GiveItemTo { unit: first, slot, target: w.by_oid(target.object).unwrap_or(0) });
                        }
                        Action::OrderTarget { flags: 0x44, ability: a @ 852008..=852013, target, .. } => {
                            let t = if target.object == ObjectId::NONE { 0 } else { w.by_oid(target.object).unwrap_or(0) };
                            out.push(Command::UseItemAt { unit: first, slot: (a - 852008) as usize, target: t, x: target.x, y: target.y });
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
                        Action::Signal { x, y, .. } => out.push(Command::Signal { player, x, y }),
                        Action::Alliance { player: other, flags } => out.push(Command::Alliance { player, other, flags }),
                        Action::TriggerChat { event, text } => out.push(Command::EventHeard { player, event: w.by_oid(event).unwrap_or(0), text }),
                        Action::TrackableHit(event) | Action::TrackableTrack(event) => out.push(Command::EventHeard { player, event: w.by_oid(event).unwrap_or(0), text: String::new() }),
                        // The click is taken once, from 6A; 69 always follows it.
                        Action::DialogClick { button, .. } => out.push(Command::DialogButton { player, button: w.by_oid(button).unwrap_or(0) }),
                        Action::DialogButton { .. } => {}
                        Action::Escape => out.push(Command::PlayerEvent { player, event: crate::event_ids::EVENT_PLAYER_END_CINEMATIC }),
                        Action::ArrowKey(k) => out.push(Command::PlayerEvent { player, event: crate::event_ids::EVENT_PLAYER_ARROW_LEFT_DOWN + k as i32 }),
                        Action::Cache { id, file, mission, key, value } => {
                            use crate::replay::CacheValue as V;
                            let word = value.get(..4).map(|b| [b[0], b[1], b[2], b[3]]);
                            let value = match id {
                                0x6B => V::Int(word.map(i32::from_le_bytes)),
                                0x6C => V::Real(word.map(f32::from_le_bytes)),
                                0x6D => V::Bool(word.map(|b| u32::from_le_bytes(b) != 0)),
                                0x6E => V::Unit(stored_unit(&value)),
                                _ => V::Int(None),
                            };
                            out.push(Command::CacheValue { cache: file, mission, key, value });
                        }
                        Action::CacheString => {}
                        Action::Trade { player: other, gold, lumber } => out.push(Command::Trade { player, other, gold: gold as i32, lumber: lumber as i32 }),
                        Action::SaveGame(name) => out.push(Command::Save { name }),
                        Action::SaveDone(_) => {}
                        Action::Pause => out.push(Command::Pause { player, on: true }),
                        Action::Resume => out.push(Command::Pause { player, on: false }),
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

/// A stored unit as the original's record (`w3_net::action::unit_record_len`): the one it
/// came in when it came from one; else its type, six item slots (type, charges, and a
/// third word, 0 here), and hero data: experience, its level less one, its skill points,
/// a word (its low half 10 for one Paladin, 0 for another, its high half 1 for both;
/// 0x10000 here), strength and its gain per level, agility, two reals (0, and 0.26 and
/// 0.32 for a Paladin of 13 and 16 agility; 0 here), its gain, intelligence and its gain,
/// five hero abilities and their levels, three reals (0, 0, and 1800 for the Paladins),
/// two words, a real, and a ten-bit mask (all 0 here).  Measured: a peon's and Paladins'
/// at level 1 and at level 3 with 537 experience, an item, and a skill learned twice.
fn unit_record(w: &World, u: &crate::types::StoredUnit) -> Vec<u8> {
    if !u.record.is_empty() {
        return u.record.clone();
    }
    let mut r = w3_net::wire::Writer::default();
    r.u32(u32::from_be_bytes(u.type_id)).u32(6);
    for k in 0..6 {
        let (t, charges) = u.items.get(k).copied().flatten().unwrap_or(([0; 4], 0));
        r.u32(u32::from_be_bytes(t)).u32(charges as u32).u32(0);
    }
    let data = w.objects.units.get(u.type_id);
    let gain = |k: &str| data.map_or(0.0f32, |o| o.real(k)).to_bits();
    let hero = data.map(|o| o.list("heroAbilList")).unwrap_or_default();
    r.u32(u.xp as u32).u32((u.level - 1).max(0) as u32).u32(u.skill_points as u32).u32(0x1_0000);
    r.u32(u.str_ as u32).u32(gain("STRplus")).u32(u.agi as u32).u32(0).u32(0).u32(gain("AGIplus")).u32(u.int as u32).u32(gain("INTplus"));
    let abilities: Vec<_> = u.abilities.iter().filter(|(id, _)| hero.iter().any(|s| crate::objects::id_from_str(s.trim()) == *id)).take(5).collect();
    r.u32(5);
    for k in 0..5 {
        let (id, level) = abilities.get(k).map_or(([0; 4], 0), |&&(id, l)| (id, l));
        r.u32(u32::from_be_bytes(id)).u32(level as u32);
    }
    r.u32(0).u32(0).u32(0).u32(2).u32(0).u32(0).u32(0).u16(0);
    r.0
}

/// A stored unit from the original's record, its fields this game reads taken and the
/// record kept.
fn stored_unit(b: &[u8]) -> Option<crate::types::StoredUnit> {
    let mut r = w3_net::wire::Reader::new(b);
    let word = |r: &mut w3_net::wire::Reader| r.u32().ok();
    let type_id = word(&mut r)?.to_be_bytes();
    let n = word(&mut r)? as usize;
    let mut items = Vec::new();
    for _ in 0..n {
        let (t, charges, _) = (word(&mut r)?, word(&mut r)?, word(&mut r)?);
        items.push((t != 0).then(|| (t.to_be_bytes(), charges as i32)));
    }
    let h: Vec<u32> = (0..12).map(|_| word(&mut r)).collect::<Option<_>>()?;
    let n = word(&mut r)? as usize;
    let mut abilities = Vec::new();
    for _ in 0..n {
        let (id, level) = (word(&mut r)?, word(&mut r)?);
        if id != 0 {
            abilities.push((id.to_be_bytes(), level as i32));
        }
    }
    Some(crate::types::StoredUnit {
        type_id,
        level: h[1] as i32 + 1,
        xp: h[0] as i32,
        skill_points: h[2] as i32,
        str_: h[4] as f32,
        agi: h[6] as f32,
        int: h[10] as f32,
        proper_name: String::new(),
        abilities,
        items,
        record: b.to_vec(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The original's record of a Paladin made at level 3, given 37 experience, Holy
    /// Light learned twice, a potion in its third slot and claws in its fifth, as
    /// SyncStoredUnit sent it.
    #[test]
    fn the_paladins_record_reads() {
        let s = "6c617048060000000000000000000000000000000000000000000000000000006568677001000000402700000000000000000000000000006674617200000000003500000000000000000000000000001902000002000000010000000a00010016000000cdcc2c400d000000000000000bd7a33e0100c03f110000006766e63f050000006268484102000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000e144020000000000000000000000000000000000";
        let b: Vec<u8> = (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect();
        let u = stored_unit(&b).unwrap();
        assert_eq!((&u.type_id, u.level, u.xp, u.skill_points), (b"Hpal", 3, 537, 1));
        assert_eq!((u.str_, u.agi, u.int), (22.0, 13.0, 17.0));
        assert_eq!(u.abilities, vec![(*b"AHhb", 2)]);
        assert_eq!(u.items[2], Some((*b"pghe", 1)));
        assert_eq!(u.items[4], Some((*b"ratf", 0)));
        assert_eq!(u.items.iter().filter(|i| i.is_some()).count(), 2);
        assert_eq!(u.record, b);
    }
}
