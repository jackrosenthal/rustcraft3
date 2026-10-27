//! Plays a replay the original wrote (.w3g) on its map without drawing: the game starts
//! with the replay's slots and seed, and each turn's actions are read as a network
//! game's are.  With RUST_LOG=w3_game=debug, each command read and its answer print.
//!
//! w3gplay <game dir> <map> <replay>
use std::path::Path;
use w3_formats::w3g;

fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("error")).init();
    let a: Vec<String> = std::env::args().collect();
    let data = war3::data::GameData::open(Path::new(&a[1]), false)?;
    let map = std::sync::Arc::new(w3_mpq::Archive::open(&a[2])?);
    data.chain.mount(map, None, war3::data::PRIORITY_MAP);
    let r = w3g::Replay::parse(&std::fs::read(&a[3])?)?;
    // Slot flags: race bits (human, orc, night elf, undead, random), occupied status 2,
    // computer difficulty 0 to 2.
    let race = |bits: u8| [(0x01, 1), (0x02, 2), (0x08, 3), (0x04, 4)].iter().find(|&&(b, _)| bits & b != 0).map(|&(_, r)| r);
    let lobby: Vec<w3_game::LobbySlot> = r
        .slots
        .iter()
        .enumerate()
        .map(|(k, s)| w3_game::LobbySlot {
            player: k as u8,
            fill: match (s.status, s.computer) {
                // W3GPLAY_HUMANS: computers played as remote people instead.
                (2, true) if std::env::var("W3GPLAY_HUMANS").is_ok() => w3_game::SlotFill::Remote,
                (2, true) => w3_game::SlotFill::Computer,
                (2, false) if s.pid == r.host.pid => w3_game::SlotFill::Local,
                (2, false) => w3_game::SlotFill::Remote,
                _ => w3_game::SlotFill::Empty,
            },
            name: (s.status == 2 && !s.computer).then(|| r.players.iter().chain([&r.host]).find(|p| p.pid == s.pid).map_or_else(String::new, |p| p.name.clone())),
            race: race(s.race),
            team: s.team as i32,
            color: s.color as i32,
            handicap: s.handicap as f32,
            difficulty: s.ai as i32,
        })
        .collect();
    let player_of = |pid: u8| r.slots.iter().position(|s| s.pid == pid && s.status == 2 && !s.computer).map(|k| k as u8);
    // W3GPLAY_TRACE names a file for every tag given and freed, by turn, as the tags
    // example writes it; W3GPLAY_MS stops the game that far in.
    let trace = std::env::var("W3GPLAY_TRACE").ok();
    w3_game::tags::RECORD.store(trace.is_some(), std::sync::atomic::Ordering::Relaxed);
    let stop: u32 = std::env::var("W3GPLAY_MS").ok().and_then(|v| v.parse().ok()).unwrap_or(u32::MAX);
    let mut g = w3_game::Game::load_with_keys(data.chain.clone(), data.version as i32, None)?;
    g.config()?;
    g.world.seed = r.seed;
    // The game's options are the settings word of its description.
    let (flags, _) = war3::net::from_settings(w3_net::stat::GameStat::parse(&r.stat)?.settings);
    g.start_lobby(&lobby, flags)?;
    let mut t = 0u32;
    for rec in &r.records {
        if let w3g::Record::Turn { ms, actions } = rec {
            t += *ms as u32;
            if t > stop {
                break;
            }
            let actions = actions.iter().filter_map(|(pid, b)| Some((player_of(*pid)?, b.clone()))).collect::<Vec<_>>();
            if !actions.is_empty() {
                log::debug!("at {t} ms");
            }
            let f = w3_game::replay::Frame { dt: *ms as f32 / 1000.0, commands: Vec::new(), actions: Some(actions) };
            g.play_frame(&f);
        }
    }
    if let Some(out) = trace {
        let lines: Vec<String> = g.world.tags.trace.clone().unwrap_or_default().iter().map(|e| match e {
            w3_game::tags::TagEvent::Take(c, s) => format!("A {} {s:#x}", String::from_utf8_lossy(c)),
            w3_game::tags::TagEvent::Free(s) => format!("F {s:#x}"),
            w3_game::tags::TagEvent::Turn => "T".into(),
        }).collect();
        std::fs::write(out, lines.join("\n") + "\n")?;
    }
    // W3GPLAY_UNITS: unit types (comma separated) whose ids to print at the end.
    if let Ok(types) = std::env::var("W3GPLAY_UNITS") {
        for &h in &g.world.unit_order {
            if let Some(u) = g.world.unit(h).filter(|u| types.split(',').any(|t| t.as_bytes() == u.type_id)) {
                println!("{} {:?} at {:.0},{:.0} dead {}", String::from_utf8_lossy(&u.type_id), u.extra.oid, u.x, u.y, u.dead);
            }
        }
    }
    Ok(())
}
