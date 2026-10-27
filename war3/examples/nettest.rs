//! Plays a two-player network game in one process, without drawing: a host and a
//! player connected over the loopback, each with its own copy of the game, both ordering
//! their units about, then compares the two games.
//!
//! nettest <game dir> <map> [seconds] [port]

use std::path::Path;

use war3::net::{Link, Role, Session};
use w3_net::client::{Browser, Client};
use w3_net::host::{HOST_PID, Host, HostMap, TFT, VERSION};

/// Two people, and NETTEST_AI computers after them.
fn lobby(local: u8) -> Vec<w3_game::LobbySlot> {
    let ai: u8 = std::env::var("NETTEST_AI").ok().and_then(|s| s.parse().ok()).unwrap_or(0);
    (0..2 + ai)
        .map(|p| w3_game::LobbySlot {
            player: p,
            fill: if p >= 2 {
                w3_game::SlotFill::Computer
            } else if p == local {
                w3_game::SlotFill::Local
            } else {
                w3_game::SlotFill::Remote
            },
            name: Some(format!("Player {}", p + 1)),
            race: Some(p as i32 % 4 + 1),
            team: p as i32,
            color: p as i32,
            handicap: 100.0,
            difficulty: 1,
        })
        .collect()
}

fn game(data: &war3::data::GameData, local: u8) -> anyhow::Result<w3_game::Game> {
    let mut g = w3_game::Game::load_with_keys(data.chain.clone(), data.version as i32, None)?;
    g.config()?;
    g.start_lobby(&lobby(local), war3::advanced::DEFAULT_FLAGS)?;
    g.outbox = Some(Vec::new());
    g.world.network = true;
    Ok(g)
}

/// A network game loaded from a saved one, as player `local`.
fn restored(data: &war3::data::GameData, bytes: &[u8], local: u8) -> anyhow::Result<w3_game::Game> {
    let mut g = w3_game::Game::load_with_keys(data.chain.clone(), data.version as i32, None)?;
    g.restore(w3_game::save::SaveGame::parse(bytes)?)?;
    g.game_event(w3_game::event_ids::EVENT_GAME_LOADED);
    g.outbox = Some(Vec::new());
    g.world.network = true;
    g.world.local_player = local;
    Ok(g)
}

/// The game's state, for comparing: each unit's place and life, and the players'
/// resources.
fn digest(g: &w3_game::Game) -> String {
    let w = &g.world;
    let mut s = String::new();
    for &h in &w.unit_order {
        if let Some(u) = w.unit(h) {
            s += &format!("{h} {} {:.3} {:.3} {:.3}\n", String::from_utf8_lossy(&u.type_id), u.x, u.y, u.life);
        }
    }
    for p in 0..4 {
        s += &format!("p{p} {:?}\n", w.players[p].state.iter().collect::<std::collections::BTreeMap<_, _>>());
    }
    s
}

fn main() -> anyhow::Result<()> {
    env_logger::init();
    let a: Vec<String> = std::env::args().collect();
    let data = war3::data::GameData::open(Path::new(&a[1]), false)?;
    let map = std::sync::Arc::new(w3_mpq::Archive::open(&a[2])?);
    data.chain.mount(map.clone(), None, war3::data::PRIORITY_MAP);
    let secs: f32 = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(30.0);
    let port: u16 = a.get(4).and_then(|s| s.parse().ok()).unwrap_or(6290);

    // The host offers the map over the protocol; the player finds it by searching,
    // joins, and says it has the map.
    let bytes = std::fs::read(&a[2])?;
    let (common, blizzard) = (data.chain.read("Scripts\\common.j")?, data.chain.read("Scripts\\blizzard.j")?);
    let checksums = w3_net::map::Checksums::compute(&bytes, &common, &blizzard, |n| map.read(n).ok());
    let mut setup = war3::net::Setup { game: "test".into(), map: a[2].clone(), speed: 2, flags: war3::advanced::DEFAULT_FLAGS, ..Default::default() };
    for p in 0..2u8 {
        let holder = if p == 0 { war3::net::Holder::Human(HOST_PID) } else { war3::net::Holder::Open };
        setup.slots.push(war3::net::NetSlot { player: p, holder, name: format!("Player {}", p + 1), race: 0, team: p as i32, color: p as i32, handicap: 0, locked: false });
    }
    let stat = w3_net::GameStat { settings: war3::net::settings(setup.flags, 2), map_width: 0, map_height: 0, map_xoro: checksums.xoro, map_path: a[2].clone(), host: "Player 1".into(), map_sha1: checksums.sha1 };
    let info = w3_net::GameInfo { product: TFT, version: VERSION, host_counter: 0, entry_key: 0, name: "test".into(), stat, slots_total: 2, game_type: 1, players: 1, player_slots: 2, uptime: 0, port };
    let mut host = Host::new(port, info, HostMap { path: a[2].clone(), bytes, checksums }, setup.table(0, None), "Player 1".into())?;
    let mut browser = Browser::new(port, TFT, VERSION)?;
    for _ in 0..300 {
        browser.poll();
        host.poll();
        if !browser.games.is_empty() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    println!("found {:?}", browser.games.iter().map(|g| (g.0, g.1.name.clone())).collect::<Vec<_>>());
    let (from, game_info, _) = browser.games.first().cloned().ok_or_else(|| anyhow::anyhow!("no game found"))?;
    let mut client = Client::join(from, &game_info, "Player 2", port)?;
    let mut ready = false;
    for _ in 0..300 {
        for e in client.poll() {
            if let w3_net::client::Event::MapCheck { .. } = e {
                client.map_status(true);
            }
        }
        ready |= host.poll().iter().any(|e| matches!(e, w3_net::host::Event::MapReady { .. }));
        if ready {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    anyhow::ensure!(ready, "the player was not seated");
    let pid = client.pid;
    host.start_countdown();
    host.end_countdown();
    let players = vec![(HOST_PID, 0), (pid, 1)];
    let mut hs = Session::new(Role { link: Link::Host(Box::new(host)), players: players.clone(), seed: 0 });
    let mut cs = Session::new(Role { link: Link::Client(Box::new(client)), players, seed: 0 });

    let mut gh = game(&data, 0)?;
    let mut gc = game(&data, 1)?;
    let dt = 1.0 / 60.0;
    let steps = (secs / dt) as usize;
    let units = |g: &w3_game::Game, p: u8| -> Vec<u32> { g.world.unit_order.iter().copied().filter(|&h| g.world.unit(h).is_some_and(|u| u.owner == p && !u.is_building)).collect() };
    let move_order = w3_game::natives::basic::order_id("move");
    // NETTEST_SAVE: halfway, the host's game is saved and both sides go on from the
    // saved game, as loading a network game does.
    let save_at = std::env::var("NETTEST_SAVE").is_ok().then_some(steps / 2);
    for step in 0..steps {
        // NETTEST_DESYNC: halfway, the player's game is nudged out of step.
        if std::env::var("NETTEST_DESYNC").is_ok() && step == steps / 2 {
            if let Some(u) = gc.world.unit_order.first().copied().and_then(|h| gc.world.unit_mut(h)) {
                u.x += 1.0;
            }
        }
        if Some(step) == save_at {
            let bytes = gh.save(&a[2])?;
            // NETTEST_SAVE_FILE: the saved game is also written there.
            if let Ok(f) = std::env::var("NETTEST_SAVE_FILE") {
                std::fs::write(f, &bytes)?;
            }
            gh = restored(&data, &bytes, 0)?;
            gc = restored(&data, &bytes, 1)?;
            println!("saved and loaded at frame {} ({} bytes)", hs.played, bytes.len());
        }
        // NETTEST_PAUSE: at a third the host pauses, and at a half the player resumes.
        if std::env::var("NETTEST_PAUSE").is_ok() {
            if step == steps / 3 {
                gh.apply(w3_game::replay::Command::Pause { player: 0, on: true });
                println!("pause at game time {:.2}", gh.world.time);
            }
            if step == steps / 2 {
                println!("resume at game time {:.2} (paused: {:?})", gc.world.time, gc.world.net_paused);
                gc.apply(w3_game::replay::Command::Pause { player: 1, on: false });
            }
        }
        // Every five seconds each side sends its units somewhere.
        if step % 300 == 60 {
            for (g, p) in [(&mut gh, 0u8), (&mut gc, 1u8)] {
                let (sx, sy) = g.world.start_of(p);
                let r = (step / 300) as f32;
                // As the interface sends them: the selection, then an order to it.
                let sel = units(g, p);
                g.apply(w3_game::replay::Command::Select { player: p, units: sel });
                g.apply(w3_game::replay::Command::SelectionOrder { player: p, order: move_order, at: Some((sx + 300.0 * r.cos(), sy + 300.0 * r.sin())), target: 0, queue: false, formation: false });
                g.apply(w3_game::replay::Command::Chat { player: p, text: format!("step {step}"), to: 0 });
            }
        }
        for (g, s, p) in [(&mut gh, &mut hs, 0u8), (&mut gc, &mut cs, 1u8)] {
            let out = g.outbox.as_mut().map(std::mem::take).unwrap_or_default();
            s.send_commands(&g.world, p, out);
            s.pump(dt);
            let frames = s.due(dt);
            let first = s.played - frames.len() as u32;
            for (k, f) in frames.iter().enumerate() {
                g.play_frame(f);
                s.check(first + k as u32 + 1, g.world.checksum());
            }
        }
        std::thread::sleep(std::time::Duration::from_micros(500));
    }
    // The player catches up on the frames still on their way.
    for _ in 0..200 {
        cs.pump(0.0);
        for f in cs.due(1.0) {
            gc.play_frame(&f);
        }
        if cs.played == hs.played {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    hs.pump(0.0);
    println!("host played {} frames, player {}; out of step: {:?}; game time {:.2}", hs.played, cs.played, hs.desynced, gh.world.time);
    let (dh, dc) = (digest(&gh), digest(&gc));
    if dh == dc {
        println!("SAME ({} units)", dh.lines().count() - 4);
        // The orders came through: each side's units left their start.
        for (g, p) in [(&gh, 0u8), (&gc, 1u8)] {
            let (sx, sy) = g.world.start_of(p);
            let away = units(g, p).iter().filter(|&&u| g.world.unit(u).is_some_and(|x| (x.x - sx).hypot(x.y - sy) > 250.0)).count();
            println!("player {p}: {away} of {} units moved off", units(g, p).len());
        }
    } else {
        println!("DIFFERENT");
        for (l, r) in dh.lines().zip(dc.lines()).filter(|(l, r)| l != r).take(10) {
            println!("  host {l}\n  peer {r}");
        }
    }
    Ok(())
}
