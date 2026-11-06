//! Starts a map as the original's census of it was taken (two players of random race,
//! a seed in hex) and compares the tags this game gives with the original's (a file of
//! type codes, one per line after comments), printing where they first part.
//!
//! tags <game dir> <map> <seed> <reference>
use std::path::Path;
fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("error")).init();
    let a: Vec<String> = std::env::args().collect();
    let data = war3::data::GameData::open(Path::new(&a[1]), false)?;
    let map = std::sync::Arc::new(w3_mpq::Archive::open(&a[2])?);
    let map_info = w3_formats::map::MapInfo::parse(&map.read("war3map.w3i")?)?;
    data.chain.mount(map, None, war3::data::PRIORITY_MAP);
    // The map's rule set, as a game of it has (`war3::scene`): melee maps' (w3i flag
    // 0x4) the melee one, others' the custom one.
    data.select_data_set(if map_info.flags & 0x4 != 0 { war3::data::DataSet::Melee } else { war3::data::DataSet::Custom });
    let seed = u32::from_str_radix(&a[3], 16)?;
    let reference: Vec<String> = std::fs::read_to_string(&a[4])?.lines().filter(|l| !l.starts_with('#')).map(|l| l.to_string()).collect();
    // TAGS_RACES="<race>,<race>": the two players' races (1 human to 4 night elf; empty
    // for random), as the host's W3HOSTRACE sets its own.
    let races: Vec<Option<i32>> = std::env::var("TAGS_RACES").map(|v| v.split(',').map(|r| r.trim().parse().ok()).collect()).unwrap_or_default();
    let mut lobby: Vec<w3_game::LobbySlot> = (0..2u8).map(|p| w3_game::LobbySlot { player: p, fill: if p == 0 { w3_game::SlotFill::Remote } else { w3_game::SlotFill::Local }, name: Some(format!("P{p}")), race: races.get(p as usize).copied().flatten(), team: p as i32, color: p as i32, handicap: 100.0, difficulty: 1 }).collect();
    // TAGS_COMPUTER="<race>,<level>": a computer player in the third slot, as the host's
    // W3HOSTCOMPUTER has it (race 1 human, 2 orc, 3 undead, 4 night elf as the script
    // numbers them; level 0 easy, 1 normal, 2 insane).
    if let Some((r, l)) = std::env::var("TAGS_COMPUTER").ok().and_then(|v| v.split_once(',').map(|(r, l)| (r.parse::<i32>().ok(), l.parse::<i32>().ok()))) {
        lobby.push(w3_game::LobbySlot { player: 2, fill: w3_game::SlotFill::Computer, name: None, race: r, team: 2, color: 2, handicap: 100.0, difficulty: l.unwrap_or(1) });
    }
    // TAGS_MAPLOBBY: the lobby the host's W3HOSTMAP offers, from the map's own players:
    // its first two users the host and the original, its other users' slots empty, its
    // computers computers; each slot's team its player's force with custom forces, its
    // color its player's, its race the map's with fixed player settings.
    if std::env::var("TAGS_MAPLOBBY").is_ok() {
        let fixed = map_info.flags & 0x20 != 0;
        let forces = map_info.flags & 0x40 != 0;
        let mut players: Vec<_> = map_info.players.iter().filter(|p| p.kind == 1 || p.kind == 2).collect();
        players.sort_by_key(|p| p.id);
        let mut users = 0;
        lobby = players
            .iter()
            .enumerate()
            .map(|(k, p)| {
                let team = if forces { map_info.forces.iter().position(|f| f.players & (1 << p.id) != 0).unwrap_or(0) as i32 } else { k as i32 };
                let race = if fixed { Some(p.race as i32) } else { races.get(users).copied().flatten() };
                let fill = match p.kind {
                    2 => w3_game::SlotFill::Computer,
                    _ => {
                        users += 1;
                        match users {
                            1 => w3_game::SlotFill::Remote,
                            2 => w3_game::SlotFill::Local,
                            _ => w3_game::SlotFill::Empty,
                        }
                    }
                };
                let name = matches!(fill, w3_game::SlotFill::Remote | w3_game::SlotFill::Local).then(|| format!("P{}", p.id));
                w3_game::LobbySlot { player: p.id as u8, fill, name, race, team, color: p.id as i32, handicap: 100.0, difficulty: 1 }
            })
            .collect();
    }
    w3_game::tags::RECORD.store(true, std::sync::atomic::Ordering::Relaxed);
    // TAGS_DRAWS: print each of the main generator's draws as it is made, from the load.
    if std::env::var("TAGS_DRAWS").is_ok() {
        w3_game::rng::TRACE.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    let mut g = w3_game::Game::load_for(data.chain.clone(), data.version as i32, None, w3_game::Start::of(&lobby))?;
    g.config()?;
    g.world.seed = seed;
    // TAGS_FLAGS: the setup's advanced options in hex (war3::advanced: 0x648 by default;
    // 0x608 without Teams Together, as the host's W3HOSTSETTINGS=0x60802 has it).
    let flags = std::env::var("TAGS_FLAGS").ok().and_then(|v| u32::from_str_radix(v.trim_start_matches("0x"), 16).ok()).unwrap_or(war3::advanced::DEFAULT_FLAGS);
    g.start_lobby(&lobby, flags)?;
    // TAGS_TURNS: how many turns of 0.1 seconds to play (one by default).
    let turns: usize = std::env::var("TAGS_TURNS").ok().and_then(|v| v.parse().ok()).unwrap_or(1);
    // TAGS_FIRST: the first turn's length (the original's first turn runs what falls
    // due at once without its clock moving, it seems).
    let first: f32 = std::env::var("TAGS_FIRST").ok().and_then(|v| v.parse().ok()).unwrap_or(0.1);
    // TAGS_CKS names a file for, after each turn, the keepalive checksum's inputs the
    // port has: the next tag (presence), the birth count, the main generator's state,
    // and the rand module's value.
    let mut cks = Vec::new();
    // TAGS_KEEPALIVE names the original's keepalives (turn and checksum a line) to
    // compare the port's with, the modules it does not compute taken as the original's
    // in Booty Bay (read under winedbg at its first turns).
    let keepalives: std::collections::BTreeMap<u32, u32> = std::env::var("TAGS_KEEPALIVE").ok().map(|f| {
        std::fs::read_to_string(f).unwrap_or_default().lines().filter(|l| !l.starts_with('#')).filter_map(|l| {
            let mut w = l.split_whitespace();
            Some((w.next()?.parse().ok()?, u32::from_str_radix(w.next()?, 16).ok()?))
        }).collect()
    }).unwrap_or_default();
    g.world.keepalive_modules = war3::net::data_modules(&data.chain);
    // TAGS_ACTIONS names a file of actions to play, "turn pid hex" a line (the turn as
    // the keepalives count them, the player as the network numbers them, from 1, and
    // the block's actions as the original sent them); TAGS_MODULE prints the game
    // module after each turn with actions.
    let mut played: std::collections::BTreeMap<u32, Vec<(u8, Vec<u8>)>> = std::collections::BTreeMap::new();
    if let Ok(f) = std::env::var("TAGS_ACTIONS") {
        for l in std::fs::read_to_string(f)?.lines().filter(|l| !l.starts_with('#')) {
            let mut w = l.split_whitespace();
            let (Some(turn), Some(pid), Some(hex)) = (w.next(), w.next(), w.next()) else { continue };
            let bytes = (0..hex.len() / 2).map(|i| u8::from_str_radix(&hex[2 * i..2 * i + 2], 16)).collect::<Result<Vec<u8>, _>>()?;
            played.entry(turn.parse()?).or_default().push((pid.parse()?, bytes));
        }
    }
    let mut first_off = None;
    for k in 0..turns {
        let n = k as u32 + 1;
        let acts = played.get(&n).cloned().unwrap_or_default();
        let dt = if k == 0 { first } else { 0.1 };
        if acts.is_empty() {
            g.world.net_turn(100);
        } else {
            // Carried out as the message comes in, before the keepalive.
            g.world.net_turn_actions(100, &acts);
            let actions = acts.iter().map(|(pid, b)| (pid - 1, b.clone())).collect();
            g.play_commands(&w3_game::replay::Frame { dt, commands: Vec::new(), actions: Some(actions), turn: None, checksum: None });
            if std::env::var("TAGS_MODULE").is_ok() {
                println!("turn {n}: game module {:08x}", g.world.game_module());
            }
        }
        if std::env::var("TAGS_MODULE").is_ok_and(|v| v == "all") {
            println!("turn {n}: game module {:08x}", g.world.game_module());
        }
        // TAGS_KEEPALIVE_ALL: each turn's keepalive against the original's after the first
        // that differs too.
        if let (Some(&want), true) = (keepalives.get(&n), first_off.is_some() && std::env::var("TAGS_KEEPALIVE_ALL").is_ok()) {
            let ours = g.world.keepalive();
            println!("turn {n}: ours {ours:08x}, the original's {want:08x}{}", if ours == want { "" } else { " (differ)" });
        }
        if let (Some(&want), None) = (keepalives.get(&n), first_off) {
            let ours = g.world.keepalive();
            if ours != want {
                first_off = Some((n, ours, want));
                println!("turn {n}: the original's keepalive wants game module {:08x}, ours {:08x} (keepalives: ours {ours:08x}, the original's {want:08x}; game ms {}, born {}, next slot {}, free {:?})", g.world.game_module_for(want), g.world.game_module(), g.world.game_ms, g.world.tags.born, g.world.tags.free.last().copied().unwrap_or(g.world.tags.slots), &g.world.tags.free[g.world.tags.free.len().saturating_sub(12)..]);
                // TAGS_TECH: each player's tech registrations and game module inputs then.
                if std::env::var("TAGS_TECH").is_ok() {
                    for (p, pl) in g.world.players.iter().enumerate().take(12) {
                        let regs: Vec<String> = pl.tech_regs.iter().map(|r| format!("{}:{}/{}/{}/{}/{}/{}/{:x}", w3_game::objects::id_str(r.id), r.max, r.made, r.producing, r.changing, r.present, r.killed, r.flags)).collect();
                        println!("tech p{p} handicap {:08x} xp {:08x} selection {:08x} hash {:08x} regs {}", pl.handicap.to_bits(), pl.handicap_xp.to_bits(), g.world.sync_selection_hash(p as u8), g.world.tech_hash(p as u8), regs.join(" "));
                    }
                }
                let presence = g.world.tags.free.last().copied().unwrap_or(g.world.tags.slots);
                let state = g.world.rng.state();
                let h = w3_game::keepalive::byte_hash((state >> 32) as u32).wrapping_add(w3_game::keepalive::byte_hash(state as u32));
                println!("  time {} presence {presence:#x} born {:#x} rng {h:#x} net {:08x} rand {:08x} modules {:x?}", g.world.game_ms, g.world.tags.born, g.world.net_sum, g.world.rand_data.checksum(), g.world.keepalive_modules);
                // (Each player in the game: a computer's too, its registrations listed.)
                for p in 0..3u8 {
                    let pl = &g.world.players[p as usize];
                    println!("  player {p}: race {} handicap {:08x} xp {:08x} tech {:08x} selection {:08x}", pl.race, pl.handicap.to_bits(), pl.handicap_xp.to_bits(), g.world.tech_hash(p), g.world.sync_selection_hash(p));
                    if pl.controller == w3_game::types::map_control::COMPUTER {
                        for r in &pl.tech_regs {
                            println!("    {} max {} made {} producing {} changing {} present {} killed {} flags {}", String::from_utf8_lossy(&r.id), r.max, r.made, r.producing, r.changing, r.present, r.killed, r.flags);
                        }
                    }
                }
            }
        }
        if std::env::var("TAGS_DRAWS").is_ok() {
            eprintln!("TURN {n} main {:p}", &g.world.rng);
        }
        // TAGS_DETECT: at that turn, the units within 900 of each flyer's detection.
        if std::env::var("TAGS_DETECT").ok().and_then(|v| v.parse::<u32>().ok()) == Some(n) {
            let w = &g.world;
            let mut all = std::collections::BTreeSet::new();
            for &h in &w.unit_order {
                let Some(d) = w.unit(h).filter(|u| u.extra.flyer_pil.is_some()) else { continue };
                let near: Vec<u32> = w.unit_order.iter().copied().filter(|&o| w.unit(o).is_some_and(|u| !u.dead && (u.x - d.x).hypot(u.y - d.y) <= 900.0)).collect();
                println!("detector {h} {} sleeping {} at {:.0},{:.0}: {} near", w3_game::objects::id_str(d.type_id), d.extra.sleeping, d.x, d.y, near.len());
                all.extend(near);
            }
            println!("distinct units near detectors: {}", all.len());
        }
        g.tick(dt);
        // TAGS_STEPS: each turn's movement steps' clocks.
        if std::env::var("TAGS_STEPS").is_ok() {
            println!("turn {n} steps {:?} ticks {:?} of {:?}", g.world.step_clocks.iter().map(|&c| f32::from_bits(c)).collect::<Vec<_>>(), g.world.step_ticks, (g.world.tick_clocks.0, g.world.tick_clocks.0 + g.world.tick_clocks.1.len() as i64 - 2));
        }
        // TAGS_UNITS: at the first turn, every unit near the watched place (x y, world):
        // its id, type, place, collision, and footprint's cells' bounds.
        if let (0, Ok(v)) = (k, std::env::var("TAGS_UNITS")) {
            let w = &g.world;
            let at: Vec<f32> = v.split(',').filter_map(|s| s.parse().ok()).collect();
            let o = w.pathing.as_ref().map_or([0.0; 2], |p| p.origin);
            for u in w.unit_order.iter().filter_map(|&h| w.unit(h)).filter(|u| (u.x - at[0]).hypot(u.y - at[1]) < 800.0) {
                let (x0, x1) = (u.pathing_cells.iter().map(|c| c.0).min(), u.pathing_cells.iter().map(|c| c.0).max());
                let (y0, y1) = (u.pathing_cells.iter().map(|c| c.1).min(), u.pathing_cells.iter().map(|c| c.1).max());
                println!("unit {:x} {} owner {} at {} {} (cells {} {}) collision {} footprint x {x0:?}-{x1:?} y {y0:?}-{y1:?}", u.extra.oid.0, w3_game::objects::id_str(u.type_id), u.owner, u.x, u.y, (u.x - o[0]) / 32.0, (u.y - o[1]) / 32.0, u.collision);
            }
        }
        // TAGS_CELLS="x0,y0,x1,y1[,turn]": at the first turn (or the given one), the
        // pathing cells there open to walkers ('.') or not ('#'), top row first.
        let cells_at = std::env::var("TAGS_CELLS").ok().and_then(|v| v.split(',').nth(4).and_then(|s| s.parse::<usize>().ok())).unwrap_or(0);
        if let (true, Ok(v)) = (k == cells_at, std::env::var("TAGS_CELLS")) {
            let b: Vec<i32> = v.split(',').take(4).filter_map(|s| s.parse().ok()).collect();
            if let (Some(p), [x0, y0, x1, y1]) = (g.world.pathing.as_ref(), b.as_slice()) {
                for y in (*y0..=*y1).rev() {
                    let row: String = (*x0..=*x1).map(|x| if p.passable((x, y), w3_game::pathing::Move::Foot) { '.' } else { '#' }).collect();
                    println!("cells {y}: {row}");
                }
            }
        }
        // TAGS_ACCEL="sx,sy,gx,gy,turn[,limit]": at that turn, the accelerator plan's
        // coarse way from the start to the goal (accelerator units, half cells).
        if let Some(v) = std::env::var("TAGS_ACCEL").ok() {
            let a: Vec<f32> = v.split(',').filter_map(|s| s.parse().ok()).collect();
            if a.len() >= 5 && a[4] as u32 == n {
                if let Some(p) = g.world.accel_pyramid() {
                    let way = w3_game::accelpath::plan(&p, 0, (a[0], a[1]), (a[2], a[3]), a.get(5).map_or(5000, |&l| l as u32));
                    println!("accel way found {} {:?}", way.found, way.points);
                }
            }
        }
        // TAGS_RES=player: that player's gold and lumber at every turn's end.
        if let Some(p) = std::env::var("TAGS_RES").ok().and_then(|v| v.parse::<usize>().ok()) {
            if let Some(pl) = g.world.players.get(p) {
                let st = |k: i32| pl.state.get(&k).copied().unwrap_or(0);
                println!("res {n} {} {}", st(w3_game::types::player_state::RESOURCE_GOLD), st(w3_game::types::player_state::RESOURCE_LUMBER));
            }
        }
        // TAGS_UNITS=turn,player: at that turn, the player's units' actions and AI state.
        if let Some((t, p)) = std::env::var("TAGS_UNITS").ok().and_then(|v| v.split_once(',').and_then(|(a, b)| Some((a.parse::<u32>().ok()?, b.parse::<u8>().ok()?)))) {
            if t == n {
                let w = &g.world;
                for u in w.unit_order.iter().filter_map(|&h| w.unit(h)).filter(|u| u.owner == p) {
                    println!("unit {:#x} {} at ({:.1}, {:.1}) {:?} ai_order {} build_end {:?} harvest_walk {} gold_run {:?}", u.handle, w3_game::objects::id_str(u.type_id), u.x, u.y, u.action, u.extra.ai_order.len(), u.extra.build_end, u.ai_harvest_walk(), u.extra.gold_run.as_ref().map(|r| r.phase));
                }
            }
        }
        // TAGS_TECH=turn: at that turn, every player's tech registrations.
        if std::env::var("TAGS_TECH").ok().and_then(|v| v.parse::<u32>().ok()) == Some(n) {
            for (p, pl) in g.world.players.iter().enumerate() {
                for r in &pl.tech_regs {
                    println!("tech {p} {} {} {} {} {} {} {} {}", w3_game::objects::id_str(r.id), r.max, r.made, r.producing, r.changing, r.present, r.killed, r.flags);
                }
            }
        }
        // TAGS_WANDERS=turn: at that turn, every wanderer's next wander due and its action.
        if std::env::var("TAGS_WANDERS").ok().and_then(|v| v.parse::<u32>().ok()) == Some(n) {
            let w = &g.world;
            for u in w.unit_order.iter().filter_map(|&h| w.unit(h)).filter(|u| u.extra.wander_at.is_some()) {
                println!("wanderer {:#x} {} at ({:.1}, {:.1}) due {:?} {:?}", u.handle, w3_game::objects::id_str(u.type_id), u.x, u.y, u.extra.wander_at.map(f32::from_bits), u.action);
            }
        }
        // TAGS_OIDS: at the given turn, every unit's id and type.
        if std::env::var("TAGS_OIDS").ok().and_then(|v| v.parse::<u32>().ok()) == Some(n) {
            let w = &g.world;
            for u in w.unit_order.iter().filter_map(|&h| w.unit(h)) {
                println!("oid {:x} {:x} {} owner {}", u.extra.oid.0, u.extra.oid.1, w3_game::objects::id_str(u.type_id), u.owner);
            }
        }
        // TAGS_WATCH: a unit's place, action, and hiding after each turn, by its id's
        // slot or its handle (hex).
        if let Some(slot) = std::env::var("TAGS_WATCH").ok().and_then(|v| u32::from_str_radix(&v, 16).ok()) {
            let w = &g.world;
            if let Some(u) = w.unit_order.iter().filter_map(|&h| w.unit(h)).find(|u| u.extra.oid.0 == slot || u.handle == slot) {
                let o = w.pathing.as_ref().map_or([0.0; 2], |p| p.origin);
                println!("turn {n} watch {:.3} {:.3} (cells {:.4} {:.4}) facing {:.2} {:?} hidden {} gold {} ai order {} legacy {} walker {}", u.x, u.y, (u.x - o[0]) / 32.0, (u.y - o[1]) / 32.0, u.facing, u.action, u.hidden, u.carry_gold, u.extra.ai_order.len(), u.extra.build_legacy, u.extra.mover.is_some());
            }
        }
        let t = &g.world.tags;
        let presence = t.free.last().copied().unwrap_or(t.slots);
        cks.push(format!("{} {:08x} {:08x} {:016x} {:08x}", k + 1, presence, t.born, g.world.rng.state(), g.world.rand_data.checksum()));
    }
    if !keepalives.is_empty() {
        match first_off {
            Some((n, ours, want)) => println!("keepalives agree to turn {}; turn {n}: ours {ours:08x}, the original's {want:08x}", n - 1),
            None => println!("keepalives agree for all {} turns compared", keepalives.len().min(turns)),
        }
    }
    if let Ok(out) = std::env::var("TAGS_CKS") {
        std::fs::write(out, cks.join("\n") + "\n")?;
    }
    let ours: Vec<String> = g.world.tags.log.clone().unwrap_or_default().iter().map(|c| String::from_utf8_lossy(c).to_string()).collect();
    println!("ours {} tags, the original's {}", ours.len(), reference.len());
    // TAGS_OUT names a file for ours, one per line, to compare further than the first
    // difference.
    if let Ok(out) = std::env::var("TAGS_OUT") {
        std::fs::write(out, ours.join("\n") + "\n")?;
    }
    // TAGS_TRACE names a file for every tag given and freed, by turn: "A code slot",
    // "F slot", "T".
    if let Ok(out) = std::env::var("TAGS_TRACE") {
        let lines: Vec<String> = g.world.tags.trace.clone().unwrap_or_default().iter().map(|e| match e {
            w3_game::tags::TagEvent::Take(c, s) => format!("A {} {s:#x}", String::from_utf8_lossy(c)),
            w3_game::tags::TagEvent::Free(s) => format!("F {s:#x}"),
            w3_game::tags::TagEvent::Turn => "T".into(),
            w3_game::tags::TagEvent::Step(k) => format!("S {k}"),
        }).collect();
        std::fs::write(out, lines.join("\n") + "\n")?;
    }
    match ours.iter().zip(&reference).position(|(a, b)| a != b) {
        Some(k) => {
            println!("first difference at tag {k}:");
            let from = k.saturating_sub(8);
            println!("  ours     {}", ours[from..(k + 12).min(ours.len())].join(" "));
            println!("  original {}", reference[from..(k + 12).min(reference.len())].join(" "));
        }
        None => println!("the same as far as both go"),
    }
    Ok(())
}
