//! The Local Area Network screens: CLocalMultiplayerJoin (the games found, the player's
//! name, Create and Join), CLocalMultiplayerCreate (a map to host, which the Custom Game
//! screen's map list chooses), and CGameChatroom (the game setup everyone shares, with
//! chat, which the host starts).

use std::path::{Path, PathBuf};

use w3_ui::{Draw, FrameDb, FramePoint as P, FrameTree, Justify, Rect};

use crate::net::{self, Browser, Conn, GameAd, Holder, Host, Msg, NetSlot, Setup};

const ROW: f32 = 0.0195;
/// CLocalMultiplayerJoin's name box takes 15 characters (FUN_6f104350).
const NAME_LEN: usize = 15;

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
    /// The lobby: as host, or joined to one; this computer's connection number (the
    /// host's is 0).
    host: Option<Host>,
    conn: Option<Conn>,
    me: u32,
    pub setup: Setup,
    pub chat: Vec<String>,
    pub line: String,
    /// The lobby's frames need rebuilding (the setup changed).
    pub rebuild: bool,
    /// The game list's selection changed, and its map's details are due.
    pub summary_due: Option<PathBuf>,
    /// A joined player's copy of the host's map, when it came from the host
    /// (Maps\Download), and whether it has been asked for.
    local_map: Option<PathBuf>,
    map_asked: bool,
    /// The saved game being loaded (the host's file, or what it sent).
    save: Option<Vec<u8>>,
    /// The host's countdown to the start: seconds since Start Game was pressed.
    countdown: Option<f32>,
    /// Sound labels for the glue to play.
    pub sounds: Vec<String>,
}

impl Lan {
    pub fn new(game_dir: &Path, name: String) -> Lan {
        Lan { name, port: net::DEFAULT_PORT, game_dir: game_dir.to_path_buf(), browser: None, selected: None, list: None, host: None, conn: None, me: 0, setup: Setup::default(), chat: Vec::new(), line: String::new(), rebuild: false, summary_due: None, local_map: None, map_asked: false, save: None, countdown: None, sounds: Vec::new() }
    }

    /// A map's path as the setup names it: within the game folder, relative to it.
    fn map_name(&self, map: &Path) -> String {
        map.strip_prefix(&self.game_dir).unwrap_or(map).to_string_lossy().replace('\\', "/")
    }

    fn map_path(&self, name: &str) -> PathBuf {
        let p = PathBuf::from(name);
        if p.is_absolute() { p } else { self.game_dir.join(p) }
    }

    /// The game list screen: searching starts.
    pub fn attach_join(&mut self, tree: &mut FrameTree, db: &FrameDb, port: u16) {
        self.port = port;
        if self.browser.is_none() {
            self.browser = Browser::new(port).map_err(|e| log::warn!("lan: {e}")).ok();
        }
        self.selected = None;
        self.sync_join(tree, db);
    }

    pub fn detach(&mut self) {
        self.browser = None;
    }

    fn games(&self) -> &[(std::net::SocketAddr, GameAd, std::time::Instant)] {
        self.browser.as_ref().map_or(&[], |b| &b.games)
    }

    fn sync_join(&self, tree: &mut FrameTree, db: &FrameDb) {
        let ad = self.selected.and_then(|k| self.games().get(k)).map(|g| &g.1);
        tree.set_text("GameCreatorValue", ad.map_or("", |a| a.host.as_str()));
        tree.set_text("GameSpeedValue", &ad.map_or(String::new(), |a| db.string(speed_key(a.speed))));
        crate::advanced::display(tree, db, ad.map_or(crate::advanced::DEFAULT_FLAGS, |a| a.flags));
        tree.set_enabled("JoinButton", ad.is_some() && !self.name.trim().is_empty());
        tree.set_enabled("CreateButton", !self.name.trim().is_empty());
    }

    /// Searches go on, the lobby's messages come in.
    pub fn update(&mut self, tree: &mut FrameTree, db: &FrameDb) -> Option<Event> {
        if let Some(b) = self.browser.as_mut() {
            let before = b.games.len();
            b.poll();
            if b.games.len() != before {
                self.selected = self.selected.filter(|&k| k < b.games.len());
                self.sync_join(tree, db);
            }
        }
        if self.host.is_some() {
            return self.host_update(db);
        }
        if self.conn.is_some() {
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
            self.summary_due = Some(self.map_path(&self.games()[k].1.map));
            self.sync_join(tree, db);
        }
    }

    /// Hosts a game on a map, its user slots open for others, the host in the first.
    pub fn host_game(&mut self, map: &Path, summary: &crate::skirmish::MapSummary, game: &str, expansion: bool, speed: u8, flags: u32) -> Result<(), &'static str> {
        let slots: Vec<NetSlot> = summary
            .info
            .players
            .iter()
            .filter(|p| p.kind == 1)
            .enumerate()
            .map(|(k, p)| NetSlot {
                player: p.id as u8,
                holder: if k == 0 { Holder::Human(0) } else { Holder::Open },
                name: if k == 0 { self.name.clone() } else { String::new() },
                race: 0,
                team: k as i32,
                color: p.id as i32,
                handicap: 0,
            })
            .collect();
        // With full observers or referees, the players the map leaves unused are the
        // Observers team (team 12, as CPlayerSlot's team list ends).
        let mut slots = slots;
        if flags & 0x10 != 0 || flags & 0x400000 != 0 {
            let used: Vec<u8> = slots.iter().map(|s| s.player).collect();
            for id in (0..12u8).filter(|id| !used.contains(id)) {
                slots.push(NetSlot { player: id, holder: Holder::Open, name: String::new(), race: 0, team: OBSERVERS, color: id as i32, handicap: 0 });
            }
        }
        let ad = GameAd { name: game.to_string(), host: self.name.clone(), map: self.map_name(map), players: 1, slots: slots.len() as u8, port: self.port, expansion, speed, flags };
        let host = Host::new(ad.clone()).map_err(|e| {
            log::warn!("lan: hosting: {e}");
            "NETERROR_CREATEGAMEFAILED"
        })?;
        self.setup = Setup { game: game.to_string(), map: ad.map, slots, speed, flags, saved: false };
        self.host = Some(host);
        self.me = 0;
        self.chat.clear();
        self.rebuild = true;
        Ok(())
    }

    /// Joins the chosen game.
    /// Hosts a saved network game (CLocalMultiplayerLoad): its map and speed, its
    /// people's slots kept for them by name, its computers as they were.
    pub fn host_saved(&mut self, file: &Path, game: &str, expansion: bool) -> Result<(), &'static str> {
        let bytes = std::fs::read(file).map_err(|_| "SAVE_ERROR_BAD_SAVE")?;
        let save = w3_game::save::SaveGame::parse(&bytes).map_err(|_| "SAVE_ERROR_BAD_SAVE")?;
        let players = save.players();
        if !players.iter().any(|p| p.human && p.name.eq_ignore_ascii_case(&self.name)) {
            return Err("NETERROR_JOINGAMEFAILED");
        }
        let map = self.map_name(Path::new(&save.map));
        let slots: Vec<NetSlot> = players
            .iter()
            .map(|p| NetSlot {
                player: p.id,
                holder: if p.computer {
                    Holder::Computer(1)
                } else if p.name.eq_ignore_ascii_case(&self.name) {
                    Holder::Human(0)
                } else {
                    Holder::Open
                },
                name: if p.computer { String::new() } else { p.name.clone() },
                race: p.race.clamp(0, 4) as usize,
                team: if p.observer { OBSERVERS } else { p.team },
                color: p.color,
                handicap: 0,
            })
            .collect();
        let game = game.to_string();
        let ad = GameAd { name: game.clone(), host: self.name.clone(), map: map.clone(), players: 1, slots: slots.len() as u8, port: self.port, expansion, speed: 2, flags: crate::advanced::DEFAULT_FLAGS };
        let host = Host::new(ad).map_err(|e| {
            log::warn!("lan: hosting: {e}");
            "NETERROR_CREATEGAMEFAILED"
        })?;
        self.setup = Setup { game, map, slots, speed: 2, flags: crate::advanced::DEFAULT_FLAGS, saved: true };
        self.host = Some(host);
        self.save = Some(bytes);
        self.me = 0;
        self.chat.clear();
        self.rebuild = true;
        Ok(())
    }

    pub fn join(&mut self) -> Result<(), &'static str> {
        self.local_map = None;
        self.map_asked = false;
        let Some((addr, _, _)) = self.selected.and_then(|k| self.games().get(k)).cloned() else { return Err("NETERROR_NOGAMESPECIFIED") };
        let mut c = Conn::connect(addr).map_err(|_| "NETERROR_JOINGAMEFAILED")?;
        c.send(&Msg::Hello { name: self.name.clone(), version: net::VERSION });
        self.conn = Some(c);
        self.chat.clear();
        Ok(())
    }

    /// Leaves the lobby (a host's closes it).
    pub fn leave(&mut self) {
        if let Some(mut h) = self.host.take() {
            h.broadcast(&Msg::Leave);
        }
        if let Some(mut c) = self.conn.take() {
            c.send(&Msg::Leave);
        }
        self.setup = Setup::default();
    }

    fn say(&mut self, text: &str) {
        let from = self.name.clone();
        if let Some(h) = self.host.as_mut() {
            h.broadcast(&Msg::Chat { from: from.clone(), text: text.to_string() });
            self.chat.push(format!("{from}: {text}"));
        } else if let Some(c) = self.conn.as_mut() {
            c.send(&Msg::Chat { from, text: text.to_string() });
        }
    }

    /// A notice everyone in the lobby sees (NETMESSAGE_PLAYERJOINED, _PLAYERLEFT).
    fn notice(&mut self, db: &FrameDb, key: &str, name: &str) {
        self.chat.push(db.string(key).replace("%s", name));
        if let Some(h) = self.host.as_mut() {
            h.broadcast(&Msg::Notice { key: key.to_string(), name: name.to_string() });
        }
    }

    fn host_update(&mut self, db: &FrameDb) -> Option<Event> {
        let map_file = self.map_path(&self.setup.map);
        let h = self.host.as_mut()?;
        let (msgs, gone) = h.poll();
        let mut changed = false;
        let mut notices = Vec::new();
        for (id, m) in msgs {
            match m {
                Msg::Hello { name, version } => {
                    if version != net::VERSION {
                        h.send(id, &Msg::Refused("NETERROR_JOINGAMEFAILED".into()));
                        h.drop_peer(id);
                        continue;
                    }
                    // A loaded game seats only those who were in it, each in their slot.
                    let saved = self.setup.saved;
                    match self.setup.slots.iter_mut().find(|s| s.holder == Holder::Open && (!saved || s.name.eq_ignore_ascii_case(&name))) {
                        Some(s) => {
                            s.holder = Holder::Human(id);
                            s.name = name.clone();
                            notices.push(("NETMESSAGE_PLAYERJOINED", name.clone()));
                            if let Some(p) = h.peers.iter_mut().find(|p| p.0 == id) {
                                p.2 = Some(name);
                            }
                            h.send(id, &Msg::Welcome { id });
                            changed = true;
                        }
                        None => {
                            h.send(id, &Msg::Refused("NETERROR_JOINGAMEFAILED".into()));
                            h.drop_peer(id);
                        }
                    }
                }
                Msg::Choose { race, team, color, handicap } => {
                    if let Some(s) = self.setup.slots.iter_mut().find(|s| s.holder == Holder::Human(id)) {
                        (s.race, s.color, s.handicap) = (race, color, handicap);
                        changed = true;
                    }
                    choose_team(&mut self.setup, Holder::Human(id), team);
                }
                Msg::Chat { from, text } => {
                    h.broadcast(&Msg::Chat { from: from.clone(), text: text.clone() });
                    self.chat.push(format!("{from}: {text}"));
                }
                Msg::Leave => h.drop_peer(id),
                // The host sends its map to a player who lacks it.
                Msg::MapRequest => {
                    match std::fs::read(&map_file) {
                        Ok(bytes) => h.send(id, &Msg::MapData { name: self.setup.map.clone(), bytes }),
                        Err(e) => log::warn!("lan: reading the map: {e}"),
                    }
                }
                _ => {}
            }
        }
        for id in gone {
            if let Some(s) = self.setup.slots.iter_mut().find(|s| s.holder == Holder::Human(id)) {
                s.holder = Holder::Open;
                // A loaded game's slot stays kept for its player.
                let name = if self.setup.saved { s.name.clone() } else { std::mem::take(&mut s.name) };
                notices.push(("NETMESSAGE_PLAYERLEFT", name));
                changed = true;
            }
        }
        let players = self.setup.slots.iter().filter(|s| matches!(s.holder, Holder::Human(_))).count() as u8;
        h.ad.players = players;
        if changed {
            h.broadcast(&Msg::Setup(self.setup.clone()));
            self.rebuild = true;
        }
        for (key, name) in notices {
            self.notice(db, key, &name);
        }
        None
    }

    fn client_update(&mut self, db: &FrameDb) -> Option<Event> {
        let c = self.conn.as_mut()?;
        let msgs = c.poll();
        let alive = c.alive;
        let mut event = None;
        for m in msgs {
            match m {
                Msg::Welcome { id } => {
                    self.me = id;
                    event = Some(Event::Lobby);
                }
                Msg::Refused(key) => {
                    self.conn = None;
                    return Some(Event::Joining(Some(refusal(&key))));
                }
                Msg::Setup(s) => {
                    self.setup = s;
                    self.rebuild = true;
                    // A map this computer lacks is asked of the host.
                    if self.local_map.is_none() && !self.map_path(&self.setup.map).is_file() && !self.map_asked {
                        self.map_asked = true;
                        if let Some(c) = self.conn.as_mut() {
                            c.send(&Msg::MapRequest);
                        }
                    }
                }
                Msg::MapData { name, bytes } => {
                    let dir = self.game_dir.join("Maps").join("Download");
                    let path = dir.join(Path::new(&name).file_name().unwrap_or_default());
                    match std::fs::create_dir_all(&dir).and_then(|_| std::fs::write(&path, &bytes)) {
                        Ok(()) => {
                            self.local_map = Some(path);
                            self.rebuild = true;
                        }
                        Err(e) => log::warn!("lan: saving the map: {e}"),
                    }
                }
                Msg::Chat { from, text } => self.chat.push(format!("{from}: {text}")),
                Msg::Notice { key, name } => self.chat.push(db.string(&key).replace("%s", &name)),
                Msg::Countdown(n) => self.countdown_line(db, n),
                Msg::SaveData(bytes) => self.save = Some(bytes),
                Msg::Start(s) => {
                    self.setup = s;
                    let conn = self.conn.take()?;
                    if self.setup.saved {
                        let me = self.setup.slots.iter().find(|s| s.holder == Holder::Human(self.me)).map_or(0, |s| s.player);
                        let save = self.save.take().unwrap_or_default();
                        return Some(Event::StartSaved(self.map(), save, net::Role::Client { conn }, self.setup.speed, me));
                    }
                    let (map, lobby) = self.start_setup();
                    return Some(Event::Start(map, lobby, net::Role::Client { conn }, self.setup.speed, self.setup.flags));
                }
                Msg::Leave => {
                    self.conn = None;
                    return Some(Event::Joining(None));
                }
                _ => {}
            }
        }
        if !alive {
            self.conn = None;
            return Some(Event::Joining(Some("NETERROR_JOINGAMEFAILED")));
        }
        event
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
        self.countdown = Some(0.0);
        None
    }

    /// The host's countdown: each second "Game starting in" five down to one, everyone
    /// told; six seconds after Start Game the game starts (FUN_6f2fab40, timer 1.0).
    pub fn tick_countdown(&mut self, db: &FrameDb, dt: f32) -> Option<Event> {
        let t0 = self.countdown?;
        let t1 = t0 + dt;
        self.countdown = Some(t1);
        for s in (t0.floor() as i32 + 1)..=(t1.floor() as i32) {
            let left = 6 - s;
            if left <= 0 {
                self.countdown = None;
                return self.start();
            }
            let n = left as u8;
            if let Some(h) = self.host.as_mut() {
                h.broadcast(&Msg::Countdown(n));
            }
            self.countdown_line(db, n);
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

    /// The host starts the game.
    pub fn start(&mut self) -> Option<Event> {
        let mut h = self.host.take()?;
        // A loaded game's state goes to everyone first.
        if self.setup.saved {
            if let Some(bytes) = self.save.as_ref() {
                h.broadcast(&Msg::SaveData(bytes.clone()));
            }
        }
        h.broadcast(&Msg::Start(self.setup.clone()));
        let peers = self.setup.slots.iter().filter_map(|s| match s.holder {
            Holder::Human(id) if id != 0 => Some((id, s.player, 0)),
            _ => None,
        });
        let peers = peers.collect();
        let role = net::Role::Host { host: h, pending: Vec::new(), peers };
        if self.setup.saved {
            let me = self.setup.slots.iter().find(|s| s.holder == Holder::Human(0)).map_or(0, |s| s.player);
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
        let n = self.setup.slots.len();
        for (k, s) in self.setup.slots.iter().enumerate() {
            let scope = format!("#{k}");
            if tree.create_scoped(db, "PlayerSlot", team_setup, &scope).is_none() {
                continue;
            }
            let row = format!("PlayerSlot{scope}");
            tree.set_point(&row, P::TopLeft, "TeamSetup", P::TopLeft, 0.0, -(k as f32) * 0.025);
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
                    tree.set_enabled(&name_menu, host && !self.setup.saved);
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
            // (FUN_6f2bc6e0).
            let teams_n = self.setup.slots.iter().filter(|s| s.team != OBSERVERS).count();
            let mut teams: Vec<String> = (1..=teams_n).map(|t| format!("{} {t}", db.string("TEAM"))).collect();
            if self.setup.slots.iter().any(|s| s.team == OBSERVERS) && matches!(s.holder, Holder::Human(_)) {
                teams.push(db.string(if self.setup.flags & 0x400000 != 0 { "REFEREE" } else { "OBSERVER" }));
            }
            let at = if observer { teams.len() - 1 } else { s.team.max(0) as usize };
            tree.set_pulldown(&format!("TeamButton{scope}"), &format!("RacePopupMenuMenu{scope}"), teams, at);
            // The color button opens the player colors' swatches.
            let swatches = (0..12).map(|c| format!("ReplaceableTextures\\TeamColor\\TeamColor{c:02}.blp")).collect();
            tree.set_pulldown_images(&format!("ColorButton{scope}"), &format!("RacePopupMenuMenu{scope}"), swatches, s.color.clamp(0, 11) as usize);
            let open = matches!(s.holder, Holder::Open | Holder::Closed) && !(self.setup.saved && !s.name.is_empty());
            // A loaded game's slots are as they were saved.
            let mine = !self.setup.saved && (s.holder == Holder::Human(self.me) || (host && matches!(s.holder, Holder::Computer(_))));
            for f in ["RaceMenu", "TeamButton", "ColorButton", "HandicapMenu"] {
                tree.set_visible(&format!("{f}{scope}"), !open && (f == "TeamButton" || !observer));
                tree.set_enabled(&format!("{f}{scope}"), mine);
            }
            let _ = n;
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
        // The map's teams: one a player slot.
        let n = self.setup.slots.iter().filter(|s| s.team != OBSERVERS).count() as i32;
        let observers = self.setup.slots.iter().any(|s| s.team == OBSERVERS);
        let chosen = tree.get(name).map_or(0, |i| tree.frames[i].selected);
        let me = self.me;
        let host = self.is_host();
        let mut move_to = None;
        let saved = self.setup.saved;
        let Some(s) = self.setup.slots.get_mut(k) else { return };
        let mine = s.holder == Holder::Human(me) || (host && matches!(s.holder, Holder::Computer(_)));
        match part {
            "NameMenu" if host && !saved && !matches!(s.holder, Holder::Human(_)) => {
                s.holder = match chosen {
                    0 => Holder::Open,
                    1 => Holder::Closed,
                    d => Holder::Computer(d as i32 - 2),
                };
            }
            "RaceMenu" if mine => s.race = chosen,
            "HandicapMenu" if mine => s.handicap = chosen,
            // The team list runs through the map's teams, then the Observers.
            "TeamButton" if mine => {
                // The list's last entry past the map's teams is the Observers.
                let next = if chosen as i32 >= n && observers && matches!(s.holder, Holder::Human(_)) { OBSERVERS } else { (chosen as i32).min(n - 1).max(0) };
                if host {
                    move_to = Some((s.holder, next));
                } else {
                    s.team = next;
                }
            }
            "ColorButton" if mine => s.color = chosen.min(11) as i32,
            _ => return,
        }
        let s = s.clone();
        if let Some((holder, team)) = move_to {
            choose_team(&mut self.setup, holder, team);
        }
        if let Some(h) = self.host.as_mut() {
            h.broadcast(&Msg::Setup(self.setup.clone()));
        } else if let Some(c) = self.conn.as_mut() {
            c.send(&Msg::Choose { race: s.race, team: s.team, color: s.color, handicap: s.handicap });
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
                out.push(Draw::Text { font: font.clone(), size: 0.012, rect: Rect { x0: row.x0 + 0.004, ..row }, text: ad.name.clone(), color, justify_h: Justify::Start, justify_v: Justify::Middle, shadow: None });
                out.push(Draw::Text { font: font.clone(), size: 0.012, rect: Rect { x1: row.x1 - 0.004, ..row }, text: format!("{}/{}", ad.players, ad.slots), color, justify_h: Justify::End, justify_v: Justify::Middle, shadow: None });
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

/// A refusal's message key, from those this game sends.
fn refusal(key: &str) -> &'static str {
    match key {
        "NETERROR_JOINGAMENOTFOUND" => "NETERROR_JOINGAMENOTFOUND",
        _ => "NETERROR_JOINGAMEFAILED",
    }
}

/// The Observers team's number (CPlayerSlot's team list: the map's teams, then 12).
pub const OBSERVERS: i32 = 12;

/// A team chosen for the slot `holder` holds: moving to the Observers team takes the
/// first open observer's slot, and leaving it the first open player's slot, the old
/// slot left open; otherwise the slot's team changes.
fn choose_team(setup: &mut net::Setup, holder: Holder, team: i32) {
    let Some(k) = setup.slots.iter().position(|s| s.holder == holder) else { return };
    let observer = setup.slots[k].team == OBSERVERS;
    if (team == OBSERVERS) == observer {
        if !observer {
            setup.slots[k].team = team;
        }
        return;
    }
    let to = setup.slots.iter().position(|s| s.holder == Holder::Open && (s.team == OBSERVERS) == (team == OBSERVERS));
    let Some(to) = to else { return };
    let old = setup.slots[k].clone();
    let slot = &mut setup.slots[to];
    slot.holder = old.holder;
    slot.name = old.name;
    slot.race = old.race;
    slot.handicap = old.handicap;
    let old_slot = &mut setup.slots[k];
    old_slot.holder = Holder::Open;
    old_slot.name.clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slot(player: u8, holder: Holder, team: i32) -> NetSlot {
        NetSlot { player, holder, name: if let Holder::Human(_) = holder { format!("p{player}") } else { String::new() }, race: 1, team, color: player as i32, handicap: 0 }
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
}
