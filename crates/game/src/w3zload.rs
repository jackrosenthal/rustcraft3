//! What a saved game in the original's format holds beyond the simulation's objects,
//! put back as it loads (`Game::restore_original` calls `restore_presentation`): the
//! local player's camera and the hashtables; and the hashtables part as the port writes
//! it (`World::saved_hashtables`).

use w3_formats::w3z::{Field, First, SaveGame, Second};

use crate::Game;
use crate::types::{Hashtable, Obj};
use crate::world::World;

impl World {
    /// The hashtables' internal ids in the order they were made (their handles', none
    /// destroyed: the natives have no DestroyHashtable).
    pub(crate) fn hashtable_ids(&self) -> Vec<u32> {
        let mut ids: Vec<u32> = self.handles.iter().filter(|(_, o)| matches!(o, Obj::Hashtable(_))).map(|(h, _)| *h).collect();
        ids.sort_unstable();
        ids
    }

    /// The hashtables part (CGameHashTableManager, `FUN_6f246430`): each hashtable by
    /// its number (from 1, as made), its parent keys, and under each its integers,
    /// reals, booleans, strings, and handles by child key (`FUN_6f246d90`; a handle its
    /// handle's value and 1), then the count of hashtables made (read: a test map's two,
    /// a flushed child key gone, its handles a unit's, Player(1)'s, and a timer's).
    /// Provisional: the keys' order (written ascending; the test map's were made so).
    pub(crate) fn saved_hashtables(&self) -> Vec<Field> {
        let w = Field::Word;
        let ids = self.hashtable_ids();
        let mut tables = Vec::new();
        for (n, &id) in ids.iter().enumerate() {
            let Some(Obj::Hashtable(t)) = self.handles.get(id) else { continue };
            let mut parents: Vec<i32> = t.ints.keys().chain(t.reals.keys()).chain(t.bools.keys()).chain(t.strings.keys()).chain(t.handles.keys()).map(|k| k.0).collect();
            parents.sort_unstable();
            parents.dedup();
            let rows = parents
                .iter()
                .map(|&p| {
                    fn sorted<'a, V, S>(m: &'a std::collections::HashMap<(i32, i32), V, S>, p: i32) -> Vec<(i32, &'a V)> {
                        let mut v: Vec<(i32, &V)> = m.iter().filter(|(k, _)| k.0 == p).map(|(k, v)| (k.1, v)).collect();
                        v.sort_by_key(|e| e.0);
                        v
                    }
                    let handle = |h: u32| self.handles.known_handle(h).unwrap_or(h);
                    vec![
                        w(p as u32),
                        Field::List(sorted(&t.ints, p).into_iter().map(|(c, v)| vec![w(c as u32), w(*v as u32)]).collect()),
                        Field::List(sorted(&t.reals, p).into_iter().map(|(c, v)| vec![w(c as u32), Field::Real(*v)]).collect()),
                        Field::List(sorted(&t.bools, p).into_iter().map(|(c, v)| vec![w(c as u32), w(*v as u32)]).collect()),
                        Field::List(sorted(&t.strings, p).into_iter().map(|(c, v)| vec![w(c as u32), Field::Text(v.clone())]).collect()),
                        Field::List(sorted(&t.handles, p).into_iter().map(|(c, v)| vec![w(c as u32), w(handle(*v)), w(1)]).collect()),
                    ]
                })
                .collect();
            tables.push(vec![w(n as u32 + 1), Field::List(rows)]);
        }
        vec![Field::List(tables), w(ids.len() as u32)]
    }
}

impl Game {
    /// The save's hashtables (its hashtables part, each by the number its `ghth` agent
    /// names), in place of those the map's start made: each the hashtable its handle
    /// names now, else one made anew under that handle; its handles resolved.
    fn restore_hashtables(&mut self, s: &SaveGame) -> anyhow::Result<()> {
        let Ok(state) = s.state() else { return Ok(()) };
        let Some((_, part)) = state.parts.iter().find(|(n, _)| *n == "hashtables") else { return Ok(()) };
        let Some(Field::List(tables)) = part.first() else { return Ok(()) };
        let Some(m) = s.objects.as_ref() else { return Ok(()) };
        let objects = m.objects()?;
        let handles: std::collections::HashMap<(u32, u32), u32> = s.handles()?.into_iter().map(|(r, h)| ((r.0, r.1), h)).collect();
        let mut by_number: std::collections::HashMap<u32, u32> = Default::default();
        for o in objects[0].iter().filter(|o| &o.entry.kind.to_be_bytes() == crate::codes::class::HASHTABLE) {
            if let (Ok(Second::Agent { tail, .. }), Some(&jass)) = (o.second(), handles.get(&(o.entry.tag, o.entry.birth))) {
                if let Some(&Field::Word(n)) = tail.first() {
                    by_number.insert(n, jass);
                }
            }
        }
        let word = |f: Option<&Field>| match f {
            Some(Field::Word(v)) => *v,
            _ => 0,
        };
        let w = &mut self.world;
        for t in tables {
            let (Some(&jass), Some(Field::List(rows))) = (by_number.get(&word(t.first())), t.get(1)) else { continue };
            let mut table = Hashtable::default();
            for r in rows {
                let p = word(r.first()) as i32;
                let list = |k: usize| match r.get(k) {
                    Some(Field::List(g)) => g.clone(),
                    _ => Vec::new(),
                };
                for g in list(1) {
                    table.ints.insert((p, word(g.first()) as i32), word(g.get(1)) as i32);
                }
                for g in list(2) {
                    if let Some(&Field::Real(v)) = g.get(1) {
                        table.reals.insert((p, word(g.first()) as i32), v);
                    }
                }
                for g in list(3) {
                    table.bools.insert((p, word(g.first()) as i32), word(g.get(1)) != 0);
                }
                for g in list(4) {
                    if let Some(Field::Text(v)) = g.get(1) {
                        table.strings.insert((p, word(g.first()) as i32), v.clone());
                    }
                }
                for g in list(5) {
                    let v = word(g.get(1));
                    table.handles.insert((p, word(g.first()) as i32), if v == 0 { 0 } else { w.handles.resolve(v) });
                }
            }
            let id = w.handles.resolve(jass);
            match w.handles.get_mut(id) {
                Some(Obj::Hashtable(old)) => **old = table,
                _ => {
                    let h = w.handles.insert(Obj::Hashtable(Box::new(table)));
                    w.handles.alias(jass, h);
                }
            }
        }
        Ok(())
    }

    /// The save's presentation state over the map's start's: the game camera's target
    /// (the camera's second table position `` `pos `` at slot 1092, its target in
    /// pathing cells, `World::camera_records`; read, the hero map's at 300 seconds,
    /// (119.965454, 225.1696), its eye at 1089 28.8 cells behind).  Provisional: the
    /// camera's fields (its distance, angles, and height) stay the map's start's.
    pub(crate) fn restore_presentation(&mut self, s: &SaveGame) -> anyhow::Result<()> {
        let Some(m) = s.objects.as_ref() else { return Ok(()) };
        let objects = m.objects()?;
        let target = objects[1].iter().find(|o| o.entry.tag == 0x8000_0000 | 1092 && &o.entry.kind.to_be_bytes() == crate::codes::class::POSITION);
        if let (Some(Ok(First::Pos { values, .. })), Some(origin)) = (target.map(|o| o.first()), self.world.pathing.as_ref().map(|p| p.origin)) {
            let (x, y) = (origin[0] + values[0] * crate::pathing::CELL, origin[1] + values[1] * crate::pathing::CELL);
            self.world.camera.pan_to(x, y, 0.0);
        }
        self.restore_hashtables(s)
    }
}
