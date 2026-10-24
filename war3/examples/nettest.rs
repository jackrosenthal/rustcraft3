//! Plays a two-player network game in one process, without drawing: a host and a
//! player connected over the loopback, each with its own copy of the game, both ordering
//! their units about, then compares the two games.
//!
//! nettest <game dir> <map> [seconds] [port]

use std::path::Path;

use war3::net::{Conn, GameAd, Host, Role, Session};

fn lobby(local: u8) -> Vec<w3_game::LobbySlot> {
    (0..2u8)
        .map(|p| w3_game::LobbySlot {
            player: p,
            fill: if p == local { w3_game::SlotFill::Local } else { w3_game::SlotFill::Remote },
            name: Some(format!("Player {}", p + 1)),
            race: Some(if p == 0 { 1 } else { 2 }),
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
    g.start_lobby(&lobby(local))?;
    g.outbox = Some(Vec::new());
    g.world.network = true;
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
    for p in 0..2 {
        s += &format!("p{p} {:?}\n", w.players[p].state.iter().collect::<std::collections::BTreeMap<_, _>>());
    }
    s
}

fn main() -> anyhow::Result<()> {
    env_logger::init();
    let a: Vec<String> = std::env::args().collect();
    let data = war3::data::GameData::open(Path::new(&a[1]), false)?;
    let map = std::sync::Arc::new(w3_mpq::Archive::open(&a[2])?);
    data.chain.mount(map, None, war3::data::PRIORITY_MAP);
    let secs: f32 = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(30.0);
    let port: u16 = a.get(4).and_then(|s| s.parse().ok()).unwrap_or(6290);

    let ad = GameAd { name: "test".into(), host: "Player 1".into(), map: a[2].clone(), players: 1, slots: 2, port, expansion: false, speed: 2 };
    let mut host = Host::new(ad)?;
    // The game is found by searching.
    let mut browser = war3::net::Browser::new(port)?;
    for _ in 0..300 {
        browser.poll();
        host.poll();
        if !browser.games.is_empty() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    println!("found {:?}", browser.games.iter().map(|g| (g.0, g.1.name.clone())).collect::<Vec<_>>());
    let client = Conn::connect(([127, 0, 0, 1], port).into())?;
    // The player's connection arrives.
    for _ in 0..100 {
        host.poll();
        if !host.peers.is_empty() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let id = host.peers.first().map(|p| p.0).ok_or_else(|| anyhow::anyhow!("no connection"))?;
    host.peers[0].2 = Some("Player 2".into());
    let mut hs = Session::new(Role::Host { host, pending: Vec::new(), peers: vec![(id, 1, 0)] });
    let mut cs = Session::new(Role::Client { conn: client });

    let mut gh = game(&data, 0)?;
    let mut gc = game(&data, 1)?;
    let dt = 1.0 / 60.0;
    let steps = (secs / dt) as usize;
    let units = |g: &w3_game::Game, p: u8| -> Vec<u32> { g.world.unit_order.iter().copied().filter(|&h| g.world.unit(h).is_some_and(|u| u.owner == p && !u.is_building)).collect() };
    let move_order = w3_game::natives::basic::order_id("move");
    for step in 0..steps {
        // Every five seconds each side sends its units somewhere.
        if step % 300 == 60 {
            for (g, p) in [(&mut gh, 0u8), (&mut gc, 1u8)] {
                let (sx, sy) = g.world.start_of(p);
                let r = (step / 300) as f32;
                for u in units(g, p) {
                    g.apply(w3_game::replay::Command::Order { unit: u, order: move_order, x: sx + 300.0 * r.cos(), y: sy + 300.0 * r.sin(), target: 0 });
                }
                g.apply(w3_game::replay::Command::Chat { player: p, text: format!("step {step}"), to: 0 });
            }
        }
        for (g, s, p) in [(&mut gh, &mut hs, 0u8), (&mut gc, &mut cs, 1u8)] {
            let out = g.outbox.as_mut().map(std::mem::take).unwrap_or_default();
            s.send_commands(p, out);
            s.pump(dt);
            for f in s.due(dt) {
                g.play_frame(&f);
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
    println!("host played {} frames, player {}", hs.played, cs.played);
    let (dh, dc) = (digest(&gh), digest(&gc));
    if dh == dc {
        println!("SAME ({} units)", dh.lines().count() - 2);
    } else {
        println!("DIFFERENT");
        for (l, r) in dh.lines().zip(dc.lines()).filter(|(l, r)| l != r).take(10) {
            println!("  host {l}\n  peer {r}");
        }
    }
    Ok(())
}
