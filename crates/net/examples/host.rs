//! Offers a two-player melee game on a map and plays it with whoever joins, without the
//! game: chat is echoed back, the game starts when a player says "go", and each turn
//! carries the players' actions as they sent them.  Prints what the players send, their keepalives'
//! checksums among it, and each turn's actions as "turn N carries pid hex".
//!
//! host <map file> <map path as the game names it> <common.j> <blizzard.j> <width>
//!     <height> [port]
//!
//! W3HOSTCOMPUTER="<race>,<level>" adds a computer player in a third slot: its race as
//! the slot table has it (1 human, 2 orc, 4 night elf, 8 undead, in hex) and its level
//! (0 easy, 1 normal, 2 insane).  W3HOSTCLOSED=n instead adds n closed slots (for a map
//! of more than two players: the lobby has as many slots as the map has players, the
//! original leaving one with fewer).  W3HOSTMAP=1 instead takes the lobby from the map's
//! own players, as the port's LAN hosting does (`war3::lan`): its users' slots open with
//! the host in the first, its computers' slots computers, each slot's team its player's
//! force with custom forces, its color its player's, its race the map's with fixed player
//! settings; the game's size the map's playable size (width and height then ignored).

use std::time::{Duration, Instant};

use w3_net::host::{Event, Host, HostMap, TFT, VERSION};
use w3_net::slot::{DOWNLOAD_UNKNOWN, layout, race, status};
use w3_net::{Chat, GameInfo, GameStat, Slot, SlotTable};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    let a: Vec<String> = std::env::args().collect();
    let bytes = std::fs::read(&a[1])?;
    let archive = w3_mpq::Archive::open(std::path::Path::new(&a[1]))?;
    let checksums = w3_net::map::Checksums::compute(&bytes, &std::fs::read(&a[3])?, &std::fs::read(&a[4])?, |n| archive.read(n).ok());
    let port: u16 = a.get(7).map_or(Ok(6112), |p| p.parse())?;
    // W3HOSTSETTINGS: the game's settings word in hex (0x64802, locked teams, by default).
    let settings = std::env::var("W3HOSTSETTINGS").ok().and_then(|v| u32::from_str_radix(v.trim_start_matches("0x"), 16).ok()).unwrap_or(0x0006_4802);
    let from_map = std::env::var("W3HOSTMAP").is_ok();
    let map_info = w3_formats::map::MapInfo::parse(&archive.read("war3map.w3i")?)?;
    let (map_width, map_height) = if from_map { (map_info.playable_width as u16, map_info.playable_height as u16) } else { (a[5].parse()?, a[6].parse()?) };
    let stat = GameStat { settings, map_width, map_height, map_xoro: checksums.xoro, map_path: a[2].clone(), host: "Host".into(), map_sha1: checksums.sha1 };
    let computer: Option<(u8, u8)> = std::env::var("W3HOSTCOMPUTER").ok().and_then(|v| {
        let (r, l) = v.split_once(',')?;
        Some((u8::from_str_radix(r, 16).ok()?, l.parse().ok()?))
    });
    let closed: u32 = std::env::var("W3HOSTCLOSED").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
    let slot = |pid, st, team, color| Slot { pid, download: if pid == 1 { 100 } else { DOWNLOAD_UNKNOWN }, status: st, computer: false, team, color, race: race::RANDOM | race::SELECTABLE, ai: 1, handicap: 100 };
    let mut list = vec![slot(1, status::OCCUPIED, 0, 0), slot(0, status::OPEN, 1, 1)];
    let mut layout = layout::MELEE;
    if from_map {
        let fixed = map_info.flags & 0x20 != 0;
        let forces = map_info.flags & 0x40 != 0;
        const RACES: [u8; 5] = [race::RANDOM, race::HUMAN, race::ORC, race::UNDEAD, race::NIGHT_ELF];
        let mut players: Vec<_> = map_info.players.iter().filter(|p| p.kind == 1 || p.kind == 2).collect();
        players.sort_by_key(|p| p.id);
        let mut host_placed = false;
        list = players
            .iter()
            .enumerate()
            .map(|(k, p)| {
                let team = if forces { map_info.forces.iter().position(|f| f.players & (1 << p.id) != 0).unwrap_or(0) as u8 } else { k as u8 };
                let race = if fixed { RACES[(p.race as usize).min(4)] } else { race::RANDOM | race::SELECTABLE };
                let host = p.kind == 1 && !host_placed;
                host_placed |= host;
                match (p.kind, host) {
                    (2, _) => Slot { download: 100, computer: true, race, ..slot(0, status::OCCUPIED, team, p.id as u8) },
                    (_, true) => Slot { race, ..slot(1, status::OCCUPIED, team, p.id as u8) },
                    _ => Slot { race, ..slot(0, status::OPEN, team, p.id as u8) },
                }
            })
            .collect();
        layout = match (forces, fixed) {
            (false, _) => layout::MELEE,
            (true, true) => layout::FIXED_PLAYER_SETTINGS,
            (true, false) => layout::CUSTOM_FORCES,
        };
    }
    let total = if from_map { list.len() as u32 } else { 2 + u32::from(computer.is_some()) + closed };
    let users = if from_map { map_info.players.iter().filter(|p| p.kind == 1).count() as u32 } else { total };
    // (A custom game's type, as the port's LAN hosting offers one.)
    let game_type = if from_map { 1 } else { 9 };
    let info = GameInfo { product: TFT, version: VERSION, host_counter: 1, entry_key: 0, name: "w3-net host".into(), stat, slots_total: total, game_type, players: 1, player_slots: users, uptime: 0, port };
    if let Some((r, level)) = computer.filter(|_| !from_map) {
        list.push(Slot { download: 100, computer: true, race: r | race::SELECTABLE, ai: level, ..slot(0, status::OCCUPIED, 2, 2) });
    }
    // (W3HOSTCLOSED: closed slots after the others, as many as the map has players more.)
    let first = list.len() as u8;
    for k in (0..closed as u8).filter(|_| !from_map) {
        list.push(slot(0, status::CLOSED, first + k, first + k));
    }
    // W3HOSTRACE: the host's slot's race flags in hex (random and selectable by default).
    if let Some(r) = std::env::var("W3HOSTRACE").ok().and_then(|v| u8::from_str_radix(v.trim_start_matches("0x"), 16).ok()) {
        list[0].race = r;
    }
    let slots = SlotTable { slots: list, seed: 0x35000000, layout, player_slots: total as u8 };
    let mut host = Host::new(port, info, HostMap { path: a[2].clone(), bytes, checksums }, slots, "Host".into())?;
    println!("offering on port {}", host.port());
    let mut countdown: Option<Instant> = None;
    let mut loading = false;
    let mut next_turn = Instant::now();
    let mut actions: Vec<(u8, Vec<u8>)> = Vec::new();
    let mut keepalives = std::collections::HashMap::new();
    // Turns sent, the first being 1 (as the keepalives count them).
    let mut sent = 0u32;
    loop {
        for e in host.poll() {
            if !matches!(e, Event::Keepalive { .. } | Event::Actions { .. }) {
                println!("{e:?}");
            }
            match e {
                Event::Keepalive { pid, checksum } => {
                    let n = keepalives.entry(pid).or_insert(0u32);
                    *n += 1;
                    println!("keepalive {pid} turn {n}: {checksum:08x}");
                }
                Event::Actions { pid, data } => {
                    println!("actions from {pid}: {data:02x?}");
                    actions.push((pid, data));
                }
                Event::Chat { from, chat: Chat::Lobby(text) } if text == "go" && countdown.is_none() => {
                    host.start_countdown();
                    countdown = Some(Instant::now());
                    let _ = from;
                }
                Event::Chat { from, chat: Chat::Lobby(text) } => host.chat(&[from], Chat::Lobby(format!("you said: {text}"))),
                Event::Left { .. } if host.players().len() == 1 => return Ok(()),
                _ => {}
            }
        }
        if countdown.is_some_and(|t| t.elapsed() >= Duration::from_secs(5)) && !loading {
            host.end_countdown();
            host.loaded();
            loading = true;
        }
        // The turns run from when every player has loaded, one each 100 ms of real time
        // (a backlog sent at once runs the game ahead of its clients' real-time clocks,
        // which trigger sleeps keep).
        if loading && host.all_loaded() && sent == 0 && Instant::now() > next_turn {
            next_turn = Instant::now();
        }
        if loading && host.all_loaded() && Instant::now() >= next_turn {
            sent += 1;
            for (pid, data) in &actions {
                println!("turn {sent} carries {pid} {}", data.iter().map(|b| format!("{b:02x}")).collect::<String>());
            }
            host.send_turn(100, std::mem::take(&mut actions));
            next_turn += Duration::from_millis(100);
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}
