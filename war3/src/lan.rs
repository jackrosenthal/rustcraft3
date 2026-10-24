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
}

impl Lan {
    pub fn new(game_dir: &Path, name: String) -> Lan {
        Lan { name, port: net::DEFAULT_PORT, game_dir: game_dir.to_path_buf(), browser: None, selected: None, list: None, host: None, conn: None, me: 0, setup: Setup::default(), chat: Vec::new(), line: String::new(), rebuild: false, summary_due: None }
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
        tree.set_enabled("LoadButton", false);
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
        let ad = GameAd { name: game.to_string(), host: self.name.clone(), map: self.map_name(map), players: 1, slots: slots.len() as u8, port: self.port, expansion, speed };
        let host = Host::new(ad.clone()).map_err(|e| {
            log::warn!("lan: hosting: {e}");
            "NETERROR_CREATEGAMEFAILED"
        })?;
        self.setup = Setup { game: game.to_string(), map: ad.map, slots, speed, flags };
        self.host = Some(host);
        self.me = 0;
        self.chat.clear();
        self.rebuild = true;
        Ok(())
    }

    /// Joins the chosen game.
    pub fn join(&mut self) -> Result<(), &'static str> {
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
                    match self.setup.slots.iter_mut().find(|s| s.holder == Holder::Open) {
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
                        (s.race, s.team, s.color, s.handicap) = (race, team, color, handicap);
                        changed = true;
                    }
                }
                Msg::Chat { from, text } => {
                    h.broadcast(&Msg::Chat { from: from.clone(), text: text.clone() });
                    self.chat.push(format!("{from}: {text}"));
                }
                Msg::Leave => h.drop_peer(id),
                _ => {}
            }
        }
        for id in gone {
            if let Some(s) = self.setup.slots.iter_mut().find(|s| s.holder == Holder::Human(id)) {
                s.holder = Holder::Open;
                notices.push(("NETMESSAGE_PLAYERLEFT", std::mem::take(&mut s.name)));
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
                }
                Msg::Chat { from, text } => self.chat.push(format!("{from}: {text}")),
                Msg::Notice { key, name } => self.chat.push(db.string(&key).replace("%s", &name)),
                Msg::Start(s) => {
                    self.setup = s;
                    let conn = self.conn.take()?;
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

    /// The host starts the game.
    pub fn start(&mut self) -> Option<Event> {
        let mut h = self.host.take()?;
        h.broadcast(&Msg::Start(self.setup.clone()));
        let peers = self.setup.slots.iter().filter_map(|s| match s.holder {
            Holder::Human(id) if id != 0 => Some((id, s.player, 0)),
            _ => None,
        });
        let peers = peers.collect();
        let (map, lobby) = self.start_setup();
        Some(Event::Start(map, lobby, net::Role::Host { host: h, pending: Vec::new(), peers }, self.setup.speed, self.setup.flags))
    }

    /// The setup as this computer plays it: its own slot local, other people remote.
    fn start_setup(&self) -> (PathBuf, Vec<w3_game::LobbySlot>) {
        let lobby = self
            .setup
            .slots
            .iter()
            .map(|s| w3_game::LobbySlot {
                player: s.player,
                fill: match s.holder {
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
        (self.map_path(&self.setup.map), lobby)
    }

    pub fn is_host(&self) -> bool {
        self.host.is_some()
    }

    pub fn map(&self) -> PathBuf {
        self.map_path(&self.setup.map)
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
            match s.holder {
                Holder::Human(_) => {
                    tree.set_items(&name_menu, vec![s.name.clone()], 0);
                    tree.set_enabled(&name_menu, false);
                }
                h => {
                    let items = ["OPEN", "CLOSED", "COMPUTER_NEWBIE", "COMPUTER_NORMAL", "COMPUTER_INSANE"].iter().map(|k| db.string(k)).collect();
                    let sel = match h {
                        Holder::Closed => 1,
                        Holder::Computer(d) => 2 + d.clamp(0, 2) as usize,
                        _ => 0,
                    };
                    tree.set_items(&name_menu, items, sel);
                    tree.set_enabled(&name_menu, host);
                }
            }
            tree.set_selected(&format!("RaceMenu{scope}"), s.race);
            tree.set_selected(&format!("HandicapMenu{scope}"), s.handicap);
            let team = db.string("TEAM_FORMAT").replace("%d", &(s.team + 1).to_string());
            tree.set_text(&format!("TeamButtonTitle{scope}"), &team);
            let open = matches!(s.holder, Holder::Open | Holder::Closed);
            let mine = s.holder == Holder::Human(self.me) || (host && matches!(s.holder, Holder::Computer(_)));
            for f in ["RaceMenu", "TeamButton", "ColorButton", "HandicapMenu"] {
                tree.set_visible(&format!("{f}{scope}"), !open);
                tree.set_enabled(&format!("{f}{scope}"), mine);
            }
            let _ = n;
        }
        tree.set_text("GameNameValue", &self.setup.game);
        tree.place_text_area_scroll_bars();
        tree.set_visible("StartGameBackdrop", host);
        tree.set_visible("AdvancedOptionsContainer", false);
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
        let n = self.setup.slots.len() as i32;
        let chosen = tree.get(name).map_or(0, |i| tree.frames[i].selected);
        let me = self.me;
        let host = self.is_host();
        let Some(s) = self.setup.slots.get_mut(k) else { return };
        let mine = s.holder == Holder::Human(me) || (host && matches!(s.holder, Holder::Computer(_)));
        match part {
            "NameMenu" if host && !matches!(s.holder, Holder::Human(_)) => {
                s.holder = match chosen {
                    0 => Holder::Open,
                    1 => Holder::Closed,
                    d => Holder::Computer(d as i32 - 2),
                };
            }
            "RaceMenu" if mine => s.race = chosen,
            "HandicapMenu" if mine => s.handicap = chosen,
            "TeamButton" if mine => s.team = (s.team + 1) % n.max(1),
            "ColorButton" if mine => s.color = (s.color + 1) % 12,
            _ => return,
        }
        let s = s.clone();
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
