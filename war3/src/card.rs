//! The command card: the 4x3 grid of buttons for the selected unit, built from the
//! units' abilities, Trains/Builds/Researches/Upgrade lists, and Units\CommandFunc.txt.

use w3_formats::profile::Profile;
use w3_game::World;
use w3_game::objects::{Id, id_from_str, id_str};

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    /// An order; `targeted` orders wait for a click on the world.
    Order { order: &'static str, targeted: bool },
    Train(Id),
    Research(Id),
    Upgrade(Id),
    BuildMenu,
    Build(Id),
    LearnMenu,
    Learn(Id),
    /// Buy an item from the selected shop.
    Buy(Id),
    /// Hire a unit from the selected shop.
    Hire(Id),
    /// Revive a dead hero at the selected altar.
    Revive(u32),
    Cancel,
    CancelTrain,
}

#[derive(Clone, Debug)]
pub struct Button {
    pub x: u32,
    pub y: u32,
    pub icon: String,
    pub hotkey: Option<char>,
    pub action: Action,
    pub enabled: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Page {
    #[default]
    Main,
    Build,
    Learn,
}

pub struct Commands {
    pub profile: Profile,
}

/// A hotkey field: a character, or a number naming a special key (512 is Escape).
fn hotkey(v: &str) -> Option<char> {
    match v.trim().parse::<u32>() {
        Ok(512) => Some('\u{1b}'),
        Ok(_) => None,
        Err(_) => v.trim().chars().next(),
    }
}

fn parse_pos(v: &[String]) -> Option<(u32, u32)> {
    let x = v.first()?.trim().parse().ok()?;
    let y = v.get(1)?.trim().parse().ok()?;
    Some((x, y))
}

impl Commands {
    pub fn load(chain: &w3_mpq::Chain) -> Commands {
        let mut profile = Profile::default();
        for f in ["Units\\CommandFunc.txt", "Units\\CommandStrings.txt"] {
            if let Ok(s) = chain.read_string(f) {
                profile.merge(&s);
            }
        }
        Commands { profile }
    }

    fn command(&self, db: &w3_ui::FrameDb, name: &str, action: Action, enabled: bool) -> Option<Button> {
        let (x, y) = parse_pos(&self.profile.get_list(name, "Buttonpos"))?;
        let art = self.profile.get_str(name, "Art")?;
        Some(Button {
            x,
            y,
            icon: db.decorate(&art),
            hotkey: self.profile.get_str(name, "Hotkey").and_then(|h| hotkey(&h)),
            action,
            enabled,
        })
    }

    /// A shop's goods for the local player: items, hired units, and revivals.
    pub fn shop_card(&self, w: &World, h: u32) -> Vec<Button> {
        let local = w.local_player;
        let (items, units) = w.shop_goods(h);
        let has_shopper = w.shopper(local, h).is_some() || w.unit(h).is_some_and(|u| u.owner == local);
        let mut out = Vec::new();
        let gold = w.players[local as usize].get_state(1);
        let lumber = w.players[local as usize].get_state(2);
        for id in items {
            let stock = w.stock_count(h, id);
            let Some(o) = w.objects.items.get(id) else { continue };
            let Some((x, y)) = parse_pos(&o.list("Buttonpos")) else { continue };
            let ok = has_shopper && stock > 0 && gold >= o.int("goldcost") && lumber >= o.int("lumbercost");
            out.push(Button { x, y, icon: o.str("Art"), hotkey: hotkey(&o.str("Hotkey")), action: Action::Buy(id), enabled: ok });
        }
        for id in units {
            let stock = w.stock_count(h, id);
            let Some(o) = w.objects.units.get(id) else { continue };
            let Some((x, y)) = parse_pos(&o.list("Buttonpos")) else { continue };
            let ok = has_shopper && stock > 0 && gold >= o.int("goldcost") && lumber >= o.int("lumbercost");
            out.push(Button { x, y, icon: o.str("Art"), hotkey: hotkey(&o.str("Hotkey")), action: Action::Hire(id), enabled: ok });
        }
        out
    }

    /// Buttons for a unit owned by the local player.
    pub fn card(&self, db: &w3_ui::FrameDb, w: &World, h: u32, page: Page) -> Vec<Button> {
        let Some(u) = w.unit(h) else { return vec![] };
        let Some(o) = w.objects.units.get(u.type_id) else { return vec![] };
        let mut out = Vec::new();
        let object_button = |id: Id, action: Action, enabled: bool| -> Option<Button> {
            let ob = w.objects.units.get(id).or_else(|| w.objects.upgrades.get(id)).or_else(|| w.objects.abilities.get(id))?;
            let (x, y) = parse_pos(&ob.list("Buttonpos"))?;
            Some(Button {
                x,
                y,
                icon: ob.str("Art"),
                hotkey: hotkey(&ob.str("Hotkey")),
                action,
                enabled,
            })
        };
        if page == Page::Build {
            for b in o.list("Builds") {
                let id = id_from_str(b.trim());
                let (g, l, _) = w.cost(id);
                let p = &w.players[u.owner as usize];
                let affordable = p.get_state(1) >= g && p.get_state(2) >= l;
                if let Some(btn) = object_button(id, Action::Build(id), affordable) {
                    out.push(btn);
                }
            }
            out.extend(self.command(db, "CmdCancel", Action::Cancel, true));
            return out;
        }
        if page == Page::Learn {
            for b in o.list("heroAbilList") {
                let id = id_from_str(b.trim());
                let Some(ab) = w.objects.abilities.get(id) else { continue };
                let Some((x, y)) = parse_pos(&ab.list("Researchbuttonpos")) else { continue };
                let icon = ab.str("Researchart");
                let icon = if icon.is_empty() { ab.str("Art") } else { icon };
                out.push(Button { x, y, icon, hotkey: hotkey(&ab.str("Researchhotkey")), action: Action::Learn(id), enabled: w.can_learn(h, id) });
            }
            out.extend(self.command(db, "CmdCancel", Action::Cancel, true));
            return out;
        }
        if u.constructing.is_some() {
            out.extend(self.command(db, "CmdCancelBuild", Action::CancelTrain, true));
            return out;
        }
        let mobile = !u.is_building && u.move_speed > 0.0;
        if mobile {
            out.extend(self.command(db, "CmdMove", Action::Order { order: "move", targeted: true }, true));
            out.extend(self.command(db, "CmdStop", Action::Order { order: "stop", targeted: false }, true));
            out.extend(self.command(db, "CmdHoldPos", Action::Order { order: "holdposition", targeted: false }, true));
            if w3_game::sim::weapon(w, u.type_id).is_some() {
                out.extend(self.command(db, "CmdAttack", Action::Order { order: "attack", targeted: true }, true));
            }
            out.extend(self.command(db, "CmdPatrol", Action::Order { order: "patrol", targeted: true }, true));
        }
        if u.is_hero && u.skill_points > 0 {
            out.extend(self.command(db, "CmdSelectSkill", Action::LearnMenu, true));
        }
        if !o.list("Builds").is_empty() {
            let race = o.str("race").to_ascii_lowercase();
            let cmd = match race.as_str() {
                "human" => "CmdBuildHuman",
                "orc" => "CmdBuildOrc",
                "nightelf" => "CmdBuildNightElf",
                "undead" => "CmdBuildUndead",
                _ => "CmdBuild",
            };
            out.extend(self.command(db, cmd, Action::BuildMenu, true));
        }
        for t in o.list("Trains") {
            let id = id_from_str(t.trim());
            let (g, l, _) = w.cost(id);
            let p = &w.players[u.owner as usize];
            let ok = p.get_state(1) >= g && p.get_state(2) >= l;
            out.extend(object_button(id, Action::Train(id), ok));
        }
        for r in o.list("Researches") {
            let id = id_from_str(r.trim());
            out.extend(object_button(id, Action::Research(id), true));
        }
        for r in o.list("Upgrade") {
            let id = id_from_str(r.trim());
            out.extend(object_button(id, Action::Upgrade(id), true));
        }
        for &(a, _) in &u.abilities {
            let Some(ab) = w.objects.abilities.get(a) else { continue };
            // Abilities with a button position appear on the card.
            let Some((x, y)) = parse_pos(&ab.list("Buttonpos")) else { continue };
            if ab.str("Art").is_empty() {
                continue;
            }
            // The ability's order string, as one of the game's order names.
            let order = w3_game::natives::basic::order_name(w3_game::natives::basic::order_id(&ab.str("Order"))).unwrap_or("");
            let targeting = w.targeting(a);
            let (order, targeted) = match ab.str("code").as_str() {
                "Ahar" | "Ahrl" => ("harvest", true),
                _ if targeting == w3_game::ability::Targeting::Passive => ("", false),
                _ => (order, targeting != w3_game::ability::Targeting::None),
            };
            let ready = order.is_empty() || w.can_cast(h, a).is_ok();
            out.push(Button { x, y, icon: ab.str("Art"), hotkey: hotkey(&ab.str("Hotkey")), action: Action::Order { order, targeted }, enabled: ready });
        }
        // An altar offers its owner's dead heroes, where their train buttons would be.
        for hero in w.revivable(h) {
            let Some(hu) = w.unit(hero) else { continue };
            let (g, l, _) = w.revive_cost(hero);
            let p = &w.players[u.owner as usize];
            let ok = p.get_state(1) >= g && p.get_state(2) >= l && u.revive.is_none();
            out.extend(object_button(hu.type_id, Action::Revive(hero), ok));
        }
        if !u.queue.is_empty() || u.revive.is_some() {
            out.extend(self.command(db, "CmdCancelTrain", Action::CancelTrain, true));
        }
        // Later buttons with the same slot win, as the card fills in order.
        let mut dedup: Vec<Button> = Vec::new();
        for b in out {
            dedup.retain(|x: &Button| (x.x, x.y) != (b.x, b.y));
            dedup.push(b);
        }
        let _ = id_str;
        dedup
    }
}

/// The disabled version of a command button icon.
pub fn disabled_icon(icon: &str) -> String {
    let lower = icon.to_ascii_lowercase();
    match lower.find("commandbuttons\\") {
        Some(i) => {
            let file = &icon[i + "commandbuttons\\".len()..];
            format!("{}CommandButtonsDisabled\\DIS{file}", &icon[..i])
        }
        None => icon.to_string(),
    }
}
