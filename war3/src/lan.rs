//! The Local Area Network screens: CLocalMultiplayerJoin (the games found, the player's
//! name, Create and Join), CLocalMultiplayerCreate (a map to host, which the Custom Game
//! screen's map list chooses), and CGameChatroom (the game setup everyone shares, with
//! chat, which the host starts).  They speak the original's protocol (`w3_net`), so the
//! original's games are listed and joined, and it can join theirs.

use std::path::{Path, PathBuf};

use w3_net::client::{self, Browser, Client};
use w3_net::host::{self, HOST_PID, Host, HostMap};
use w3_net::packet::{TFT, ROC, VERSION, leave};
use w3_net::{Chat, GameInfo, GameStat, SlotTable};
use w3_ui::{Draw, FrameDb, FramePoint as P, FrameTree, Justify, Rect};

use crate::net::{self, Holder, NetSlot, Setup};

const ROW: f32 = 0.0195;
/// CLocalMultiplayerJoin's name box takes 15 characters (FUN_6f104350).
const NAME_LEN: usize = 15;
/// A GAMEINFO's game type: a custom map, one of Blizzard's (8), a saved game (0x200).
const GAME_CUSTOM: u32 = 1;
const GAME_BLIZZARD: u32 = 8;
const GAME_SAVED: u32 = 0x200;

/// What the screens ask of the glue.
pub enum Event {
    /// The lobby is open.
    Lobby,
    /// Back to the game list, with a message to show (a NETERROR key), if any.
    Joining(Option<&'static str>),
    /// The game begins: its map, the setup for this computer, how it plays, and its
    /// speed.
    Start(PathBuf, Vec<w3_game::LobbySlot>, net::Role, u8, u32),
    /// A loaded game begins: its map, its saved state, how it plays, its speed, and
    /// this computer's player.
    StartSaved(PathBuf, Vec<u8>, net::Role, u8, u8),
}

pub struct Lan {
    pub name: String,
    port: u16,
    game_dir: PathBuf,
    browser: Option<Browser>,
    pub selected: Option<usize>,
    list: Option<Rect>,
    /// The lobby: as host, or joined to one; this computer's player id in it.
    host: Option<Box<Host>>,
    client: Option<Box<Client>>,
    me: u8,
    pub setup: Setup,
    /// A joined player's game: as offered, and its slots until this computer has the
    /// map they are of.
    joined: Option<GameInfo>,
    table: Option<SlotTable>,
    pub chat: Vec<String>,
    pub line: String,
    /// The lobby's frames need rebuilding (the setup changed).
    pub rebuild: bool,
    /// The game list's selection changed, and its map's details are due.
    pub summary_due: Option<PathBuf>,
    /// A joined player's copy of the host's map, when this computer has it.
    local_map: Option<PathBuf>,
    /// The saved game being loaded (the host's file).
    save: Option<Vec<u8>>,
    /// The countdown to the start: seconds since it began.
    countdown: Option<f32>,
    /// Sound labels for the glue to play.
    pub sounds: Vec<String>,
}

impl Lan {
    pub fn new(game_dir: &Path, name: String) -> Lan {
        Lan {
            name,
            port: net::DEFAULT_PORT,
            game_dir: game_dir.to_path_buf(),
            browser: None,
            selected: None,
            list: None,
            host: None,
            client: None,
            me: 0,
            setup: Setup::default(),
            joined: None,
            table: None,
            chat: Vec::new(),
            line: String::new(),
            rebuild: false,
            summary_due: None,
            local_map: None,
            save: None,
            countdown: None,
            sounds: Vec::new(),
        }
    }

    /// A map's path as the protocol names it: within the game folder, relative to it,
    /// with backslashes (`Maps\(2)BootyBay.w3m`).
    fn map_name(&self, map: &Path) -> String {
        map.strip_prefix(&self.game_dir).unwrap_or(map).to_string_lossy().replace('/', "\\")
    }

    fn map_path(&self, name: &str) -> PathBuf {
        let p = PathBuf::from(name.replace('\\', "/"));
        if p.is_absolute() { p } else { self.game_dir.join(p) }
    }

    /// The game list screen: searching starts.
    pub fn attach_join(&mut self, tree: &mut FrameTree, db: &FrameDb, port: u16) {
        self.port = port;
        if self.browser.is_none() {
            self.browser = Browser::new(port, TFT, VERSION).map_err(|e| log::warn!("lan: {e}")).ok();
        }
        self.selected = None;
        self.sync_join(tree, db);
    }

    pub fn detach(&mut self) {
        self.browser = None;
    }

    fn games(&self) -> &[(std::net::SocketAddr, GameInfo, std::time::Instant)] {
        self.browser.as_ref().map_or(&[], |b| &b.games)
    }

    fn sync_join(&self, tree: &mut FrameTree, db: &FrameDb) {
        let g = self.selected.and_then(|k| self.games().get(k)).map(|g| &g.1);
        let (flags, speed) = g.map_or((crate::advanced::DEFAULT_FLAGS, 0), |g| net::from_settings(g.stat.settings));
        tree.set_text("GameCreatorValue", g.map_or("", |g| g.stat.host.as_str()));
        tree.set_text("GameSpeedValue", &g.map_or(String::new(), |_| db.string(speed_key(speed))));
        crate::advanced::display(tree, db, flags);
        // The buttons stay enabled: without a name or a game they say what is needed.
    }

    /// Searches go on, the lobby's messages come in.
    pub fn update(&mut self, tree: &mut FrameTree, db: &FrameDb) -> Option<Event> {
        if let Some(b) = self.browser.as_mut() {
            if b.poll() {
                let n = b.games.len();
                self.selected = self.selected.filter(|&k| k < n);
                self.sync_join(tree, db);
            }
        }
        if self.host.is_some() {
            return self.host_update(db);
        }
        if self.client.is_some() {
            return self.client_update(db);
        }
        None
    }

    /// Typing goes to the name on the game list, to the chat line in the lobby.
    pub fn key(&mut self, c: char, tree: &mut FrameTree, db: &FrameDb, lobby: bool) -> bool {
        let (text, max) = if lobby { (&mut self.line, 255) } else { (&mut self.name, NAME_LEN) };
        match c {
            '\u{8}' => {
                text.pop();
            }
            '\r' | '\n' if lobby => {
                let line = std::mem::take(&mut self.line);
                if !line.trim().is_empty() {
                    self.say(&line);
                }
            }
            c if !c.is_control() && text.chars().count() < max => text.push(c),
            _ => return false,
        }
        if !lobby {
            self.sync_join(tree, db);
        }
        true
    }

    /// A click in the game list chooses a game.
    pub fn click(&mut self, tree: &mut FrameTree, db: &FrameDb, x: f32, y: f32) {
        let Some(r) = self.list else { return };
        if !r.contains(x, y) {
            return;
        }
        let k = ((r.y1 - y) / ROW) as usize;
        if k < self.games().len() {
            self.selected = Some(k);
            self.summary_due = Some(self.map_path(&self.games()[k].1.stat.map_path));
            self.sync_join(tree, db);
        }
    }

    /// The map's slots for the setup, in its player order: its users' open, its
    /// computers' computers.  With custom forces each slot's team is its player's force;
    /// with fixed player settings the map's races hold and its computers are locked.
    /// With full observers or referees, the players the map leaves unused are the
    /// Observers team (team 12, as CPlayerSlot's team list ends).
    fn map_setup(summary: &crate::skirmish::MapSummary, flags: u32) -> Setup {
        let info = &summary.info;
        let fixed = info.flags & 0x20 != 0;
        let forces: Vec<String> = if info.flags & 0x40 != 0 { info.forces.iter().map(|f| summary.text(&f.name)).collect() } else { Vec::new() };
        let force = |id: u32| info.forces.iter().position(|f| f.players & (1 << id) != 0).unwrap_or(0) as i32;
        let mut players: Vec<_> = info.players.iter().filter(|p| p.kind == 1 || p.kind == 2).collect();
        players.sort_by_key(|p| p.id);
        let mut slots: Vec<NetSlot> = players
            .iter()
            .enumerate()
            .map(|(k, p)| NetSlot {
                player: p.id as u8,
                holder: if p.kind == 2 { Holder::Computer(1) } else { Holder::Open },
                name: String::new(),
                race: if fixed { p.race.min(4) as usize } else { 0 },
                team: if forces.is_empty() { k as i32 } else { force(p.id) },
                color: p.id as i32,
                handicap: 0,
                locked: fixed && p.kind == 2,
            })
            .collect();
        if flags & 0x10 != 0 || flags & 0x400000 != 0 {
            let used: Vec<u8> = slots.iter().map(|s| s.player).collect();
            for id in (0..12u8).filter(|id| !used.contains(id)) {
                slots.push(NetSlot { player: id, holder: Holder::Open, name: String::new(), race: 0, team: OBSERVERS, color: id as i32, handicap: 0, locked: false });
            }
        }
        Setup { slots, flags, fixed, forces, ..Default::default() }
    }

    /// Hosts a game on a map, its user slots open for others, the host in the first.
    /// `chain` gives the game's scripts, which the map's checksums cover.
    pub fn host_game(&mut self, map: &Path, summary: &crate::skirmish::MapSummary, game: &str, expansion: bool, speed: u8, flags: u32, chain: &w3_mpq::Chain) -> Result<(), &'static str> {
        let mut setup = Lan::map_setup(summary, flags);
        let first = setup.slots.iter_mut().find(|s| s.holder == Holder::Open && s.team != OBSERVERS).ok_or("NETERROR_CREATEGAMEFAILED")?;
        first.holder = Holder::Human(HOST_PID);
        first.name = self.name.clone();
        setup.game = game.to_string();
        setup.map = self.map_name(map);
        setup.speed = speed;
        let users = summary.info.players.iter().filter(|p| p.kind == 1).count() as u32;
        let game_type = GAME_CUSTOM | if summary.signed { GAME_BLIZZARD } else { 0 };
        self.host_setup(map, setup, summary, expansion, game_type, users, None, chain)
    }

    /// Offers the setup's game.  A loaded game is offered as 1.27b offers one: by its
    /// save's path and an identifier of the save in place of the map's, its size and
    /// SHA-1 zero (the map itself is still checked as a joining player's MAPCHECK).
    #[allow(clippy::too_many_arguments)]
    fn host_setup(&mut self, map: &Path, setup: Setup, summary: &crate::skirmish::MapSummary, expansion: bool, game_type: u32, users: u32, save: Option<(String, u32)>, chain: &w3_mpq::Chain) -> Result<(), &'static str> {
        let failed = |e: &dyn std::fmt::Display| {
            log::warn!("lan: hosting: {e}");
            "NETERROR_CREATEGAMEFAILED"
        };
        let bytes = std::fs::read(map).map_err(|e| failed(&e))?;
        let archive = w3_mpq::Archive::open(map).map_err(|e| failed(&e))?;
        let (common, blizzard) = (chain.read("Scripts\\common.j").unwrap_or_default(), chain.read("Scripts\\blizzard.j").unwrap_or_default());
        let checksums = w3_net::map::Checksums::compute(&bytes, &common, &blizzard, |n| archive.read(n).ok());
        let stat = match save {
            Some((path, id)) => GameStat { settings: net::settings(setup.flags, setup.speed), map_width: 0, map_height: 0, map_xoro: id, map_path: path, host: self.name.clone(), map_sha1: [0; 20] },
            None => GameStat {
                settings: net::settings(setup.flags, setup.speed),
                map_width: summary.info.playable_width as u16,
                map_height: summary.info.playable_height as u16,
                map_xoro: checksums.xoro,
                map_path: setup.map.clone(),
                host: self.name.clone(),
                map_sha1: checksums.sha1,
            },
        };
        let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.subsec_nanos() ^ d.as_secs() as u32);
        let table = setup.table(seed, None);
        let info = GameInfo {
            product: if expansion { TFT } else { ROC },
            version: VERSION,
            host_counter: 0,
            entry_key: 0,
            name: setup.game.clone(),
            stat,
            slots_total: table.slots.len() as u32,
            game_type,
            players: 1,
            player_slots: users,
            uptime: 0,
            port: self.port,
        };
        // The game list's searches stop; the host answers others'.
        self.browser = None;
        let h = Host::new(self.port, info, HostMap { path: setup.map.clone(), bytes, checksums }, table, self.name.clone()).map_err(|e| failed(&e))?;
        self.setup = setup;
        self.host = Some(Box::new(h));
        self.me = HOST_PID;
        self.chat.clear();
        self.rebuild = true;
        Ok(())
    }

    /// A saved network game's setup: its people's slots kept for them by name, its
    /// computers as they were.
    fn saved_setup(save: &w3_game::save::SaveGame) -> Setup {
        let slots = save
            .players()
            .iter()
            .map(|p| NetSlot {
                player: p.id,
                holder: if p.computer { Holder::Computer(1) } else { Holder::Open },
                name: if p.computer { String::new() } else { p.name.clone() },
                race: p.race.clamp(0, 4) as usize,
                team: if p.observer { OBSERVERS } else { p.team },
                color: p.color,
                handicap: 0,
                locked: false,
            })
            .collect();
        Setup { slots, speed: 2, flags: crate::advanced::DEFAULT_FLAGS, saved: true, ..Default::default() }
    }

    /// A save's identifier, as GAMEINFO carries it in place of the map's checksum.
    fn save_id(bytes: &[u8]) -> u32 {
        crc32fast::hash(bytes)
    }

    /// Hosts a saved network game (CLocalMultiplayerLoad): its map and speed, its
    /// people's slots kept for them by name, its computers as they were.  Everyone
    /// loads their own copy of the save (each computer saved it).
    pub fn host_saved(&mut self, file: &Path, game: &str, expansion: bool, chain: &w3_mpq::Chain) -> Result<(), &'static str> {
        let bytes = std::fs::read(file).map_err(|_| "SAVE_ERROR_BAD_SAVE")?;
        let save = w3_game::save::SaveGame::parse(&bytes).map_err(|_| "SAVE_ERROR_BAD_SAVE")?;
        let mut setup = Lan::saved_setup(&save);
        let mine = setup.slots.iter_mut().find(|s| s.holder == Holder::Open && s.name.eq_ignore_ascii_case(&self.name)).ok_or("NETERROR_JOINGAMEFAILED")?;
        mine.holder = Holder::Human(HOST_PID);
        // The save names its map as the game loaded it: a file, or one in the game's
        // folder.
        let map = Some(PathBuf::from(&save.map)).filter(|p| p.is_file()).unwrap_or_else(|| self.map_path(&save.map));
        let summary = crate::skirmish::summarize_path(&map).ok_or("NETERROR_MAPFILENOTFOUND")?;
        setup.game = game.to_string();
        setup.map = self.map_name(&map);
        let users = save.players().iter().filter(|p| p.human).count() as u32;
        let path = format!("Save\\Multiplayer\\{}", file.file_name().unwrap_or_default().to_string_lossy());
        self.host_setup(&map, setup, &summary, expansion, GAME_SAVED, users, Some((path, Lan::save_id(&bytes))), chain)?;
        self.save = Some(bytes);
        Ok(())
    }

    /// Joins the chosen game.
    pub fn join(&mut self) -> Result<(), &'static str> {
        self.local_map = None;
        self.table = None;
        let Some((addr, game, _)) = self.selected.and_then(|k| self.games().get(k)).cloned() else { return Err("NETERROR_NOGAMESPECIFIED") };
        // A loaded game is joined with this computer's own copy of its save.
        let saved = game.game_type & GAME_SAVED != 0;
        self.save = None;
        if saved {
            let file = Path::new(&game.stat.map_path.replace('\\', "/")).file_name().map(|f| f.to_owned()).unwrap_or_default();
            let bytes = crate::data::multiplayer_save_dir().and_then(|d| std::fs::read(d.join(file)).ok());
            match bytes {
                Some(b) if Lan::save_id(&b) == game.stat.map_xoro => self.save = Some(b),
                _ => return Err("NETERROR_DIFFERENTSAVEFILE"),
            }
        }
        let c = Client::join(addr, &game, &self.name, self.port).map_err(|_| "NETERROR_JOINGAMEFAILED")?;
        let (flags, speed) = net::from_settings(game.stat.settings);
        self.setup = Setup { game: game.name.clone(), speed, flags, saved, ..Default::default() };
        self.joined = Some(game);
        self.client = Some(Box::new(c));
        self.chat.clear();
        Ok(())
    }

    /// Leaves the lobby (a host's closes it).
    pub fn leave(&mut self) {
        if let Some(mut h) = self.host.take() {
            h.close();
        }
        if let Some(mut c) = self.client.take() {
            c.leave(leave::LOBBY);
        }
        self.countdown = None;
        self.setup = Setup::default();
    }

    /// Everyone in the lobby but this computer.
    fn others(&self) -> Vec<u8> {
        self.people().into_iter().map(|p| p.0).filter(|&pid| pid != self.me).collect()
    }

    /// The lobby's people: their ids and names.
    fn people(&self) -> Vec<(u8, String)> {
        match (&self.host, &self.client) {
            (Some(h), _) => h.players(),
            (_, Some(c)) => c.players.iter().cloned().chain(std::iter::once((self.me, self.name.clone()))).collect(),
            _ => Vec::new(),
        }
    }

    fn say(&mut self, text: &str) {
        let line = format!("{}: {text}", self.name);
        let to = self.others();
        if let Some(h) = self.host.as_mut() {
            h.chat(&to, Chat::Lobby(text.to_string()));
            self.chat.push(line);
        } else if let Some(c) = self.client.as_mut() {
            c.chat(Chat::Lobby(text.to_string()));
            self.chat.push(line);
        }
    }

    /// A lobby notice (NETMESSAGE_PLAYERJOINED, _PLAYERLEFT): each computer says it
    /// when it hears of the player.
    fn notice(&mut self, db: &FrameDb, key: &str, name: &str) {
        self.chat.push(db.string(key).replace("%s", name));
    }

    /// The host's setup goes to everyone as the slot table.
    fn send_setup(&mut self) {
        if let Some(h) = self.host.as_mut() {
            h.slots = self.setup.table(h.slots.seed, Some(&h.slots));
            h.send_slots();
        }
    }

    fn host_update(&mut self, db: &FrameDb) -> Option<Event> {
        let h = self.host.as_mut()?;
        let events = h.poll();
        let mut changed = false;
        for e in events {
            match e {
                host::Event::Joined { pid, name } => {
                    // A loaded game seats only those who were in it, each in their slot.
                    if self.setup.saved {
                        self.seat_saved(pid, &name);
                    }
                    self.notice(db, "NETMESSAGE_PLAYERJOINED", &name);
                    changed = true;
                }
                host::Event::Left { pid, .. } => {
                    let name = self.setup.slot_of(pid).map(|s| s.name.clone()).unwrap_or_default();
                    self.notice(db, "NETMESSAGE_PLAYERLEFT", &name);
                    changed = true;
                }
                host::Event::Chat { from, chat: Chat::Lobby(text) } => {
                    let name = self.people().into_iter().find(|p| p.0 == from).map(|p| p.1).unwrap_or_default();
                    self.chat.push(format!("{name}: {text}"));
                }
                host::Event::Choice { pid, chat } => {
                    self.choice(pid, chat);
                    self.send_setup();
                    self.rebuild = true;
                }
                _ => {}
            }
        }
        if changed {
            let h = self.host.as_ref()?;
            let (table, people) = (h.slots.clone(), h.players());
            self.setup.take_table(&table, &people);
            self.rebuild = true;
        }
        None
    }

    /// A loaded game's joining player takes their own slot (by name), or is turned away.
    fn seat_saved(&mut self, pid: u8, name: &str) {
        let Some(h) = self.host.as_mut() else { return };
        let mine = self.setup.slots.iter().position(|s| s.holder == Holder::Open && s.name.eq_ignore_ascii_case(name));
        let seated = h.slots.slots.iter().position(|s| s.pid == pid);
        match (mine, seated) {
            // Who holds the slots moves; the slots' choices stay.
            (Some(k), Some(at)) if k != at => {
                let (to, from) = (h.slots.slots[k], h.slots.slots[at]);
                let s = &mut h.slots.slots[k];
                (s.pid, s.status, s.download) = (from.pid, from.status, from.download);
                let s = &mut h.slots.slots[at];
                (s.pid, s.status, s.download) = (to.pid, to.status, to.download);
                h.send_slots();
            }
            (Some(_), Some(_)) => {}
            _ => h.drop_player(pid, leave::LOBBY),
        }
    }

    /// A joined player's choice for their own slot, as the host applies it.
    fn choice(&mut self, pid: u8, chat: Chat) {
        let fixed = self.setup.fixed;
        let Some(s) = self.setup.slots.iter_mut().find(|s| s.holder == Holder::Human(pid)) else { return };
        match chat {
            Chat::Team(t) => choose_team(&mut self.setup, Holder::Human(pid), t as i32),
            Chat::Color(c) if !fixed => s.color = (c as i32).min(11),
            Chat::Race(bits) if !fixed => s.race = [0x20u8, 0x01, 0x02, 0x08, 0x04].iter().position(|&r| bits & r != 0).unwrap_or(0),
            Chat::Handicap(h) => s.handicap = (100u8.saturating_sub(h) / 10).min(5) as usize,
            _ => {}
        }
    }

    fn client_update(&mut self, db: &FrameDb) -> Option<Event> {
        let c = self.client.as_mut()?;
        let events = c.poll();
        let mut event = None;
        for e in events {
            match e {
                client::Event::Joined { pid } => {
                    self.me = pid;
                    event = Some(Event::Lobby);
                }
                client::Event::Rejected { .. } => {
                    self.client = None;
                    return Some(Event::Joining(Some("NETERROR_JOINGAMEFAILED")));
                }
                client::Event::Slots => {
                    self.table = self.client.as_ref().map(|c| c.slots.clone());
                    self.take_table();
                }
                client::Event::PlayerJoined { name, .. } => self.notice(db, "NETMESSAGE_PLAYERJOINED", &name),
                client::Event::PlayerLeft { pid, .. } => {
                    let name = self.setup.slot_of(pid).map(|s| s.name.clone()).unwrap_or_default();
                    self.notice(db, "NETMESSAGE_PLAYERLEFT", &name);
                }
                client::Event::Chat { from, chat: Chat::Lobby(text) } => {
                    let name = self.people().into_iter().find(|p| p.0 == from).map(|p| p.1).unwrap_or_default();
                    self.chat.push(format!("{name}: {text}"));
                }
                client::Event::MapCheck { path, size, crc32, .. } => {
                    self.setup.map = path.clone();
                    let have = self.find_map(&path, size, crc32);
                    if let Some(c) = self.client.as_mut() {
                        c.map_status(have.is_some());
                    }
                    if let Some(p) = have {
                        self.have_map(p);
                    }
                }
                client::Event::MapDownloaded(bytes) => {
                    // The host's map is kept in Maps\Download.
                    let dir = self.game_dir.join("Maps").join("Download");
                    let file = PathBuf::from(self.setup.map.replace('\\', "/"));
                    let path = dir.join(file.file_name().unwrap_or_default());
                    match std::fs::create_dir_all(&dir).and_then(|_| std::fs::write(&path, &bytes)) {
                        Ok(()) => self.have_map(path),
                        Err(e) => log::warn!("lan: saving the map: {e}"),
                    }
                }
                client::Event::CountdownStart => self.countdown = Some(0.0),
                client::Event::CountdownEnd => {
                    self.countdown = None;
                    return self.client_start();
                }
                client::Event::Closed => {
                    self.client = None;
                    return Some(Event::Joining(None));
                }
                _ => {}
            }
        }
        event
    }

    /// The host's map on this computer, if it is here: at its path, or in Maps\Download,
    /// the same size and checksum.
    fn find_map(&self, name: &str, size: u32, crc32: u32) -> Option<PathBuf> {
        let at = self.map_path(name);
        let download = self.game_dir.join("Maps").join("Download").join(at.file_name()?);
        [at, download].into_iter().find(|p| std::fs::read(p).is_ok_and(|b| b.len() as u32 == size && crc32fast::hash(&b) == crc32))
    }

    /// This computer has the host's map: the setup takes its slots.
    fn have_map(&mut self, path: PathBuf) {
        let saved = self.save.as_ref().and_then(|b| w3_game::save::SaveGame::parse(b).ok());
        if let Some(save) = saved {
            self.setup.slots = Lan::saved_setup(&save).slots;
        } else if let Some(summary) = crate::skirmish::summarize_path(&path) {
            let base = Lan::map_setup(&summary, self.setup.flags);
            (self.setup.slots, self.setup.fixed, self.setup.forces) = (base.slots, base.fixed, base.forces);
        }
        self.local_map = Some(path);
        self.take_table();
    }

    /// A joined player's setup from the host's slot table, once it has the map.
    fn take_table(&mut self) {
        let (Some(table), false) = (self.table.as_ref(), self.setup.slots.is_empty()) else { return };
        let people = self.people();
        if table.slots.len() == self.setup.slots.len() {
            self.setup.take_table(table, &people);
        }
        self.rebuild = true;
    }

    /// "Game starting in N ..." in the chat with the timer's tick (FUN_6f2fab40).
    fn countdown_line(&mut self, db: &FrameDb, n: u8) {
        self.chat.push(db.string("TIMER_COUNTDOWN").replacen("%d", &n.to_string(), 1));
        self.sounds.push("ChatroomTimerTick".to_string());
    }

    /// The host presses Start Game (FUN_6f2f9ce0): with fewer than two players, fewer
    /// than two who are not observers, or all of them on one team, the message to show;
    /// otherwise the countdown begins.
    pub fn begin_start(&mut self) -> Option<&'static str> {
        if self.host.is_none() || self.countdown.is_some() {
            return None;
        }
        let used: Vec<i32> = self.setup.slots.iter().filter(|s| matches!(s.holder, Holder::Human(_) | Holder::Computer(_))).map(|s| s.team).collect();
        let playing: Vec<i32> = used.iter().copied().filter(|&t| t != OBSERVERS).collect();
        if used.len() == 1 {
            return Some("NEED_MORE_THAN_ONE");
        }
        if playing.len() < 2 {
            return Some("NEED_AT_LEAST_TWO");
        }
        if playing.iter().all(|&t| t == playing[0]) {
            return Some("NEED_MORE_TEAMS");
        }
        if let Some(h) = self.host.as_mut() {
            h.start_countdown();
        }
        self.countdown = Some(0.0);
        None
    }

    /// The countdown: each second "Game starting in" five down to one; six seconds
    /// after Start Game the host starts the game (FUN_6f2fab40, timer 1.0), and the
    /// players when it says so.
    pub fn tick_countdown(&mut self, db: &FrameDb, dt: f32) -> Option<Event> {
        let t0 = self.countdown?;
        let t1 = t0 + dt;
        self.countdown = Some(t1);
        for s in (t0.floor() as i32 + 1)..=(t1.floor() as i32) {
            let left = 6 - s;
            if left <= 0 {
                if self.host.is_some() {
                    self.countdown = None;
                    return self.start();
                }
                break;
            }
            self.countdown_line(db, left as u8);
        }
        None
    }

    /// Whether the start's countdown is running.
    pub fn counting_down(&self) -> bool {
        self.countdown.is_some()
    }

    /// A hosted game's name: GAMENAME with the player's name, in 31 bytes
    /// (CLocalMultiplayerCreate and CLocalMultiplayerLoad, FUN_6f2ea760).
    pub fn game_name(&self, db: &FrameDb) -> String {
        let mut s = db.string("GAMENAME").replacen("%s", &self.name, 1);
        while s.len() > 31 {
            s.pop();
        }
        s
    }

    /// Each person's id and player.
    fn players(&self) -> Vec<(u8, u8)> {
        self.setup
            .slots
            .iter()
            .filter_map(|s| match s.holder {
                Holder::Human(pid) => Some((pid, s.player)),
                _ => None,
            })
            .collect()
    }

    /// The host starts the game.
    pub fn start(&mut self) -> Option<Event> {
        let mut h = self.host.take()?;
        h.end_countdown();
        let seed = h.slots.seed;
        let role = net::Role { link: net::Link::Host(h), players: self.players(), seed, game: self.setup.game.clone() };
        if self.setup.saved {
            let me = self.setup.slot_of(HOST_PID).map_or(0, |s| s.player);
            return Some(Event::StartSaved(self.map(), self.save.take().unwrap_or_default(), role, self.setup.speed, me));
        }
        let (map, lobby) = self.start_setup();
        Some(Event::Start(map, lobby, role, self.setup.speed, self.setup.flags))
    }

    /// A joined player's game begins (the host's countdown is over).
    fn client_start(&mut self) -> Option<Event> {
        let c = self.client.take()?;
        let seed = c.slots.seed;
        let role = net::Role { link: net::Link::Client(c), players: self.players(), seed, game: self.setup.game.clone() };
        if self.setup.saved {
            let me = self.setup.slot_of(self.me).map_or(0, |s| s.player);
            return Some(Event::StartSaved(self.map(), self.save.take().unwrap_or_default(), role, self.setup.speed, me));
        }
        let (map, lobby) = self.start_setup();
        Some(Event::Start(map, lobby, role, self.setup.speed, self.setup.flags))
    }

    /// The setup as this computer plays it: its own slot local, other people remote,
    /// and those on the Observers team observers.
    fn start_setup(&self) -> (PathBuf, Vec<w3_game::LobbySlot>) {
        let lobby = self
            .setup
            .slots
            .iter()
            .map(|s| w3_game::LobbySlot {
                player: s.player,
                fill: match s.holder {
                    Holder::Human(id) if s.team == OBSERVERS => w3_game::SlotFill::Observer(id == self.me),
                    Holder::Human(id) if id == self.me => w3_game::SlotFill::Local,
                    Holder::Human(_) => w3_game::SlotFill::Remote,
                    Holder::Computer(_) => w3_game::SlotFill::Computer,
                    Holder::Open | Holder::Closed => w3_game::SlotFill::Empty,
                },
                name: matches!(s.holder, Holder::Human(_)).then(|| s.name.clone()),
                race: (s.race > 0).then_some(s.race as i32),
                team: s.team,
                color: s.color,
                handicap: 100.0 - 10.0 * s.handicap as f32,
                difficulty: match s.holder {
                    Holder::Computer(d) => d,
                    _ => 1,
                },
            })
            .collect();
        (self.map(), lobby)
    }

    pub fn is_host(&self) -> bool {
        self.host.is_some()
    }

    pub fn map(&self) -> PathBuf {
        self.local_map.clone().unwrap_or_else(|| self.map_path(&self.setup.map))
    }

    /// The lobby: a PlayerSlot row per slot in TeamSetup, the host's to change, each
    /// person's own choices theirs; the chat; Start for the host.
    pub fn attach_lobby(&self, tree: &mut FrameTree, db: &FrameDb) {
        if let (Some(parent), true) = (tree.get("TeamSetupContainer"), db.templates.contains_key("TeamSetup")) {
            if let Some(i) = tree.create(db, "TeamSetup", parent) {
                tree.frames[i].all_points = true;
            }
        }
        let Some(team_setup) = tree.get("TeamSetup") else { return };
        let width = tree.get("TeamSetupContainer").and_then(|i| tree.frames[i].width);
        let host = self.is_host();
        let forces = &self.setup.forces;
        // With custom forces each force's slots come under its name (measured under
        // Wine: a 0.01 heading, 0.004 between forces).
        let mut y = 0.0;
        let mut heading = None;
        // The slots are the map's players in order; with custom forces their rows are
        // in force order.
        let mut order: Vec<usize> = (0..self.setup.slots.len()).collect();
        if !forces.is_empty() {
            order.sort_by_key(|&k| self.setup.slots[k].team);
        }
        for k in order {
            let s = &self.setup.slots[k];
            if !forces.is_empty() && s.team != OBSERVERS && heading != Some(s.team) {
                if heading.is_some() {
                    y += 0.004;
                }
                heading = Some(s.team);
                let scope = format!("#f{}", s.team);
                if tree.create_scoped(db, "StandardSmallTextTemplate", team_setup, &scope).is_some() {
                    let label = format!("StandardSmallTextTemplate{scope}");
                    tree.set_point(&label, P::TopLeft, "TeamSetup", P::TopLeft, 0.0, -y);
                    tree.set_size(&label, width, Some(0.01));
                    tree.set_text(&label, forces.get(s.team as usize).map_or("", |f| f.as_str()));
                }
                y += 0.01;
            }
            let scope = format!("#{k}");
            if tree.create_scoped(db, "PlayerSlot", team_setup, &scope).is_none() {
                continue;
            }
            let row = format!("PlayerSlot{scope}");
            tree.set_point(&row, P::TopLeft, "TeamSetup", P::TopLeft, 0.0, -y);
            y += 0.025;
            tree.set_size(&row, width, None);
            let name_menu = format!("NameMenu{scope}");
            let observer = s.team == OBSERVERS;
            match s.holder {
                Holder::Human(_) => {
                    tree.set_items(&name_menu, vec![s.name.clone()], 0);
                    tree.set_enabled(&name_menu, false);
                }
                // A loaded game's slot waits for its player, by name.
                Holder::Open if self.setup.saved && !s.name.is_empty() => {
                    tree.set_items(&name_menu, vec![s.name.clone()], 0);
                    tree.set_enabled(&name_menu, false);
                }
                h => {
                    // An observer's slot is open or closed, never a computer's.
                    let keys: &[&str] = if observer { &["OPEN", "CLOSED"] } else { &["OPEN", "CLOSED", "COMPUTER_NEWBIE", "COMPUTER_NORMAL", "COMPUTER_INSANE"] };
                    let items = keys.iter().map(|k| db.string(k)).collect();
                    let sel = match h {
                        Holder::Closed => 1,
                        Holder::Computer(d) => 2 + d.clamp(0, 2) as usize,
                        _ => 0,
                    };
                    tree.set_items(&name_menu, items, sel);
                    tree.set_enabled(&name_menu, host && !self.setup.saved && !s.locked);
                }
            }
            tree.set_selected(&format!("RaceMenu{scope}"), s.race);
            tree.set_selected(&format!("HandicapMenu{scope}"), s.handicap);
            // An observer's team is OBSERVER (REFEREE with referees), FUN_6f2bade0.
            let team = if observer {
                db.string(if self.setup.flags & 0x400000 != 0 { "REFEREE" } else { "OBSERVER" })
            } else {
                db.string("TEAM_FORMAT").replace("%d", &(s.team + 1).to_string())
            };
            tree.set_text(&format!("TeamButtonTitle{scope}"), &team);
            // The team button opens the list of teams, "TEAM %u", then the Observers
            // (FUN_6f2bc6e0); with custom forces a team is a force.
            let teams_n = if forces.is_empty() { self.setup.slots.iter().filter(|s| s.team != OBSERVERS).count() } else { forces.len() };
            let mut teams: Vec<String> = (1..=teams_n).map(|t| format!("{} {t}", db.string("TEAM"))).collect();
            if self.setup.slots.iter().any(|s| s.team == OBSERVERS) && matches!(s.holder, Holder::Human(_)) {
                teams.push(db.string(if self.setup.flags & 0x400000 != 0 { "REFEREE" } else { "OBSERVER" }));
            }
            let at = if observer { teams.len() - 1 } else { s.team.max(0) as usize };
            tree.set_pulldown(&format!("TeamButton{scope}"), &format!("RacePopupMenuMenu{scope}"), teams, at);
            // The color button opens the player colors' swatches.
            let swatches = (0..12).map(|c| format!("ReplaceableTextures\\TeamColor\\TeamColor{c:02}.blp")).collect();
            tree.set_pulldown_images(&format!("ColorButton{scope}"), &format!("RacePopupMenuMenu{scope}"), swatches, s.color.clamp(0, 11) as usize);
            // Fixed player settings show even an open slot's race, team, and color.
            let open = matches!(s.holder, Holder::Open | Holder::Closed) && !(self.setup.saved && !s.name.is_empty()) && !(self.setup.fixed && !observer);
            // A loaded game's slots are as they were saved, and a locked computer as the
            // map sets it.
            let mine = !self.setup.saved && !s.locked && (s.holder == Holder::Human(self.me) || (host && matches!(s.holder, Holder::Computer(_))));
            for f in ["RaceMenu", "TeamButton", "ColorButton", "HandicapMenu"] {
                tree.set_visible(&format!("{f}{scope}"), !open && (f == "TeamButton" || !observer));
                let fixed = self.setup.fixed && matches!(f, "RaceMenu" | "ColorButton");
                tree.set_enabled(&format!("{f}{scope}"), mine && !fixed);
                // A disabled team or color button shows no arrow (seen under Wine).
                if matches!(f, "TeamButton" | "ColorButton") {
                    tree.set_visible(&format!("{f}Arrow{scope}"), mine && !fixed);
                }
            }
        }
        tree.set_text("GameNameValue", &self.setup.game);
        tree.place_text_area_scroll_bars();
        tree.set_visible("StartGameBackdrop", host);
        crate::advanced::display(tree, db, self.setup.flags);
        self.sync_chat(tree);
    }

    pub fn sync_chat(&self, tree: &mut FrameTree) {
        tree.set_text("ChatTextArea", &self.chat.join("\n"));
        // The newest lines show (eight rows of 0.0115).
        if let Some(i) = tree.get("ChatTextArea") {
            tree.frames[i].selected = self.chat.len().saturating_sub(8);
        }
    }

    /// A lobby control: the host sets any slot, a joined player their own.
    pub fn button(&mut self, tree: &mut FrameTree, db: &FrameDb, name: &str) {
        let Some((part, k)) = name.split_once('#') else { return };
        let Ok(k) = k.parse::<usize>() else { return };
        // The map's teams: its forces, or one a player slot.
        let n = if self.setup.forces.is_empty() { self.setup.slots.iter().filter(|s| s.team != OBSERVERS).count() } else { self.setup.forces.len() } as i32;
        let fixed = self.setup.fixed;
        let observers = self.setup.slots.iter().any(|s| s.team == OBSERVERS);
        let chosen = tree.get(name).map_or(0, |i| tree.frames[i].selected);
        let me = self.me;
        let host = self.is_host();
        let mut move_to = None;
        let mut choice = Chat::Lobby(String::new());
        let saved = self.setup.saved;
        let Some(s) = self.setup.slots.get_mut(k) else { return };
        let mine = !s.locked && (s.holder == Holder::Human(me) || (host && matches!(s.holder, Holder::Computer(_))));
        match part {
            "NameMenu" if host && !saved && !s.locked && !matches!(s.holder, Holder::Human(_)) => {
                s.holder = match chosen {
                    0 => Holder::Open,
                    1 => Holder::Closed,
                    d => Holder::Computer(d as i32 - 2),
                };
            }
            "RaceMenu" if mine && !fixed => {
                s.race = chosen;
                choice = Chat::Race([0x20, 0x01, 0x02, 0x08, 0x04][chosen.min(4)]);
            }
            "HandicapMenu" if mine => {
                s.handicap = chosen;
                choice = Chat::Handicap(100 - 10 * chosen.min(5) as u8);
            }
            // The team list runs through the map's teams, then the Observers; a joined
            // player's choice is the host's to make.
            "TeamButton" if mine => {
                // The list's last entry past the map's teams is the Observers.
                let next = if chosen as i32 >= n && observers && matches!(s.holder, Holder::Human(_)) { OBSERVERS } else { (chosen as i32).min(n - 1).max(0) };
                if host {
                    move_to = Some((s.holder, next));
                }
                choice = Chat::Team(next as u8);
            }
            "ColorButton" if mine && !fixed => {
                s.color = chosen.min(11) as i32;
                choice = Chat::Color(s.color as u8);
            }
            _ => return,
        }
        if let Some((holder, team)) = move_to {
            choose_team(&mut self.setup, holder, team);
        }
        if host {
            self.send_setup();
        } else if let Some(c) = self.client.as_mut() {
            c.chat(choice);
        }
        self.rebuild = true;
        let _ = db;
    }

    /// The game list's rows, and the edit boxes' text.
    pub fn draws(&mut self, tree: &FrameTree, db: &FrameDb, out: &mut Vec<Draw>) {
        let font = db.decorate("MasterFont");
        if let Some(c) = tree.get("GameListContainer").and_then(|i| tree.frames[i].rect) {
            let inner = Rect { x0: c.x0 + 0.006, x1: c.x1 - 0.006, y0: c.y0, y1: c.y1 - 0.004 };
            self.list = Some(inner);
            for (k, (_, ad, _)) in self.games().iter().enumerate().take((inner.h() / ROW) as usize) {
                let y1 = inner.y1 - k as f32 * ROW;
                let row = Rect { x0: inner.x0, x1: inner.x1, y0: y1 - ROW, y1 };
                let chosen = self.selected == Some(k);
                if chosen {
                    out.push(Draw::Image { texture: String::new(), rect: row, uv: [0.0, 0.0, 1.0, 1.0], color: [0.0, 0.0, 0.8, 0.8], blend: w3_ui::Blend::Alpha, rotated: false });
                }
                let color = if chosen { [1.0, 0.8, 0.0, 1.0] } else { [1.0; 4] };
                // The name, then in gold how many of the slots are taken (as 1.27b counts
                // them: the slots less those open to people).
                let taken = (ad.slots_total + ad.players).saturating_sub(ad.player_slots);
                let text = format!("{} |cffFCD312({taken}/{})|r", ad.name, ad.slots_total);
                out.push(Draw::Text { font: font.clone(), size: 0.012, rect: Rect { x0: row.x0 + 0.004, ..row }, text, color, justify_h: Justify::Start, justify_v: Justify::Middle, shadow: None });
            }
        }
        crate::skirmish::color_draws(tree, self.setup.slots.iter().map(|s| s.color), out);
        // Edit boxes without a text frame of their own: their text in MasterFont, inside
        // EditBorderSize, with the cursor.
        for (frame, text, size) in [("PlayerNameEditBox", &self.name, 0.012), ("ChatEditBox", &self.line, 0.015)] {
            let Some(i) = tree.get(frame).filter(|&i| tree.is_visible(i)) else { continue };
            let Some(r) = tree.frames[i].rect else { continue };
            let inset = tree.frames[i].def.num("EditBorderSize").unwrap_or(0.009);
            let rect = Rect { x0: r.x0 + inset, x1: r.x1 - inset, ..r };
            out.push(Draw::Text { font: font.clone(), size, rect, text: format!("{text}|"), color: [1.0; 4], justify_h: Justify::Start, justify_v: Justify::Middle, shadow: None });
        }
    }
}

/// The speed's GlobalStrings name.
pub fn speed_key(speed: u8) -> &'static str {
    match speed {
        0 => "SLOW",
        1 => "NORMAL",
        _ => "FAST",
    }
}

/// The Observers team's number (CPlayerSlot's team list: the map's teams, then 12).
pub const OBSERVERS: i32 = 12;

/// A team chosen for the slot `holder` holds: moving to the Observers team takes the
/// first open observer's slot, and leaving it the first open player's slot, the old
/// slot left open; otherwise the slot's team changes.  With custom forces a slot's team
/// is its force, so another team means the first open slot of that force.
fn choose_team(setup: &mut net::Setup, holder: Holder, team: i32) {
    let Some(k) = setup.slots.iter().position(|s| s.holder == holder) else { return };
    let observer = setup.slots[k].team == OBSERVERS;
    let forces = !setup.forces.is_empty();
    if setup.slots[k].team == team || (!forces && (team == OBSERVERS) == observer) {
        if !observer {
            setup.slots[k].team = team;
        }
        return;
    }
    let to = setup.slots.iter().position(|s| s.holder == Holder::Open && if forces { s.team == team } else { (s.team == OBSERVERS) == (team == OBSERVERS) });
    let Some(to) = to else { return };
    let old = setup.slots[k].clone();
    let slot = &mut setup.slots[to];
    slot.holder = old.holder;
    slot.name = old.name;
    // Fixed player settings keep the new slot's race.
    if !setup.fixed {
        slot.race = old.race;
    }
    slot.handicap = old.handicap;
    let old_slot = &mut setup.slots[k];
    old_slot.holder = Holder::Open;
    old_slot.name.clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slot(player: u8, holder: Holder, team: i32) -> NetSlot {
        NetSlot { player, holder, name: if let Holder::Human(_) = holder { format!("p{player}") } else { String::new() }, race: 1, team, color: player as i32, handicap: 0, locked: false }
    }

    #[test]
    fn a_player_moves_to_the_observers_and_back() {
        let mut setup = net::Setup { slots: vec![slot(0, Holder::Human(0), 0), slot(1, Holder::Human(5), 1), slot(2, Holder::Open, OBSERVERS), slot(3, Holder::Open, OBSERVERS)], ..Default::default() };
        choose_team(&mut setup, Holder::Human(5), OBSERVERS);
        assert_eq!(setup.slots[1].holder, Holder::Open);
        assert_eq!(setup.slots[2].holder, Holder::Human(5));
        assert_eq!(setup.slots[2].name, "p1");
        // Back to the first open player's slot.
        choose_team(&mut setup, Holder::Human(5), 0);
        assert_eq!(setup.slots[1].holder, Holder::Human(5));
        assert_eq!(setup.slots[2].holder, Holder::Open);
        // A team change among the map's teams stays in the slot.
        choose_team(&mut setup, Holder::Human(0), 1);
        assert_eq!(setup.slots[0].team, 1);
    }

    #[test]
    fn with_custom_forces_a_team_change_takes_an_open_slot_of_that_force() {
        let mut setup = net::Setup {
            slots: vec![slot(0, Holder::Human(0), 0), slot(1, Holder::Open, 0), slot(2, Holder::Open, 1), slot(3, Holder::Computer(1), 1)],
            fixed: true,
            forces: vec!["A".into(), "B".into()],
            ..Default::default()
        };
        setup.slots[2].race = 3;
        choose_team(&mut setup, Holder::Human(0), 1);
        assert_eq!(setup.slots[0].holder, Holder::Open);
        assert_eq!(setup.slots[2].holder, Holder::Human(0));
        assert_eq!(setup.slots[2].team, 1);
        // The map fixes the race.
        assert_eq!(setup.slots[2].race, 3);
        // No open slot in the force: nothing changes.
        setup.slots[0].holder = Holder::Closed;
        setup.slots[1].holder = Holder::Closed;
        choose_team(&mut setup, Holder::Human(0), 0);
        assert_eq!(setup.slots[2].holder, Holder::Human(0));
        assert_eq!(setup.slots[2].team, 1);
    }
}
