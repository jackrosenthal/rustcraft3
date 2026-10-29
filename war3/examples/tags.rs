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
    data.chain.mount(map, None, war3::data::PRIORITY_MAP);
    let seed = u32::from_str_radix(&a[3], 16)?;
    let reference: Vec<String> = std::fs::read_to_string(&a[4])?.lines().filter(|l| !l.starts_with('#')).map(|l| l.to_string()).collect();
    let lobby: Vec<w3_game::LobbySlot> = (0..2u8).map(|p| w3_game::LobbySlot { player: p, fill: if p == 0 { w3_game::SlotFill::Remote } else { w3_game::SlotFill::Local }, name: Some(format!("P{p}")), race: None, team: p as i32, color: p as i32, handicap: 100.0, difficulty: 1 }).collect();
    w3_game::tags::RECORD.store(true, std::sync::atomic::Ordering::Relaxed);
    let mut g = w3_game::Game::load_with_keys(data.chain.clone(), data.version as i32, None)?;
    g.config()?;
    g.world.seed = seed;
    g.start_lobby(&lobby, war3::advanced::DEFAULT_FLAGS)?;
    // TAGS_TURNS: how many turns of 0.1 seconds to play (one by default).
    let turns: usize = std::env::var("TAGS_TURNS").ok().and_then(|v| v.parse().ok()).unwrap_or(1);
    // TAGS_FIRST: the first turn's length (the original's first turn runs what falls
    // due at once without its clock moving, it seems).
    let first: f32 = std::env::var("TAGS_FIRST").ok().and_then(|v| v.parse().ok()).unwrap_or(0.1);
    // TAGS_DRAWS: print each of the main generator's draws as it is made.
    if std::env::var("TAGS_DRAWS").is_ok() {
        w3_game::rng::TRACE.store(true, std::sync::atomic::Ordering::Relaxed);
    }
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
    g.world.keepalive_modules = w3_game::keepalive::BOOTY_BAY_MODULES.into_iter().collect();
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
            g.play_commands(&w3_game::replay::Frame { dt, commands: Vec::new(), actions: Some(actions) });
            if std::env::var("TAGS_MODULE").is_ok() {
                println!("turn {n}: game module {:08x}", g.world.game_module());
            }
        }
        if let (Some(&want), None) = (keepalives.get(&n), first_off) {
            let ours = g.world.keepalive();
            if ours != want {
                first_off = Some((n, ours, want));
            }
        }
        g.tick(dt);
        // TAGS_STEPS: each turn's movement steps' clocks.
        if std::env::var("TAGS_STEPS").is_ok() {
            println!("turn {n} steps {:?}", g.world.step_clocks.iter().map(|&c| f32::from_bits(c)).collect::<Vec<_>>());
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
