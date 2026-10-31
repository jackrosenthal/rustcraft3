//! The game's own state in a saved game (after the object manager, `FUN_6f1e8630`):
//! the parts the port makes, field by field as `w3_formats::w3z::GameState` reads them.

use w3_formats::w3z::{Field, GameState, Ref, SaveGame};

use crate::Game;
use crate::world::World;
use crate::codes;

impl World {
    /// The tagged object of a kind (its first), as references name it.
    pub(crate) fn first_of(&self, code: &[u8; 4]) -> Ref {
        self.tags.live.iter().filter(|(_, (c, _))| c == code).map(|(&s, (_, b))| Ref(s, *b)).min_by_key(|r| r.0).unwrap_or(Ref(u32::MAX, u32::MAX))
    }

    /// CGameWar3's first part (`FUN_6f1e8460`): two words (1, 1), its agent (`+gam`),
    /// its three name tables (`FUN_6f1d6c60`, each name and id in the order made): the
    /// unit types' names from UnitUI.slk's rows (spaces dropped; provisional: the
    /// original has the rocket engine after the siege engine, and the sea elemental
    /// after the revenant of the depths), none, and the orders' names; and two shorts.
    pub fn saved_game_part(&self) -> Vec<Field> {
        let mut units = Vec::new();
        if let Ok(text) = self.chain.read_string("Units\\UnitUI.slk") {
            let t = w3_formats::slk::Table::parse(&text);
            for id in &t.order {
                let Some(name) = t.get(id, "name").map(|v| v.as_str()) else { continue };
                let id4: [u8; 4] = id.as_bytes().try_into().unwrap_or([0; 4]);
                units.push(vec![Field::Text(name.replace(' ', "")), Field::Word(u32::from_be_bytes(id4))]);
            }
        }
        let orders = crate::orders::ORDERS.iter().map(|&(id, n)| vec![Field::Text(n.to_string()), Field::Word(id as u32)]).collect();
        vec![Field::Word(1), Field::Word(1), Field::Ref(self.first_of(codes::class::GAME_STATE)), Field::List(units), Field::List(Vec::new()), Field::List(orders), Field::Short(0), Field::Short(1)]
    }
}

impl World {
    /// The game's second part (CGameWar3's after the fog, `FUN_6f1e8460`): its twelve
    /// start locations (`FUN_6f1dbec0`: each its place, its priorities by index as
    /// SetStartLocPrio gave them (slot, priority), whether a playing slot's start, 0, -1, whether defined, 0), three words (the players, 12, 2), the
    /// players' agents (`+ply`), 12, each team's playing players, a word (1), a second
    /// table object's reference (the template's), and five words (3, 0, 0, 0, 0).
    /// Provisional: the words read as constants, a start's user flag, the teams' lists.
    pub fn saved_game_2(&self, template: &[Field]) -> Vec<Field> {
        // A playing slot's (PLAYER_SLOT_STATE_PLAYING, 1: a computer's too, an empty
        // slot's not; read: Forestwalk's empty third slot 0).
        let user = |k: usize| self.players.iter().take(12).any(|p| p.start_location == k as i32 && p.slot_state == 1);
        let starts = (0..12)
            .map(|k| match self.start_locations.get(k) {
                Some(s) if s.x != 0.0 || s.y != 0.0 || s.prio_count > 0 => {
                    let prios = (0..s.prio_count.max(0)).map(|i| s.prios.get(&i).copied().unwrap_or((0, 0))).map(|(slot, prio)| vec![Field::Word(slot as u32), Field::Word(prio as u32)]).collect();
                    vec![Field::Real(s.x), Field::Real(s.y), Field::List(prios), Field::Word(user(k) as u32), Field::Word(0), Field::Word(u32::MAX), Field::Word(1), Field::Word(0)]
                }
                _ => vec![Field::Real(0.0), Field::Real(0.0), Field::List(Vec::new()), Field::Word(0), Field::Word(0), Field::Word(u32::MAX), Field::Word(0), Field::Word(0)],
            })
            .collect();
        let mut plys: Vec<Ref> = self.tags.live.iter().filter(|(_, (c, _))| c == codes::class::PLAYER).map(|(&s, (_, b))| Ref(s, *b)).collect();
        plys.sort_by_key(|r| r.0);
        let teams = (0..12)
            .map(|t| vec![Field::List(self.players.iter().enumerate().filter(|(k, p)| *k < 12 && p.team == t && p.slot_state == 1).filter_map(|(k, _)| plys.get(k).map(|r| vec![Field::Ref(*r)])).collect())])
            .collect();
        let ui = template.get(8).cloned().unwrap_or(Field::Ref(Ref(0x8000_0000 | 1077, 1077)));
        vec![
            Field::List(starts),
            Field::Word(self.player_count as u32),
            Field::Word(12),
            Field::Word(2),
            Field::List(plys.iter().map(|r| vec![Field::Ref(*r)]).collect()),
            Field::Word(12),
            Field::List(teams),
            Field::Word(1),
            ui,
            Field::Word(3),
            Field::Word(0),
            Field::Word(0),
            Field::Word(0),
            Field::Word(0),
        ]
    }

    /// The game's third part: the game UI's (none), a list of strings (none), the game
    /// cache manager (`gccm`), and twelve words (the original's uninitialized memory but
    /// for its first: the template's, provisional).
    pub fn saved_game_3(&self, template: &[Field]) -> Vec<Field> {
        vec![Field::List(Vec::new()), Field::Ref(self.first_of(codes::class::GAME_CACHE_MANAGER)), template.get(2).cloned().unwrap_or(Field::List((0..12).map(|_| vec![Field::Word(0)]).collect()))]
    }

    /// The fog of war's part (CFogOfWarMap's, `FUN_6f250d00`): two bytes (1, 1), its
    /// size in cells, its rows' stride (a power of two) and that power, then four grids
    /// a short a cell, rows from the map's bottom (the stride's unused cells 0xffff in
    /// the first, 0 in the others): the players unexplored (a bit each; the neutral
    /// players' always), those seeing now, a third mask (the template's: not read), and the sight heights; its bounds
    /// (bottom, left, top, right), their scale (1/128) twice, three shorts (1), and a
    /// word (0).
    pub fn saved_fog(&self, template: &[Field]) -> Option<Vec<Field>> {
        let f = self.fog.as_ref()?;
        let (w, h) = (f.width, f.height);
        let stride = w.next_power_of_two();
        let shift = stride.trailing_zeros();
        let mut grids = [vec![0xffffu16; stride * h], vec![0u16; stride * h], vec![0u16; stride * h], vec![0u16; stride * h]];
        for y in 0..h {
            for x in 0..w {
                let (c, k) = (y * w + x, y * stride + x);
                let mut unexplored = 0xffffu16;
                let mut visible = 0u16;
                // The neutral players' bits never set (read: Neutral Hostile's creeps
                // leave none in a save's grids).
                for p in 0..12.min(f.explored.len()) {
                    if f.explored[p].get(c).copied().unwrap_or(false) {
                        unexplored &= !(1 << p);
                    }
                    if f.visible.get(p).and_then(|v| v.get(c)).copied().unwrap_or(false) {
                        visible |= 1 << p;
                    }
                }
                grids[0][k] = unexplored;
                grids[1][k] = visible;
                grids[3][k] = f.sight_height.get(c).copied().unwrap_or(0) as u16;
            }
        }
        if let Some(Field::List(t)) = template.get(8) {
            for (k, v) in t.iter().enumerate().take(grids[2].len()) {
                if let [Field::Short(x)] = v[..] {
                    grids[2][k] = x;
                }
            }
        }
        let b = self.map_bounds;
        let mut out = vec![Field::Byte(1), Field::Byte(1), Field::Word(w as u32), Field::Word(h as u32), Field::Word(stride as u32), Field::Word(shift)];
        for g in grids {
            out.push(Field::List(g.into_iter().map(|v| vec![Field::Short(v)]).collect()));
        }
        out.extend([Field::Real(b.min_y), Field::Real(b.min_x), Field::Real(b.max_y), Field::Real(b.max_x), Field::Real(1.0 / 128.0), Field::Real(1.0 / 128.0), Field::Short(1), Field::Short(1), Field::Short(1), Field::Word(0)]);
        Some(out)
    }

    /// The terrain's part (`FUN_6f78c900`): 1; 0 and a byte a terrain corner, 255 where
    /// its ground texture is the blight's tile (`FUN_6f757c80`; the tile `FUN_6f75c7f0`
    /// chooses, the map's ground tile of the lowest Terrain.slk blightPri, as the original
    /// draws blight: Booty Bay's rough dirt), which a load paints back (`FUN_6f74cc20`);
    /// 2, the doodads' animations set (none) and a byte a doodad (its flags' bits 26
    /// and 27; the template's: not read); 1, 10000, 0, and the deformations (none); 0
    /// and none.  Provisional: the doodads' bytes, deformations a game has.
    pub fn saved_terrain(&self, template: &[Field]) -> Option<Vec<Field>> {
        let t = w3_formats::map::Terrain::parse(&self.chain.read("war3map.w3e").ok()?).ok()?;
        let slk = w3_formats::slk::Table::parse(&self.chain.read_string("TerrainArt\\Terrain.slk").ok()?);
        let pri = |id: &[u8; 4]| slk.get(&String::from_utf8_lossy(id), "blightPri").and_then(|v| v.as_str().parse::<i32>().ok()).unwrap_or(1000);
        let mut blight = 0u8;
        let mut best = 1000;
        for (k, g) in t.ground_tilesets.iter().enumerate() {
            if pri(g) < best {
                (best, blight) = (pri(g), k as u8);
            }
        }
        let corners = t.corners.iter().map(|c| vec![Field::Byte(if c.ground_texture == blight { 255 } else { 0 })]).collect();
        let doodads = template.get(5).cloned().unwrap_or(Field::List(Vec::new()));
        Some(vec![
            Field::Word(1),
            Field::Word(0),
            Field::List(corners),
            Field::Word(2),
            Field::List(Vec::new()),
            doodads,
            Field::Word(1),
            // The next multiple of 10 seconds in milliseconds, 5000 before the first
            // frame (read: 5000 at 0, 10000 at 5, 20000 at 10, 30000 at 20 seconds;
            // provisional).
            Field::Word(if self.time < 0.001 { 5000 } else { ((self.time * 1000.0) as u32 / 10000 + 1) * 10000 }),
            Field::Word(0),
            Field::List(Vec::new()),
            Field::Word(0),
            Field::List(Vec::new()),
        ])
    }

    /// The sounds' part (`FUN_6f359b50`): the eight volume groups' levels (each the level
    /// it goes to), "Music", a word (-1), and the looping sounds playing on objects
    /// (none the port makes: provisional, Booty Bay's haunted mine's MineDomeLoop not
    /// written).
    pub fn saved_sounds(&self) -> Vec<Field> {
        vec![Field::List(self.volume_groups.iter().map(|g| vec![Field::Real(g.1)]).collect()), Field::Text("Music".into()), Field::Word(u32::MAX), Field::List(Vec::new())]
    }

    /// The game UI's part (as the test maps all have it; provisional: its fields not
    /// read): a byte (1), no references, words (1, 0), no cinematic scene, a word (0),
    /// three reals (3000, 5000, 0.5), a color (0xff000000), and two empty strings.
    pub fn saved_ui(&self) -> Vec<Field> {
        vec![Field::Byte(1), Field::List(Vec::new()), Field::Word(1), Field::Word(0), Field::Word(0), Field::Word(0), Field::Real(3000.0), Field::Real(5000.0), Field::Real(0.5), Field::Word(0xff00_0000), Field::Text(String::new()), Field::Text(String::new())]
    }

    /// The AI's town sites (`FUN_6f9ca1e0`): each site's AI (`AIts`) as made, and their
    /// count.
    pub fn saved_sites(&self) -> Vec<Field> {
        let refs: Vec<Vec<Field>> = self.tags.sites.iter().map(|&(_, _, (s, b))| vec![Field::Ref(Ref(s, b))]).collect();
        let n = refs.len() as u32;
        vec![Field::List(refs), Field::Word(n)]
    }
}

impl Game {
    /// The map setup's part (CMapSetupWar3's, `FUN_6f2465b0`): the map's name and
    /// description, its scripts' xoro checksum (as a game is announced with), a byte
    /// (1), its path, and words and bytes the test maps all have (1, 1, 0x02008000, 0,
    /// 0; 0, 0, 1, 2, 0, 2; provisional: not read).
    pub fn saved_map_setup(&self) -> Vec<Field> {
        let w = &self.world;
        let read = |n: &str| w.chain.read(n).ok();
        let (common, blizzard) = (read("Scripts\\common.j").unwrap_or_default(), read("Scripts\\Blizzard.j").unwrap_or_default());
        let xoro = w3_net::map::Checksums::compute(&[], &common, &blizzard, read).xoro;
        vec![
            Field::Text(w.localized(&self.info.name)),
            Field::Text(w.localized(&self.info.description)),
            Field::Word(xoro),
            Field::Byte(1),
            Field::Text(w.map_path.clone()),
            Field::Word(1),
            Field::Word(1),
            Field::Word(0x0200_8000),
            Field::Word(0),
            Field::Word(0),
            Field::Byte(0),
            Field::Byte(0),
            Field::Byte(1),
            Field::Byte(2),
            Field::Byte(0),
            Field::Byte(2),
        ]
    }

    /// The game's state with the parts the port makes in place of the template's.
    pub fn saved_state(&self, template: &GameState) -> GameState {
        let mut g = template.clone();
        for (name, f) in &mut g.parts {
            match *name {
                "game" => *f = self.world.saved_game_part(),
                "map setup" => *f = self.saved_map_setup(),
                "game 2" => *f = self.world.saved_game_2(f),
                "game 3" => *f = self.world.saved_game_3(f),
                "references" => *f = self.world.saved_sites(),
                "sounds" => *f = self.world.saved_sounds(),
                "terrain" => {
                    if let Some(x) = self.world.saved_terrain(f) {
                        *f = x;
                    }
                }
                "fog" => {
                    if let Some(x) = self.world.saved_fog(f) {
                        *f = x;
                    }
                }
                "ui" => *f = self.world.saved_ui(),
                // None the port makes yet (provisional: a game's hashtables not written).
                "hashtables" if !self.world.handles.iter().any(|(_, o)| matches!(o, crate::types::Obj::Hashtable(_))) => *f = vec![Field::List(Vec::new()), Field::Word(0)],
                _ => {}
            }
        }
        g
    }

    /// The game state's parts the port makes, against the original's save of the same
    /// moment.
    pub fn compare_state(&self, orig: &SaveGame) -> Vec<String> {
        let Ok(theirs) = orig.state() else { return vec!["state: not read".into()] };
        let ours = self.saved_state(&theirs);
        let mut out = Vec::new();
        if std::env::var("W3Z_FIND").is_ok() {
            for &h in &self.world.unit_order {
                if let Some(u) = self.world.unit(h).filter(|u| u.buffs.iter().any(|b| b.slot != 0) || u.extra.object_slots[2].iter().any(|s| self.world.tags.live.get(&s.0).is_some_and(|e| e.0[0] == b'B'))) {
                    let o = self.world.objects.units.get(u.type_id);
                    let dmg = o.map(|o| (o.int("dmgplus1"), o.int("dice1"), o.int("sides1"), o.int("dmgplus2"), o.int("dice2"), o.int("sides2"), o.int("weapsOn"), o.str("weapTp1").to_string(), o.str("weapTp2").to_string(), o.str("atkType1").to_string())).unwrap_or_default();
                    out.push(format!("unit {} {:?} owner {} slots {:?} dmg {dmg:?} buffs {:?}", crate::objects::id_str(u.type_id), h, u.owner, u.extra.object_slots[0].first(), u.buffs.iter().map(|b| (crate::objects::id_str(b.id), b.slot, b.caster)).collect::<Vec<_>>()));
                }
            }
            out.push(format!("players {:?}", self.world.players.iter().map(|p| (p.controller, p.start_location, p.team, p.race, p.race_preference, p.slot_state)).collect::<Vec<_>>()));
        }
        for ((name, a), (_, b)) in ours.parts.iter().zip(&theirs.parts) {
            if *name == "fog" {
                if let Ok(path) = std::env::var("W3Z_FOGDUMP") {
                    let mut raw = Vec::new();
                    for g in &a[6..10] {
                        if let Field::List(g) = g {
                            for v in g {
                                if let [Field::Short(x)] = v[..] {
                                    raw.extend(x.to_le_bytes());
                                }
                            }
                        }
                    }
                    let _ = std::fs::write(path, raw);
                }
                // Grid by grid: the cells differing, and the first.
                for k in 0..4 {
                    if let (Some(Field::List(x)), Some(Field::List(y))) = (a.get(6 + k), b.get(6 + k)) {
                        let differ: Vec<usize> = (0..x.len().max(y.len())).filter(|&j| x.get(j) != y.get(j)).collect();
                        out.push(format!("fog grid {k}: {} cells ours, {} the original's; {} differ{}", x.len(), y.len(), differ.len(), differ.first().map_or(String::new(), |&j| format!(", the first {j} (row {}, column {}): {:?} vs {:?}", j / 256, j % 256, x.get(j), y.get(j)))));
                    }
                }
                let (ha, hb) = (&a[..6.min(a.len())], &b[..6.min(b.len())]);
                let (ta, tb) = (a.get(10..).unwrap_or(&[]), b.get(10..).unwrap_or(&[]));
                if ha != hb || ta != tb {
                    out.push(format!("fog head {ha:?} vs {hb:?}; tail {ta:?} vs {tb:?}"));
                }
                continue;
            }
            if matches!(*name, "game" | "map setup" | "game 2" | "game 3" | "references" | "hashtables" | "sounds" | "ui" | "terrain") {
                let first = (0..a.len().max(b.len())).find(|&k| a.get(k) != b.get(k));
                out.push(format!("{name}: {}", first.map_or("the same".into(), |k| match (a.get(k), b.get(k)) {
                    (Some(Field::List(x)), Some(Field::List(y))) => {
                        let j = (0..x.len().max(y.len())).find(|&j| x.get(j) != y.get(j)).unwrap_or(0);
                        format!("field {k}: {} entries vs {}; first differing {j}: {:?} vs {:?}", x.len(), y.len(), x.get(j), y.get(j))
                    }
                    (x, y) => format!("field {k}: {x:?} vs {y:?}"),
                })));
            }
        }
        out
    }
}
