//! A live web dashboard for Mafarazzo TD, served over HTTP by the game's host while a
//! game of the map runs: the level being fought, the countdown to the next, the 40
//! levels' creeps, and warnings (with sound, in the page) before air and boss levels.
//!
//! The map is recognized by its script's globals rather than its name, so its edits
//! (the 2.x originals, the RELAXED versions with their larger counts) all read right:
//! the level table is the script's own `udg_RoundEnemy`/`udg_RoundAmount`, its creeps'
//! names, life, bounty, and movement from the map's object data.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use w3_game::Game;
use w3_game::types::Obj;
use w3_jass::Value;

pub const DEFAULT_PORT: u16 = 8000;

/// The page, served as is; it follows the game over `/ws`.
const PAGE: &str = include_str!("tddash.html");

/// The creeps' names in English: the map names them only in Chinese.  The same rawcodes
/// serve every level of the 2.x originals and the RELAXED edits; a creep not here keeps
/// the map's own name.
const ENGLISH: &[(&[u8; 4], &str)] = &[
    (b"n000", "Running Boy"), (b"h001", "Running Horse"), (b"h009", "Werewolf"), (b"h00A", "Forest Sentinel"),
    (b"h00B", "Thundercat"), (b"h00C", "Skeleton"), (b"h00D", "War Bull"), (b"u000", "Gargoyle"),
    (b"u001", "Hippogryph"), (b"h00E", "Roach"), (b"h00F", "Sea Elemental"), (b"h00G", "Treant"),
    (b"h00H", "High Elf"), (b"u002", "Dragonhawk Rider"), (b"h00I", "Giant Forest Spider"), (b"h00J", "Guard Captain"),
    (b"u003", "Phantom Vulture"), (b"h00K", "Dark Elf"), (b"u004", "Money Owl"), (b"h00L", "Giant Centaur"),
    (b"h00M", "Siege Engine"), (b"u005", "Chimaera"), (b"h00N", "Sasquatch"), (b"h00O", "Stone Ape"),
    (b"u006", "Frost Wyrm"), (b"h00P", "Ancient Giant"), (b"h00Q", "Inferno Kodo"), (b"u007", "Bronze Dragon"),
    (b"h00R", "Space Orc"), (b"h00W", "Ancient Infernal"), (b"h00T", "Water Elemental"), (b"o00B", "Spirit Wolf"),
    (b"h00V", "Phoenix"), (b"o00C", "Troll Berserker"), (b"u008", "Skeletal Mage"), (b"u009", "Abomination"),
    (b"u00A", "Destroyer"), (b"n007", "Lava Spawn"), (b"n008", "Spirit Hawk"), (b"h00S", "Ancient Infernal"),
];

const LEVELS: usize = 40;
/// The script's New Round makes each level's count at each of its six spawn regions.
const SPAWNS: u32 = 6;
/// The creeps' owner, Player(11).
const CREEP_PLAYER: u8 = 11;
/// How often the page's state is remade, in real seconds.
const PUBLISH_EVERY: f32 = 0.25;

/// Whether a game is of Mafarazzo TD: its script has the level table and round timer.
pub fn is_mafarazzo(g: &Game) -> bool {
    ["udg_RoundEnemy", "udg_RoundAmount", "udg_Level", "udg_NewRound", "udg_Lives", "udg_Fight"].iter().all(|n| g.vm.global(n).is_some())
}

/// What the game thread shares with the server's: the state as last published with its
/// count (each WebSocket sends a count it has not yet sent), and the flag that ends the
/// server.
struct Shared {
    state: Mutex<(u64, Arc<str>)>,
    stop: AtomicBool,
}

/// The running server.  Dropping it stops its threads and frees the port, for the next
/// game to bind.
pub struct Dashboard {
    shared: Arc<Shared>,
    pub url: String,
    since: f32,
    /// The map's tagged levels, read from its `-boss` and `-fantas` help text once.
    boss: Vec<u32>,
    fantas: Vec<u32>,
}

impl Drop for Dashboard {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
    }
}

impl Dashboard {
    pub fn start(g: &Game, port: u16) -> std::io::Result<Dashboard> {
        let listener = TcpListener::bind(("0.0.0.0", port))?;
        listener.set_nonblocking(true)?;
        let shared = Arc::new(Shared { state: Mutex::new((0, Arc::from("{}"))), stop: AtomicBool::new(false) });
        {
            let shared = shared.clone();
            std::thread::Builder::new().name("tddash".into()).spawn(move || serve(listener, shared))?;
        }
        let url = format!("http://{}:{port}/", lan_address().unwrap_or_else(|| "localhost".into()));
        let (boss, fantas) = tagged_levels(g);
        let mut d = Dashboard { shared, url, since: PUBLISH_EVERY, boss, fantas };
        d.update(g, 0.0);
        Ok(d)
    }

    /// Remakes the page's state every `PUBLISH_EVERY` seconds of real time and pushes it
    /// to the pages connected.
    pub fn update(&mut self, g: &Game, dt: f32) {
        self.since += dt;
        if self.since < PUBLISH_EVERY {
            return;
        }
        self.since = 0.0;
        if let Ok(json) = serde_json::to_string(&self.snapshot(g)) {
            let mut s = self.shared.state.lock().unwrap();
            *s = (s.0 + 1, Arc::from(json));
        }
    }

    fn snapshot(&self, g: &Game) -> Snapshot {
        let (w, vm) = (&g.world, &g.vm);
        let int = |n: &str| vm.global(n).map_or(0, |v| v.int());
        let level = int("udg_Level").max(0) as u32;
        let timer = vm.global("udg_NewRound").and_then(|v| match v {
            Value::Handle(h) => match w.handles.get(h) {
                Some(Obj::Timer(t)) => Some(t),
                _ => None,
            },
            _ => None,
        });
        let creeps = w.handles.iter().filter(|(_, o)| matches!(o, Obj::Unit(u) if u.owner == CREEP_PLAYER && !u.dead)).count() as u32;
        let players = (0..CREEP_PLAYER)
            .filter_map(|p| w.players.get(p as usize))
            // Those who left (or lost) keep their kills on the board.
            .filter(|p| p.controller == w3_game::types::map_control::USER && p.slot_state != w3_game::types::slot_state::EMPTY)
            .map(|p| Player {
                id: p.id,
                color: p.color,
                name: w.player_name(p.id),
                kills: vm.global_elem("udg_Ladderboard", p.id as usize + 1).map_or(0, |v| v.int()),
                left: p.slot_state == w3_game::types::slot_state::LEFT,
            })
            .collect();
        let levels = (1..=LEVELS as u32)
            .map(|n| {
                let enemy = vm.global_elem("udg_RoundEnemy", n as usize).map_or(0, |v| v.int());
                let id = (enemy as u32).to_be_bytes();
                let obj = w.objects.units.get(id);
                let (name, hp, gold, air) = obj.map_or((String::new(), 0.0, (0, 0), false), |o| {
                    let (plus, dice, sides) = (o.int("bountyplus"), o.int("bountydice"), o.int("bountysides"));
                    let name = ENGLISH.iter().find(|(r, _)| **r == id).map_or_else(|| w.localized(&o.str("Name")), |(_, n)| n.to_string());
                    (name, o.real("HP"), (plus + dice, plus + dice * sides), o.str("movetp").eq_ignore_ascii_case("fly"))
                });
                Level {
                    n,
                    name,
                    count: vm.global_elem("udg_RoundAmount", n as usize).map_or(0, |v| v.int()).max(0) as u32,
                    spawns: SPAWNS,
                    hp: hp.round() as u32,
                    gold,
                    air,
                    boss: self.boss.contains(&n),
                    fantas: self.fantas.contains(&n),
                }
            })
            .collect();
        Snapshot {
            map: w.localized(&g.info.name),
            time: w.time,
            level,
            lives: int("udg_Lives"),
            fight: vm.global("udg_Fight").is_some_and(|v| matches!(v, Value::Bool(true))),
            countdown: timer.filter(|t| t.running).map(|t| t.remaining_now()),
            creeps,
            players,
            levels,
        }
    }
}

#[derive(Serialize)]
struct Snapshot {
    map: String,
    /// Game seconds, which stop with the game, for the page to tell a paused game.
    time: f32,
    /// The level last begun (0 before the first).
    level: u32,
    lives: i32,
    /// A level's creeps are out; End Round clears it when the last dies.
    fight: bool,
    /// Game seconds until the next level, while its timer runs.
    countdown: Option<f32>,
    creeps: u32,
    players: Vec<Player>,
    levels: Vec<Level>,
}

#[derive(Serialize)]
struct Player {
    id: u8,
    color: i32,
    name: String,
    kills: i32,
    left: bool,
}

#[derive(Serialize)]
struct Level {
    n: u32,
    name: String,
    /// Per spawn region; the level has `count * spawns` creeps.
    count: u32,
    spawns: u32,
    hp: u32,
    /// A kill's bounty: its least and most.
    gold: (i32, i32),
    air: bool,
    boss: bool,
    fantas: bool,
}

/// The boss and fantas levels as the map's `-boss` and `-fantas` messages list them
/// ("Boss Levels Are: 11, 18, 30, 40", "You'll get fantas at levels : 2, 19, ..."):
/// the numbers on the line naming them, among the map's trigger strings, or the
/// original's own lists for an edit that dropped the message.  (Air levels come from the
/// creeps' own movement type instead.)
fn tagged_levels(g: &Game) -> (Vec<u32>, Vec<u32>) {
    let mut ids: Vec<_> = g.world.trigger_strings.keys().copied().collect();
    ids.sort_unstable();
    let find = |key: &str| -> Vec<u32> {
        let line = ids.iter().filter_map(|id| g.world.trigger_strings.get(id)).flat_map(|s| s.lines()).find(|l| {
            let l = l.to_ascii_lowercase();
            l.contains(key) && l.contains("levels") && l.contains(|c: char| c.is_ascii_digit())
        });
        let mut v: Vec<u32> = line.map_or(Vec::new(), |l| l.split(|c: char| !c.is_ascii_digit()).filter_map(|s| s.parse().ok()).filter(|&n| (1..=LEVELS as u32).contains(&n)).collect());
        v.dedup();
        v
    };
    let or = |v: Vec<u32>, d: &[u32]| if v.is_empty() { d.to_vec() } else { v };
    (or(find("boss"), &[11, 18, 30, 40]), or(find("fantas"), &[2, 19, 28, 33, 38]))
}

/// This computer's address on its LAN, as the route to an outside address picks it
/// (no packet is sent), for the link shown in the game.
fn lan_address() -> Option<String> {
    let s = UdpSocket::bind(("0.0.0.0", 0)).ok()?;
    s.connect(("192.0.2.1", 9)).ok()?;
    Some(s.local_addr().ok()?.ip().to_string())
}

fn serve(listener: TcpListener, shared: Arc<Shared>) {
    while !shared.stop.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((stream, _)) => {
                let shared = shared.clone();
                // A connection of its own thread: a WebSocket stays for as long as its
                // page is open, and a slow browser holds up no other.
                std::thread::spawn(move || {
                    if let Err(e) = respond(stream, &shared) {
                        log::debug!("tddash: {e}");
                    }
                });
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => std::thread::sleep(Duration::from_millis(50)),
            Err(e) => {
                log::warn!("tddash: {e}");
                std::thread::sleep(Duration::from_millis(250));
            }
        }
    }
}

fn respond(mut s: TcpStream, shared: &Shared) -> std::io::Result<()> {
    s.set_nonblocking(false)?;
    s.set_read_timeout(Some(Duration::from_secs(5)))?;
    // The request's head is peeked, not read, so a WebSocket upgrade's is still there
    // for tungstenite's handshake to read.
    let mut buf = [0u8; 8192];
    let mut seen = 0;
    let head = loop {
        let n = s.peek(&mut buf)?;
        if n == 0 {
            return Ok(());
        }
        if let Some(end) = buf[..n].windows(4).position(|w| w == b"\r\n\r\n") {
            break String::from_utf8_lossy(&buf[..end + 4]).into_owned();
        }
        if n == buf.len() {
            return Ok(());
        }
        if n == seen {
            std::thread::sleep(Duration::from_millis(10));
        }
        seen = n;
    };
    let mut words = head.split_whitespace();
    let (method, path) = (words.next().unwrap_or(""), words.next().unwrap_or(""));
    let path = path.split('?').next().unwrap_or("");
    if (method, path) == ("GET", "/ws") {
        return websocket(s, shared);
    }
    std::io::copy(&mut (&mut s).take(head.len() as u64), &mut std::io::sink())?;
    let (status, kind, body): (&str, &str, Arc<str>) = match (method, path) {
        ("GET", "/") | ("GET", "/index.html") => ("200 OK", "text/html; charset=utf-8", Arc::from(PAGE)),
        ("GET", "/state") => ("200 OK", "application/json", shared.state.lock().unwrap().1.clone()),
        ("GET", _) => ("404 Not Found", "text/plain", Arc::from("not found")),
        _ => ("405 Method Not Allowed", "text/plain", Arc::from("method not allowed")),
    };
    let head = format!("HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n", body.len());
    s.write_all(head.as_bytes())?;
    s.write_all(body.as_bytes())?;
    s.flush()
}

/// A WebSocket: each state published goes out as a text message as soon as it is made,
/// until the page closes or the dashboard stops.  Reads wait at most `WS_TICK`, so the
/// one thread both answers the page (its pings, its close) and pushes.
fn websocket(s: TcpStream, shared: &Shared) -> std::io::Result<()> {
    const WS_TICK: Duration = Duration::from_millis(50);
    let mut ws = tungstenite::accept(s).map_err(|e| std::io::Error::other(e.to_string()))?;
    ws.get_mut().set_read_timeout(Some(WS_TICK))?;
    ws.get_mut().set_nodelay(true)?;
    let mut sent = 0u64;
    let result = loop {
        if shared.stop.load(Ordering::Relaxed) {
            break ws.close(None);
        }
        match ws.read() {
            Ok(tungstenite::Message::Close(_)) => break Ok(()),
            Ok(_) => {}
            Err(tungstenite::Error::Io(e)) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => {}
            Err(e) => break Err(e),
        }
        let (n, msg) = shared.state.lock().unwrap().clone();
        if n != sent {
            sent = n;
            if let Err(e) = ws.send(tungstenite::Message::text(msg.to_string())) {
                break Err(e);
            }
        }
    };
    // A close the page began is answered by tungstenite as its read; this flushes it.
    let _ = ws.flush();
    match result {
        Ok(()) | Err(tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed) => Ok(()),
        Err(e) => Err(std::io::Error::other(e.to_string())),
    }
}
