//! The original's actions: what a player's game sends for each command (OUTGOING_ACTION),
//! which the host's turns carry (INCOMING_ACTION) and replays record.  A player's block
//! of actions is these back to back.  Units and items are named by the original's object
//! ids, a pair of words each (`ObjectId`).

use crate::Error;
use crate::wire::{Reader, Writer};

/// An object as the original names it: its two id words.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct ObjectId(pub u32, pub u32);

impl ObjectId {
    pub const NONE: ObjectId = ObjectId(u32::MAX, u32::MAX);
}

/// An order's target point and object.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Target {
    pub x: f32,
    pub y: f32,
    pub object: ObjectId,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    /// 0x01 to 0x05: pause, resume, a speed, faster, slower.
    Pause,
    Resume,
    SetSpeed(u8),
    IncreaseSpeed,
    DecreaseSpeed,
    /// 0x06, 0x07: save the game under a name; the save is done.
    SaveGame(String),
    SaveDone(u32),
    /// 0x10: an order with no target, to the selection (`flags` as the interface sets
    /// them: queue 0x1, and so on).  `ability` is an order id or an object type
    /// (training, building); `a` and `b` are the ability's ids, or -1.
    Order { flags: u16, ability: u32, a: u32, b: u32 },
    /// 0x11: at a point.
    OrderPoint { flags: u16, ability: u32, a: u32, b: u32, x: f32, y: f32 },
    /// 0x12: at a point and object.
    OrderTarget { flags: u16, ability: u32, a: u32, b: u32, target: Target },
    /// 0x13: giving or dropping an item.
    OrderItem { flags: u16, ability: u32, a: u32, b: u32, target: Target, item: ObjectId },
    /// 0x14: an order with two targets (the second at `x2`, `y2`), its 9 middle bytes kept.
    OrderTwo { flags: u16, ability: u32, a: u32, b: u32, x: f32, y: f32, ability2: u32, middle: [u8; 9], x2: f32, y2: f32 },
    /// 0x16: the selection changes: `add` true adds, false removes.
    Select { add: bool, objects: Vec<ObjectId> },
    /// 0x17, 0x18: a control group is assigned, chosen.
    AssignGroup { group: u8, objects: Vec<ObjectId> },
    SelectGroup { group: u8, unknown: u8 },
    /// 0x19: the active subgroup, by a unit of its type.
    SelectSubgroup { kind: u32, object: ObjectId },
    /// 0x1A: before a subgroup choice.
    PreSubselection,
    /// 0x1B, 0x1C.
    Unknown1B { a: u8, object: ObjectId },
    SelectGroundItem { a: u8, object: ObjectId },
    /// 0x1D: a hero's revival is cancelled.
    CancelRevival(ObjectId),
    /// 0x1E: an entry leaves a structure's queue, by its place and type.
    Dequeue { slot: u8, kind: u32 },
    /// 0x20 to 0x32: cheats, their values when they have them.
    Cheat { id: u8, data: Vec<u8> },
    /// 0x50: alliance flags toward a player; 0x51: gold and lumber to one.
    Alliance { player: u8, flags: u32 },
    Trade { player: u8, gold: u32, lumber: u32 },
    /// 0x60: a chat line the map's triggers hear.
    TriggerChat { a: u32, b: u32, text: String },
    /// 0x61: Escape.
    Escape,
    /// 0x62: a trigger's sync (TriggerSyncReady and the like).
    TriggerSync { a: u32, b: u32, c: u32 },
    /// 0x66, 0x67: the skill or build menu is opened.
    SkillMenu,
    BuildMenu,
    /// 0x68: a minimap signal.
    Signal { x: f32, y: f32, a: u32 },
    /// 0x69, 0x6A: continuing the game after a cinematic or the like, 16 bytes each.
    ContinueB([u8; 16]),
    ContinueA([u8; 16]),
    /// 0x6B to 0x70: a game cache value (Sync Stored*): its file, mission, and key, and
    /// the value's bytes as sent.
    Cache { id: u8, file: String, mission: String, key: String, value: Vec<u8> },
    /// 0x75: an arrow key.
    ArrowKey(u8),
}

fn object(r: &mut Reader) -> Result<ObjectId, Error> {
    Ok(ObjectId(r.u32()?, r.u32()?))
}

fn f32(r: &mut Reader) -> Result<f32, Error> {
    Ok(f32::from_bits(r.u32()?))
}

fn objects(r: &mut Reader) -> Result<Vec<ObjectId>, Error> {
    let n = r.u16()?;
    (0..n).map(|_| object(r)).collect()
}

/// The bytes a cheat carries after its id (most none).
fn cheat_len(id: u8) -> usize {
    match id {
        // KeyserSoze and LeafitToMe: a byte and an amount; GreedIsGood the same.
        0x27 | 0x28 | 0x2D => 5,
        // DayLightSavings: a time of day.
        0x2E => 4,
        _ => 0,
    }
}

impl Action {
    /// A block of actions, read to its end.
    pub fn read_all(b: &[u8]) -> Result<Vec<Action>, Error> {
        let mut r = Reader::new(b);
        let mut out = Vec::new();
        while !r.is_empty() {
            out.push(Action::read(&mut r)?);
        }
        Ok(out)
    }

    pub fn read(r: &mut Reader) -> Result<Action, Error> {
        use Action as A;
        let id = r.u8()?;
        Ok(match id {
            0x01 => A::Pause,
            0x02 => A::Resume,
            0x03 => A::SetSpeed(r.u8()?),
            0x04 => A::IncreaseSpeed,
            0x05 => A::DecreaseSpeed,
            0x06 => A::SaveGame(r.cstr()?),
            0x07 => A::SaveDone(r.u32()?),
            0x10..=0x14 => {
                let (flags, ability, a, b) = (r.u16()?, r.u32()?, r.u32()?, r.u32()?);
                match id {
                    0x10 => A::Order { flags, ability, a, b },
                    0x11 => A::OrderPoint { flags, ability, a, b, x: f32(r)?, y: f32(r)? },
                    0x12 => A::OrderTarget { flags, ability, a, b, target: Target { x: f32(r)?, y: f32(r)?, object: object(r)? } },
                    0x13 => A::OrderItem { flags, ability, a, b, target: Target { x: f32(r)?, y: f32(r)?, object: object(r)? }, item: object(r)? },
                    _ => A::OrderTwo { flags, ability, a, b, x: f32(r)?, y: f32(r)?, ability2: r.u32()?, middle: r.array()?, x2: f32(r)?, y2: f32(r)? },
                }
            }
            0x16 => A::Select { add: r.u8()? == 1, objects: objects(r)? },
            0x17 => A::AssignGroup { group: r.u8()?, objects: objects(r)? },
            0x18 => A::SelectGroup { group: r.u8()?, unknown: r.u8()? },
            0x19 => A::SelectSubgroup { kind: r.u32()?, object: object(r)? },
            0x1A => A::PreSubselection,
            0x1B => A::Unknown1B { a: r.u8()?, object: object(r)? },
            0x1C => A::SelectGroundItem { a: r.u8()?, object: object(r)? },
            0x1D => A::CancelRevival(object(r)?),
            0x1E => A::Dequeue { slot: r.u8()?, kind: r.u32()? },
            0x20..=0x32 => A::Cheat { id, data: r.bytes(cheat_len(id))?.to_vec() },
            0x50 => A::Alliance { player: r.u8()?, flags: r.u32()? },
            0x51 => A::Trade { player: r.u8()?, gold: r.u32()?, lumber: r.u32()? },
            0x60 => A::TriggerChat { a: r.u32()?, b: r.u32()?, text: r.cstr()? },
            0x61 => A::Escape,
            0x62 => A::TriggerSync { a: r.u32()?, b: r.u32()?, c: r.u32()? },
            0x66 => A::SkillMenu,
            0x67 => A::BuildMenu,
            0x68 => A::Signal { x: f32(r)?, y: f32(r)?, a: r.u32()? },
            0x69 => A::ContinueB(r.array()?),
            0x6A => A::ContinueA(r.array()?),
            0x6B..=0x70 => {
                let (file, mission, key) = (r.cstr()?, r.cstr()?, r.cstr()?);
                // Integers, reals, and booleans carry four bytes; a clear none.
                let n = match id {
                    0x6B..=0x6D => 4,
                    _ => 0,
                };
                A::Cache { id, file, mission, key, value: r.bytes(n)?.to_vec() }
            }
            0x75 => A::ArrowKey(r.u8()?),
            id => return Err(Error::Bad(format!("action {id:#04x}"))),
        })
    }

    pub fn write(&self, w: &mut Writer) {
        use Action as A;
        let obj = |w: &mut Writer, o: &ObjectId| {
            w.u32(o.0).u32(o.1);
        };
        match self {
            A::Pause => {
                w.u8(0x01);
            }
            A::Resume => {
                w.u8(0x02);
            }
            A::SetSpeed(s) => {
                w.u8(0x03).u8(*s);
            }
            A::IncreaseSpeed => {
                w.u8(0x04);
            }
            A::DecreaseSpeed => {
                w.u8(0x05);
            }
            A::SaveGame(n) => {
                w.u8(0x06).cstr(n);
            }
            A::SaveDone(v) => {
                w.u8(0x07).u32(*v);
            }
            A::Order { flags, ability, a, b } => {
                w.u8(0x10).u16(*flags).u32(*ability).u32(*a).u32(*b);
            }
            A::OrderPoint { flags, ability, a, b, x, y } => {
                w.u8(0x11).u16(*flags).u32(*ability).u32(*a).u32(*b).u32(x.to_bits()).u32(y.to_bits());
            }
            A::OrderTarget { flags, ability, a, b, target } => {
                w.u8(0x12).u16(*flags).u32(*ability).u32(*a).u32(*b).u32(target.x.to_bits()).u32(target.y.to_bits());
                obj(w, &target.object);
            }
            A::OrderItem { flags, ability, a, b, target, item } => {
                w.u8(0x13).u16(*flags).u32(*ability).u32(*a).u32(*b).u32(target.x.to_bits()).u32(target.y.to_bits());
                obj(w, &target.object);
                obj(w, item);
            }
            A::OrderTwo { flags, ability, a, b, x, y, ability2, middle, x2, y2 } => {
                w.u8(0x14).u16(*flags).u32(*ability).u32(*a).u32(*b).u32(x.to_bits()).u32(y.to_bits()).u32(*ability2).bytes(middle).u32(x2.to_bits()).u32(y2.to_bits());
            }
            A::Select { add, objects } => {
                w.u8(0x16).u8(if *add { 1 } else { 2 }).u16(objects.len() as u16);
                objects.iter().for_each(|o| obj(w, o));
            }
            A::AssignGroup { group, objects } => {
                w.u8(0x17).u8(*group).u16(objects.len() as u16);
                objects.iter().for_each(|o| obj(w, o));
            }
            A::SelectGroup { group, unknown } => {
                w.u8(0x18).u8(*group).u8(*unknown);
            }
            A::SelectSubgroup { kind, object } => {
                w.u8(0x19).u32(*kind);
                obj(w, object);
            }
            A::PreSubselection => {
                w.u8(0x1A);
            }
            A::Unknown1B { a, object } => {
                w.u8(0x1B).u8(*a);
                obj(w, object);
            }
            A::SelectGroundItem { a, object } => {
                w.u8(0x1C).u8(*a);
                obj(w, object);
            }
            A::CancelRevival(o) => {
                w.u8(0x1D);
                obj(w, o);
            }
            A::Dequeue { slot, kind } => {
                w.u8(0x1E).u8(*slot).u32(*kind);
            }
            A::Cheat { id, data } => {
                w.u8(*id).bytes(data);
            }
            A::Alliance { player, flags } => {
                w.u8(0x50).u8(*player).u32(*flags);
            }
            A::Trade { player, gold, lumber } => {
                w.u8(0x51).u8(*player).u32(*gold).u32(*lumber);
            }
            A::TriggerChat { a, b, text } => {
                w.u8(0x60).u32(*a).u32(*b).cstr(text);
            }
            A::Escape => {
                w.u8(0x61);
            }
            A::TriggerSync { a, b, c } => {
                w.u8(0x62).u32(*a).u32(*b).u32(*c);
            }
            A::SkillMenu => {
                w.u8(0x66);
            }
            A::BuildMenu => {
                w.u8(0x67);
            }
            A::Signal { x, y, a } => {
                w.u8(0x68).u32(x.to_bits()).u32(y.to_bits()).u32(*a);
            }
            A::ContinueB(b) => {
                w.u8(0x69).bytes(b);
            }
            A::ContinueA(b) => {
                w.u8(0x6A).bytes(b);
            }
            A::Cache { id, file, mission, key, value } => {
                w.u8(*id).cstr(file).cstr(mission).cstr(key).bytes(value);
            }
            A::ArrowKey(k) => {
                w.u8(0x75).u8(*k);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every action in the original's replays (in the game's folder and the Wine
    /// reference install, which the repository does not hold) reads, to the end of its
    /// block, and writes back the same.
    #[test]
    fn the_originals_actions_read_and_write_back() {
        let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../Warcraft III/Replay");
        let Ok(dir) = std::fs::read_dir(root) else { return };
        let mut seen = std::collections::BTreeMap::new();
        for e in dir.flatten().filter(|e| e.path().extension().is_some_and(|x| x == "w3g")) {
            let r = w3_formats::w3g::Replay::parse(&std::fs::read(e.path()).unwrap()).unwrap();
            for rec in &r.records {
                let w3_formats::w3g::Record::Turn { actions, .. } = rec else { continue };
                for (_, block) in actions {
                    let all = Action::read_all(block).unwrap_or_else(|err| panic!("{}: {err} in {block:02x?}", e.path().display()));
                    let mut w = Writer::default();
                    for a in &all {
                        *seen.entry(block[0]).or_insert(0) += 1;
                        a.write(&mut w);
                    }
                    assert_eq!(&w.0, block);
                }
            }
        }
        eprintln!("actions seen by first id: {seen:x?}");
    }
}

#[cfg(test)]
mod measured {
    use super::*;

    /// The original's block as it accepted its Allies dialog toward player 0 (Ally,
    /// Share Vision, and Allied Victory ticked, 200 gold offered), sent to a hosted game.
    #[test]
    fn the_allies_dialog_sends_alliances_then_resources() {
        let block = [0x50, 0x00, 0x3f, 0x04, 0x00, 0x00, 0x51, 0x00, 0xc8, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        let all = Action::read_all(&block).unwrap();
        assert_eq!(all, vec![Action::Alliance { player: 0, flags: 0x43f }, Action::Trade { player: 0, gold: 200, lumber: 0 }]);
        let mut w = Writer::default();
        for a in &all {
            a.write(&mut w);
        }
        assert_eq!(w.0, block);
    }
}
