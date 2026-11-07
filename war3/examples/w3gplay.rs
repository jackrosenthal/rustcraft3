//! Plays one of the original's replays (.w3g) on the port: the game set up from its record
//! (the map its description names, its slots, its seed), each turn's actions carried out
//! as the network gives them, and each of its checksums compared with the port's.
//!
//! w3gplay <game dir> <replay> [map file]
use std::path::Path;
fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("error")).init();
    let a: Vec<String> = std::env::args().collect();
    let bytes = std::fs::read(&a[2])?;
    let replay = w3_formats::w3g::Replay::parse(&bytes)?;
    // The game as the port reads it: a map loaded directly when the record says so.
    let parsed = w3_game::replay::Replay::parse(&bytes)?;
    let stat = w3_net::GameStat::parse(&replay.stat)?;
    println!("version {} build {}, map {}, seed {:08x}, settings {:08x}, {} slots", replay.version, replay.build, stat.map_path, replay.seed, stat.settings, replay.slots.len());
    // W3G_TRACE names a file for every tag given and freed, by turn, as the tags
    // example's TAGS_TRACE: "A code slot", "F slot", "T", each keepalive "S 1000+turn".
    let trace = std::env::var("W3G_TRACE").ok();
    if trace.is_some() {
        w3_game::tags::RECORD.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    let data = war3::data::GameData::open(Path::new(&a[1]), false)?;
    let map_file = match a.get(3) {
        Some(m) => m.clone(),
        None => Path::new(&a[1]).join(stat.map_path.replace('\\', "/")).to_string_lossy().to_string(),
    };
    let map = std::sync::Arc::new(w3_mpq::Archive::open(&map_file)?);
    let map_info = w3_formats::map::MapInfo::parse(&map.read("war3map.w3i")?)?;
    data.chain.mount(map, None, war3::data::PRIORITY_MAP);
    // The map's rule set, as a game of it has (`war3::scene`): melee maps' (w3i flag
    // 0x4) the melee one, others' the custom one.
    data.select_data_set(if map_info.flags & 0x4 != 0 { war3::data::DataSet::Melee } else { war3::data::DataSet::Custom });
    // The slots as the record has them: its players by their ids, the host's on this
    // computer; races by their bits (human 1, orc 2, night elf 4, undead 8, random 0x20).
    // W3G_LOCAL: the player whose computer the game is played on (the host's by default).
    let local: u8 = std::env::var("W3G_LOCAL").ok().and_then(|v| v.parse().ok()).unwrap_or(replay.host.pid);
    let lobby: Vec<w3_game::LobbySlot> = replay
        .slots
        .iter()
        .enumerate()
        .map(|(k, s)| {
            let name = std::iter::once(&replay.host).chain(&replay.players).find(|p| p.pid == s.pid && s.pid != 0).map(|p| p.name.clone());
            let fill = if s.status != 2 {
                w3_game::SlotFill::Empty
            } else if s.computer {
                w3_game::SlotFill::Computer
            } else if s.pid == local {
                w3_game::SlotFill::Local
            } else {
                w3_game::SlotFill::Remote
            };
            let race = match s.race & 0x3f {
                1 => Some(1),
                2 => Some(2),
                8 => Some(3),
                4 => Some(4),
                _ => None,
            };
            w3_game::LobbySlot { player: k as u8, fill, name, race, team: s.team as i32, color: s.color as i32, handicap: s.handicap as f32, difficulty: s.ai as i32 }
        })
        .collect();
    // Its computers' objects made as the map loads, as a network game's lobby starts it
    // (or as a map loaded directly does).
    // (W3G_LOBBY: begun from the record's slots, as a setup screen's game, whatever it says.)
    let loaded = parsed.lobby.is_none() && std::env::var("W3G_LOBBY").is_err();
    let start = if loaded { w3_game::Start::without_setup(&map_info, false) } else { w3_game::Start::of(&lobby) };
    let mut g = w3_game::Game::load_for(data.chain.clone(), data.version as i32, None, start)?;
    g.config()?;
    g.world.seed = replay.seed;
    // The settings word holds the advanced options eight bits up, and the speed.
    let (flags, speed) = war3::net::from_settings(stat.settings);
    g.world.game_speed = speed.min(2) as i32;
    if loaded {
        // A map loaded directly, as the original's -loadfile begins it, its player the
        // record's.
        g.start_without_setup(false)?;
        let local = g.world.local_player as usize;
        if let Some(p) = g.world.players.get_mut(local) {
            p.name = replay.host.name.clone();
        }
    } else {
        g.start_lobby(&lobby, flags)?;
    }
    g.world.keepalive_modules = war3::net::data_modules(&data.chain);
    // W3G_WRITE names a file for the port's own replay of the game as it plays it, with
    // the record's setup.
    let write = std::env::var("W3G_WRITE").ok();
    if write.is_some() {
        g.recorder = Some(w3_game::replay::Recorder::new(w3_game::replay::Replay { lobby: Some(lobby.clone()).filter(|_| !loaded), frames: Vec::new(), ..parsed.clone() }));
    }
    // A network game's turns play as the client plays them (`Game::play_turn`); each
    // checksum record follows the turn whose state it sums.
    // The players who left (the record's leaves before a turn) are taken with it.
    let (mut turn, mut checked, mut first_off) = (0u32, 0u32, None);
    let (mut last, mut leaves) = (None, Vec::new());
    for rec in &replay.records {
        match rec {
            w3_formats::w3g::Record::Leave { pid, result, .. } => {
                let player = replay.slots.iter().position(|s| s.pid == *pid).unwrap_or(0) as u8;
                leaves.push(w3_game::replay::Leave { pid: *pid, player, word: *result });
            }
            w3_formats::w3g::Record::Turn { ms, actions } => {
                turn += 1;
                let acts = actions.iter().map(|(pid, b)| (pid - 1, b.clone())).collect();
                // W3G_NONET: the turns as a game played alone takes them, no message.
                let net = std::env::var("W3G_NONET").is_err().then(|| (*ms, actions.clone()));
                let frame = w3_game::replay::Frame { dt: *ms as f32 / 1000.0, actions: Some(acts), turn: net, leaves: std::mem::take(&mut leaves), ..Default::default() };
                if trace.is_some() {
                    g.world.tags.mark_step(1000 + turn as usize);
                }
                let (_, keepalive) = g.play_turn(&frame);
                last = Some(keepalive);
            }
            w3_formats::w3g::Record::Checksum(b) if b.len() == 4 => {
                let want = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
                checked += 1;
                if first_off.is_none() && last != Some(want) && std::env::var("W3G_DEBUG").is_ok() {
                    println!("turn {turn}: game module {:08x}, wanted {:08x}; net {:08x} rand {:08x} born {} free {:?} slots {} ms {} rng {:016x}", g.world.game_module(), g.world.game_module_for(want), g.world.net_sum, g.world.rand_data.checksum(), g.world.tags.born, g.world.tags.free.last(), g.world.tags.slots, g.world.game_ms, g.world.rng.state());
                    for (p, pl) in g.world.players.iter().enumerate().take(2) {
                        for r in &pl.tech_regs {
                            println!("player {p}: {} {} {} {} {} {} {} {}", String::from_utf8_lossy(&r.id), r.max, r.made, r.producing, r.changing, r.present, r.killed, r.flags);
                        }
                    }
                }
                if first_off.is_none() && last != Some(want) {
                    first_off = Some((turn, last, want));
                }
            }
            _ => {}
        }
    }
    match first_off {
        Some((t, ours, want)) => println!("checksums agree to turn {}; turn {t}: ours {:08x?}, the replay's {want:08x}", t.saturating_sub(1), ours),
        None => println!("checksums agree for all {checked} of {turn} turns"),
    }
    if std::env::var("W3G_DEBUG").is_ok() {
        println!("players removed, with their results: {:?}", g.world.player_results);
    }
    if let Some(out) = trace {
        let lines: Vec<String> = g.world.tags.trace.clone().unwrap_or_default().iter().map(|e| match e {
            w3_game::tags::TagEvent::Take(c, s) => format!("A {} {s:#x}", String::from_utf8_lossy(c)),
            w3_game::tags::TagEvent::Free(s) => format!("F {s:#x}"),
            w3_game::tags::TagEvent::Gone(s) => format!("f {s:#x}"),
            w3_game::tags::TagEvent::Turn => "T".into(),
            w3_game::tags::TagEvent::Step(k) => format!("S {k}"),
        }).collect();
        std::fs::write(out, lines.join("\n") + "\n")?;
    }
    if let (Some(path), Some(r)) = (write, g.recorder.take()) {
        std::fs::write(path, r.replay.to_bytes()?)?;
    }
    Ok(())
}
