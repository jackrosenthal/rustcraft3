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
    /// 0x60: a chat line one of the map's chat event registrations hears, by the
    /// registration's id (one action for each that matches, in their order; measured).
    TriggerChat { event: ObjectId, text: String },
    /// 0x61: Escape (sent with or without a registration for it; measured).
    Escape,
    /// 0x62: a trigger's sync (TriggerSyncReady and the like).
    TriggerSync { a: u32, b: u32, c: u32 },
    /// 0x66, 0x67: the skill or build menu is opened.
    SkillMenu,
    BuildMenu,
    /// 0x68: a minimap signal.
    Signal { x: f32, y: f32, a: u32 },
    /// 0x64, 0x65: a trackable clicked or moved onto, by the hit or track event
    /// registration's id (one action for each registration; measured).
    TrackableHit(ObjectId),
    TrackableTrack(ObjectId),
    /// 0x6A then 0x69: a dialog's button clicked, the dialog and the button by their ids
    /// (measured: both for each click, 0x6A's naming the dialog first).
    DialogButton { button: ObjectId, dialog: ObjectId },
    DialogClick { dialog: ObjectId, button: ObjectId },
    /// 0x6B to 0x6E: a game cache value (Sync Stored*): its file, mission, and key, and
    /// the value's bytes as sent (a unit's record for 0x6E, `unit_record_len`); 0x70 to
    /// 0x73: an integer, real, boolean, or unit the sender did not have, with none.
    Cache { id: u8, file: String, mission: String, key: String, value: Vec<u8> },
    /// 0x6F and 0x74: SyncStoredString of a string had and not had, sent bare: no file,
    /// mission, key, or value (measured).
    CacheString(u8),
    /// 0x75: an arrow key's press or release (left, right, down, up; each a press then
    /// a release), sent only for events registered (measured).
    ArrowKey(u8),
}

/// The length of a stored unit's record as SyncStoredUnit sends it, the same as a
/// saved cache holds (`w3_formats::w3v::unit_record_len`).  A peon's is 90 bytes
/// (measured).
pub fn unit_record_len(mut r: Reader) -> Result<usize, Error> {
    let rest = r.rest();
    w3_formats::w3v::unit_record_len(rest).ok_or(Error::Short)
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
            0x60 => A::TriggerChat { event: object(r)?, text: r.cstr()? },
            0x61 => A::Escape,
            0x62 => A::TriggerSync { a: r.u32()?, b: r.u32()?, c: r.u32()? },
            0x66 => A::SkillMenu,
            0x67 => A::BuildMenu,
            0x68 => A::Signal { x: f32(r)?, y: f32(r)?, a: r.u32()? },
            0x64 => A::TrackableHit(object(r)?),
            0x65 => A::TrackableTrack(object(r)?),
            0x69 => A::DialogButton { button: object(r)?, dialog: object(r)? },
            0x6A => A::DialogClick { dialog: object(r)?, button: object(r)? },
            0x6F | 0x74 => A::CacheString(id),
            0x6B..=0x73 => {
                let (file, mission, key) = (r.cstr()?, r.cstr()?, r.cstr()?);
                // Integers, reals, and booleans carry four bytes; a clear none.
                let n = match id {
                    0x6B..=0x6D => 4,
                    0x6E => unit_record_len(r.clone())?,
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
            A::TriggerChat { event, text } => {
                w.u8(0x60);
                obj(w, event);
                w.cstr(text);
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
            A::TrackableHit(o) => {
                w.u8(0x64);
                obj(w, o);
            }
            A::TrackableTrack(o) => {
                w.u8(0x65);
                obj(w, o);
            }
            A::DialogButton { button, dialog } => {
                w.u8(0x69);
                obj(w, button);
                obj(w, dialog);
            }
            A::DialogClick { dialog, button } => {
                w.u8(0x6A);
                obj(w, dialog);
                obj(w, button);
            }
            A::Cache { id, file, mission, key, value } => {
                w.u8(*id).cstr(file).cstr(mission).cstr(key).bytes(value);
            }
            A::CacheString(id) => {
                w.u8(*id);
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

    fn hex(s: &str) -> Vec<u8> {
        (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
    }

    fn round_trip(block: &[u8]) -> Vec<Action> {
        let all = Action::read_all(block).unwrap();
        let mut w = Writer::default();
        for a in &all {
            a.write(&mut w);
        }
        assert_eq!(w.0, block);
        all
    }

    /// The original's blocks on a test map with triggers on each: a trackable moved
    /// onto and clicked (its two hit registrations), Escape, an arrow key, a chat line
    /// three registrations hear, a dialog's button, and game cache syncs (an integer, a
    /// real, a boolean, a string, a peon, then a missing key and a level 3 Paladin).
    #[test]
    fn the_originals_trigger_actions() {
        let o = |n: u32| ObjectId(n, n);
        assert_eq!(round_trip(&hex("65892f0000892f0000")), vec![Action::TrackableTrack(o(0x2f89))]);
        assert_eq!(round_trip(&hex("647a2f00007a2f0000647b2f00007b2f0000")), vec![Action::TrackableHit(o(0x2f7a)), Action::TrackableHit(o(0x2f7b))]);
        assert_eq!(round_trip(&[0x61]), vec![Action::Escape]);
        assert_eq!(round_trip(&[0x75, 0x00, 0x75, 0x01]), vec![Action::ArrowKey(0), Action::ArrowKey(1)]);
        let chat = round_trip(&hex("60602f0000602f00007100606f2f00006f2f0000710060702f0000702f00007100"));
        assert_eq!(chat, [0x2f60, 0x2f6f, 0x2f70].map(|n| Action::TriggerChat { event: o(n), text: "q".into() }));
        let (d, b) = (o(0x2f76), ObjectId(0x305c, 0x3a47));
        assert_eq!(round_trip(&hex("6a762f0000762f00005c300000473a0000695c300000473a0000762f0000762f0000")), vec![Action::DialogClick { dialog: d, button: b }, Action::DialogButton { button: b, dialog: d }]);
        let cache = round_trip(&hex(&("6b65762e773376006d69006b6900d20400006c65762e773376006d72006b7200000020406d65762e773376006d62006b6200010000006f6e65762e77337600".to_owned() + "6d75006b75006f65706f" + &"00".repeat(68) + "02" + &"00".repeat(17))));
        assert_eq!(cache.len(), 5);
        assert_eq!(cache[3], Action::CacheString(0x6F));
        let Action::Cache { id: 0x6E, value, .. } = &cache[4] else { panic!() };
        assert_eq!(value.len(), 90);
        let hero = round_trip(&hex("6b65762e773376006d69006b6900d20400006c65762e773376006d72006b7200000020406d65762e773376006d62006b6200010000007065762e773376006d69006e6f6e65006e65762e773376006d75006b75006c617048060000000000000000000000000000000000000000000000000000006568677001000000402700000000000000000000000000006674617200000000003500000000000000000000000000001902000002000000010000000a00010016000000cdcc2c400d000000000000000bd7a33e0100c03f110000006766e63f050000006268484102000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000e144020000000000000000000000000000000000"));
        assert!(matches!(&hero[3], Action::Cache { id: 0x70, key, value, .. } if key == "none" && value.is_empty()));
        let Action::Cache { id: 0x6E, value, .. } = &hero[4] else { panic!() };
        assert_eq!(&value[..4], b"lapH");
        // A real, a boolean, a unit, and a string not stored, then the rest as before.
        let missing = round_trip(&hex("7165762e773376006d69006e72007265762e773376006d69006e62007365762e773376006d69006e7500746b65762e773376006d69006b6900d20400006c65762e773376006d72006b7200000020406d65762e773376006d62006b6200010000006f"));
        assert_eq!(missing.iter().take(4).map(|a| match a { Action::Cache { id, value, .. } if value.is_empty() => *id, Action::CacheString(id) => *id, _ => 0 }).collect::<Vec<_>>(), [0x71, 0x72, 0x73, 0x74]);
    }
}
