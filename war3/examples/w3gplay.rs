//! Plays one of the original's replays (.w3g) on the port: the game set up from its record
//! (the map its description names, its slots, its seed), each turn's actions carried out
//! as the network gives them, and each of its checksums compared with the port's.
//!
//! w3gplay <game dir> <replay> [map file]
use std::path::Path;
fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("error")).init();
    let a: Vec<String> = std::env::args().collect();
    let replay = w3_formats::w3g::Replay::parse(&std::fs::read(&a[2])?)?;
    let stat = w3_net::GameStat::parse(&replay.stat)?;
    println!("version {} build {}, map {}, seed {:08x}, settings {:08x}, {} slots", replay.version, replay.build, stat.map_path, replay.seed, stat.settings, replay.slots.len());
    let data = war3::data::GameData::open(Path::new(&a[1]), false)?;
    let map_file = match a.get(3) {
        Some(m) => m.clone(),
        None => Path::new(&a[1]).join(stat.map_path.replace('\\', "/")).to_string_lossy().to_string(),
    };
    let map = std::sync::Arc::new(w3_mpq::Archive::open(&map_file)?);
    data.chain.mount(map, None, war3::data::PRIORITY_MAP);
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
    let mut g = w3_game::Game::load_with_keys(data.chain.clone(), data.version as i32, None)?;
    g.config()?;
    g.world.seed = replay.seed;
    // The settings word holds the advanced options eight bits up.
    let (flags, _speed) = war3::net::from_settings(stat.settings);
    g.start_lobby(&lobby, flags)?;
    g.world.keepalive_modules = w3_game::keepalive::BOOTY_BAY_MODULES.into_iter().collect();
    // Each checksum record follows the turn whose state it sums; the port's keepalive for
    // the next turn is made from that state.
    let (mut turn, mut checked, mut first_off) = (0u32, 0u32, None);
    let mut last = None;
    for rec in &replay.records {
        match rec {
            w3_formats::w3g::Record::Turn { ms, actions } => {
                turn += 1;
                if actions.is_empty() {
                    g.world.net_turn(*ms);
                } else {
                    g.world.net_turn_actions(*ms, actions);
                    let acts = actions.iter().map(|(pid, b)| (pid - 1, b.clone())).collect();
                    g.play_commands(&w3_game::replay::Frame { dt: *ms as f32 / 1000.0, commands: Vec::new(), actions: Some(acts), turn: None });
                }
                last = Some(g.world.keepalive());
                g.tick(*ms as f32 / 1000.0);
            }
            w3_formats::w3g::Record::Checksum(b) if b.len() == 4 => {
                let want = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
                checked += 1;
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
    Ok(())
}
