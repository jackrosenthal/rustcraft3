//! The command card: the 4x3 grid of buttons for the selected unit, built from the
//! units' abilities, Trains/Builds/Researches/Upgrade lists, and Units\CommandFunc.txt.

use w3_formats::profile::Profile;
use w3_game::World;
use w3_game::objects::{Id, id_from_str, id_str};

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    /// An order; `targeted` orders wait for a click on the world.
    Order { order: &'static str, targeted: bool },
    /// An order without a name (an ability's built-in order).
    OrderId { order: i32, targeted: bool },
    Train(Id),
    Research(Id),
    Upgrade(Id),
    BuildMenu,
    Build(Id),
    LearnMenu,
    Learn(Id),
    /// Open a spell book's page.
    BookMenu(Id),
    /// Buy an item from the selected shop.
    Buy(Id),
    /// Hire a unit from the selected shop.
    Hire(Id),
    /// Revive a dead hero at the selected altar.
    Revive(u32),
    /// Awaken a dead hero at the selected tavern.
    Awaken(u32),
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
    /// Tooltip title and text (Tip and Ubertip, placeholders filled in).
    pub tip: String,
    pub ubertip: String,
    /// Mana, gold, lumber, and food costs shown in the tooltip.
    pub costs: [i32; 4],
    /// An autocast ability's orderon and orderoff, and whether its autocast is on.
    pub autocast: Option<(&'static str, &'static str, bool)>,
    /// REQUIRESTOOLTIP and the requirements, while some are not met.
    pub requires: String,
    /// The button of the unit's current order, under CommandButtonActiveHighlight.
    pub active: bool,
    /// An ability cooling down: the seconds left and its whole cooldown, for
    /// CommandButtonCooldown.
    pub cooldown: Option<(f32, f32)>,
    /// A count shown in the button's corner (CommandButtonNumberOverlay).
    pub number: Option<i32>,
}

impl Button {
    /// A button standing only for its tooltip.
    pub fn text_only(tip: String, ubertip: String) -> Button {
        Button { tip, ubertip, ..Button::new(0, 0, String::new(), None, Action::Order { order: "", targeted: false }, true) }
    }

    fn new(x: u32, y: u32, icon: String, hotkey: Option<char>, action: Action, enabled: bool) -> Button {
        Button { x, y, icon, hotkey, action, enabled, tip: String::new(), ubertip: String::new(), costs: [0; 4], autocast: None, requires: String::new(), active: false, cooldown: None, number: None }
    }

    /// Disables the button and lists `id`'s requirements (at research level `level`)
    /// when `owner` does not meet them all.
    fn with_requirements(mut self, db: &w3_ui::FrameDb, w: &World, owner: u8, id: Id, level: u32) -> Button {
        if w.missing_requirements(owner, id, level).is_empty() {
            return self;
        }
        self.enabled = false;
        let mut text = db.string("REQUIRESTOOLTIP");
        for (r, amount) in w.requirements(id, level) {
            // A research requirement names its level.
            let name = match w.objects.upgrades.get(r) {
                Some(o) => level_entry(&o.list("Name"), amount.max(1) as u32),
                None => w.objects.units.get(r).or_else(|| w.dependency_group(r)).map(|o| o.str("Name")).unwrap_or_default(),
            };
            text.push_str(&format!("\n - {name}"));
        }
        self.requires = text;
        self
    }

    fn with_tip(mut self, tip: String, ubertip: String, costs: [i32; 4]) -> Button {
        self.tip = tip;
        self.ubertip = ubertip;
        self.costs = costs;
        self
    }
}

/// Fills a tooltip's `<ID,Field>` and `<ID,Field,%>` placeholders with the object's
/// value (a percentage when marked), whole numbers without decimals.
pub fn fill_placeholders(w: &World, s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find('<') {
        out.push_str(&rest[..i]);
        let Some(j) = rest[i..].find('>') else {
            out.push_str(&rest[i..]);
            return out;
        };
        let inner = &rest[i + 1..i + j];
        let parts: Vec<&str> = inner.split(',').collect();
        let value = (parts.len() >= 2 && parts[0].len() == 4).then(|| {
            let id = id_from_str(parts[0]);
            let o = w.objects.abilities.get(id).or_else(|| w.objects.units.get(id)).or_else(|| w.objects.items.get(id)).or_else(|| w.objects.upgrades.get(id)).or_else(|| w.objects.buffs.get(id))?;
            let v = o.str(parts[1]);
            let pct = parts.get(2).is_some_and(|p| p.trim() == "%");
            let n: Option<f32> = v.trim().parse().ok();
            Some(match n {
                Some(n) => {
                    let n = if pct { n * 100.0 } else { n };
                    if (n - n.round()).abs() < 1e-3 { format!("{}", n.round() as i64) } else { format!("{n}") }
                }
                None => v,
            })
        });
        match value.flatten() {
            Some(v) => out.push_str(&v),
            None => out.push_str(&rest[i..i + j + 1]),
        }
        rest = &rest[i + j + 1..];
    }
    out.push_str(rest);
    out
}

/// An entry of a per-level list (Tip, Ubertip), the last when the list is shorter.
pub fn level_entry(list: &[String], level: u32) -> String {
    list.get(level.saturating_sub(1) as usize).or(list.last()).cloned().unwrap_or_default()
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Page {
    #[default]
    Main,
    Build,
    Learn,
    /// A spell book's spells.
    Book(Id),
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
    /// The command buttons' strings, with the player's custom keys over them.
    pub fn load(chain: &w3_mpq::Chain, custom_keys: Option<&Profile>) -> Commands {
        let mut profile = Profile::default();
        for f in ["Units\\CommandFunc.txt", "Units\\CommandStrings.txt"] {
            if let Ok(s) = chain.read_string(f) {
                profile.merge(&s);
            }
        }
        for (section, keys) in custom_keys.map(|k| &k.sections).into_iter().flatten() {
            if let Some(own) = profile.sections.get_mut(section) {
                own.extend(keys.iter().map(|(k, v)| (k.clone(), v.clone())));
            }
        }
        Commands { profile }
    }

    fn command(&self, db: &w3_ui::FrameDb, name: &str, action: Action, enabled: bool) -> Option<Button> {
        let (x, y) = parse_pos(&self.profile.get_list(name, "Buttonpos"))?;
        let art = self.profile.get_str(name, "Art")?;
        let tip = self.profile.get_str(name, "Tip").unwrap_or_default();
        let uber = self.profile.get_str(name, "Ubertip").unwrap_or_default();
        Some(Button::new(x, y, db.decorate(&art), self.profile.get_str(name, "Hotkey").and_then(|h| hotkey(&h)), action, enabled).with_tip(tip, uber, [0; 4]))
    }

    /// A shop's goods for the local player: items, hired units, and revivals.  A good is
    /// unavailable out of the shop's reach, before it first arrives (COOLDOWNSTOCKTOOLTIP),
    /// or sold out (OUTOFSTOCKTOOLTIP) (FUN_6f3fe0f0, CAbilitySellUnit::vf166); gold and
    /// lumber are checked when it is bought.
    pub fn shop_card(&self, db: &w3_ui::FrameDb, w: &World, h: u32) -> Vec<Button> {
        let local = w.local_player;
        let (items, units) = w.shop_goods(h);
        let has_shopper = w.shopper(local, h).is_some() || w.unit(h).is_some_and(|u| u.owner == local);
        let mut out = Vec::new();
        // Without a buyer near: "Requires: - Nearby Patron", and no price
        // (CAbilitySellItem::vf166, FUN_6f416b60).
        let patron_note = format!("{}\n - {}", db.string("REQUIRESTOOLTIP"), db.string("REQUIRESNEARPATRON"));
        let stock_note = |id: Id| -> String {
            if !has_shopper {
                patron_note.clone()
            } else if w.stock_count(h, id) > 0 {
                String::new()
            } else if w.stock_arriving(h, id) {
                db.string("COOLDOWNSTOCKTOOLTIP")
            } else {
                db.string("OUTOFSTOCKTOOLTIP")
            }
        };
        for id in items {
            let stock = w.stock_count(h, id);
            let Some(o) = w.objects.items.get(id) else { continue };
            let Some((x, y)) = parse_pos(&o.list("Buttonpos")) else { continue };
            let ok = has_shopper && stock > 0;
            let costs = if has_shopper { [0, o.int("goldcost"), o.int("lumbercost"), 0] } else { [0; 4] };
            let mut b = Button::new(x, y, o.str("Art"), hotkey(&o.str("Hotkey")), Action::Buy(id), ok).with_tip(o.str("Tip"), fill_placeholders(w, &o.str("Ubertip")), costs);
            b.requires = stock_note(id);
            out.push(b);
        }
        for id in units {
            let stock = w.stock_count(h, id);
            let Some(o) = w.objects.units.get(id) else { continue };
            let Some((x, y)) = parse_pos(&o.list("Buttonpos")) else { continue };
            let ok = has_shopper && stock > 0;
            let costs = if !has_shopper {
                [0; 4]
            } else if w.hero_token_pays(local, id) {
                [0, 0, 0, o.int("fused")]
            } else {
                [0, o.int("goldcost"), o.int("lumbercost"), o.int("fused")]
            };
            let mut b = Button::new(x, y, o.str("Art"), hotkey(&o.str("Hotkey")), Action::Hire(id), ok).with_tip(o.str("Tip"), fill_placeholders(w, &o.str("Ubertip")), costs);
            b.requires = stock_note(id);
            // Its requirements, at the hero level for heroes (REQUIRESTOOLTIP).
            if b.requires.is_empty() {
                b = b.with_requirements(db, w, local, id, w.requirement_level(local, id));
            }
            out.push(b);
        }
        // A tavern's Awaken: the player's first four heroes along the top row, the dead
        // ones to awaken ("Awakentip - name" with the cost), the others by name
        // (CAbilityAwaken::vf112, vf166).
        if w.awakens(h) {
            let gold = w.players[local as usize].get_state(1);
            let lumber = w.players[local as usize].get_state(2);
            for (i, hero) in w.awaken_heroes(local).into_iter().enumerate() {
                let Some(hu) = w.unit(hero) else { continue };
                let Some(o) = w.objects.units.get(hu.type_id) else { continue };
                let dead = hu.dead && !hu.reviving;
                let mut b = Button::new(i as u32, 0, o.str("Art"), None, Action::Awaken(hero), false);
                if dead {
                    let (g, l) = w.awaken_cost(hero);
                    b.enabled = has_shopper && gold >= g && lumber >= l;
                    b = b.with_tip(format!("{} - {}", o.str("Awakentip"), hu.proper_name), String::new(), [0, g, l, o.int("fused")]);
                } else {
                    b = b.with_tip(hu.proper_name.clone(), String::new(), [0; 4]);
                }
                out.push(b);
            }
        }
        out
    }

    /// An ability's buttons on a unit's card (two for Call to Arms), at its Buttonpos
    /// or at `at`.
    fn ability_buttons(&self, db: &w3_ui::FrameDb, w: &World, h: u32, a: Id, at: Option<(u32, u32)>) -> Vec<Button> {
        let _ = db;
        let mut out = Vec::new();
        let Some(u) = w.unit(h) else { return out };
        let Some(o) = w.objects.units.get(u.type_id) else { return out };
        let Some(ab) = w.objects.abilities.get(a) else { return out };
        // SetPlayerAbilityAvailable takes the button away.
        if w.players[u.owner as usize].abilities_unavailable.contains(&a) || !w.ability_visible(a) {
            return out;
        }
        // Root (Aroo) shows Uproot (Unbuttonpos, Unart, Unorder) while rooted, and a
        // morph its way back while in its other form (UnitID).
        const MORPHS: [&str; 9] = ["Abrf", "Arav", "Astn", "Abur", "AEme", "ANcr", "ANrg", "Acpf", "Aetf"];
        let code = ab.str("code");
        let in_other_form = MORPHS.contains(&code.as_str()) && id_from_str(ab.level_str("UnitID", w.ability_level(h, a).max(1)).trim()) == u.type_id;
        // A toggled ability that is on shows its off side (Unart, Untip, Unorder), if it
        // may be turned off.
        let toggled_on = matches!(code.as_str(), "Adef" | "AEim" | "AHds" | "Amfl" | "ANth" | "Amdf") && !ab.str("Unorder").is_empty() && w.toggle_active(h, a) && w.toggle_can_end(a);
        let unrooted_side = (code == "Aroo" && !u.extra.uprooted) || in_other_form || toggled_on;
        // Abilities with a button position appear on the card.
        let Some((x, y)) = at.or_else(|| parse_pos(&ab.list(if unrooted_side { "Unbuttonpos" } else { "Buttonpos" }))) else { return out };
        // Art may list one icon per race (Rally): human, orc, undead, night elf.
        let race_index = match o.str("race").to_ascii_lowercase().as_str() {
            "orc" => 1,
            "undead" => 2,
            "nightelf" => 3,
            _ => 0,
        };
        let pick = |key: &str| {
            let arts = ab.list(key);
            arts.get(race_index).or(arts.first()).map(|s| s.trim().to_string()).unwrap_or_default()
        };
        let art = pick(if unrooted_side { "Unart" } else { "Art" });
        if art.is_empty() {
            return out;
        }
        // The ability's order string, as one of the game's order names.
        let order = w3_game::natives::basic::order_name(w3_game::natives::basic::order_id(&w.order_string(a))).unwrap_or("");
        let targeting = w.targeting(a);
        let (order, targeted) = match ab.str("code").as_str() {
            "Ahar" | "Ahrl" => ("harvest", true),
            // Militia walk to their Town Hall themselves.
            "Amil" => (order, false),
            "Aroo" if unrooted_side => ("unroot", false),
            "Aroo" => ("root", true),
            c if MORPHS.contains(&c) || toggled_on => {
                let name = ab.str(if unrooted_side { "Unorder" } else { "Order" });
                (w3_game::natives::basic::order_name(w3_game::natives::basic::order_id(&name)).unwrap_or(""), false)
            }
            _ if targeting == w3_game::ability::Targeting::Passive => ("", false),
            _ => (order, targeting != w3_game::ability::Targeting::None),
        };
        let ready = toggled_on || order.is_empty() || w.can_cast(h, a).is_ok();
        let level = w.ability_level(h, a).max(1);
        let (tip_key, uber_key, hot_key) = if unrooted_side { ("Untip", "Unubertip", "Unhotkey") } else { ("Tip", "Ubertip", "Hotkey") };
        let tip = level_entry(&ab.list(tip_key), level);
        let uber = fill_placeholders(w, &level_entry(&ab.list(uber_key), level));
        let mana = ab.int(&format!("Cost{level}"));
        // An ability without an order string uses its code's built-in order; a spell
        // book opens its page.
        let action = match w.intrinsic_order_of(a) {
            _ if code == "Aspb" => Action::BookMenu(a),
            Some(id) if order.is_empty() && targeting != w3_game::ability::Targeting::Passive => Action::OrderId { order: id, targeted },
            _ => Action::Order { order, targeted },
        };
        let ready = toggled_on || ready && (order.is_empty() || w.can_cast(h, a).is_ok()) && (!matches!(action, Action::OrderId { .. }) || w.can_cast(h, a).is_ok());
        let mut button = Button::new(x, y, art, hotkey(&ab.str(hot_key)), action, ready).with_tip(tip, uber, [mana, 0, 0, 0]).with_requirements(db, w, u.owner, a, 1);
        // Autocast abilities toggle with their orderon and orderoff.
        let on = w3_game::natives::basic::order_name(w3_game::natives::basic::order_id(&ab.str("Orderon")));
        let off = w3_game::natives::basic::order_name(w3_game::natives::basic::order_id(&ab.str("Orderoff")));
        if let (Some(on), Some(off)) = (on, off) {
            button.autocast = Some((on, off, u.autocast.contains(&a)));
        }
        let left = u.cooldowns.get(&a).copied().unwrap_or(0.0);
        let total = w.data(a, "Cool", level as u32);
        if left > 0.0 && total > 0.0 {
            button.cooldown = Some((left, total));
        }
        out.push(button);
        // Call to Arms shows its second button (Unbuttonpos, Unart, Unorder: Back to
        // Work) beside it; other toggles show one button at a time.
        let both = ab.str("code") == "Amic";
        if let (true, Some((ux, uy)), unart) = (both, parse_pos(&ab.list("Unbuttonpos")), pick("Unart")) {
            let unorder = w3_game::natives::basic::order_name(w3_game::natives::basic::order_id(&ab.str("Unorder"))).unwrap_or("");
            if !unart.is_empty() && !unorder.is_empty() {
                let tip = level_entry(&ab.list("Untip"), level);
                let uber = fill_placeholders(w, &level_entry(&ab.list("Unubertip"), level));
                out.push(Button::new(ux, uy, unart, hotkey(&ab.str("Unhotkey")), Action::Order { order: unorder, targeted: false }, true).with_tip(tip, uber, [0; 4]));
            }
        }
        out
    }

    /// Buttons for a unit owned by the local player.
    pub fn card(&self, db: &w3_ui::FrameDb, w: &World, h: u32, page: Page) -> Vec<Button> {
        let mut out = self.card_buttons(db, w, h, page);
        // The current order's button is lit, Stop for an idle unit (as the original's
        // card shows: Stop on an idle Mortar Team, Attack Ground on one shelling).
        if let Some(u) = w.unit(h) {
            let current = if u.order.id != 0 { u.order.id } else { w3_game::natives::basic::order_id("stop") };
            for b in &mut out {
                let id = match b.action {
                    Action::Order { order, .. } if !order.is_empty() => w3_game::natives::basic::order_id(order),
                    Action::OrderId { order, .. } => order,
                    _ => 0,
                };
                b.active = id != 0 && id == current;
            }
        }
        out
    }

    fn card_buttons(&self, db: &w3_ui::FrameDb, w: &World, h: u32, page: Page) -> Vec<Button> {
        let Some(u) = w.unit(h) else { return vec![] };
        let Some(o) = w.objects.units.get(u.type_id) else { return vec![] };
        let mut out = Vec::new();
        let object_button = |id: Id, action: Action, enabled: bool| -> Option<Button> {
            let ob = w.objects.units.get(id).or_else(|| w.objects.upgrades.get(id)).or_else(|| w.objects.abilities.get(id))?;
            // What SetPlayerTechMaxAllowed rules out does not show.
            let next = w.players[u.owner as usize].tech_researched.get(&id).copied().unwrap_or(0) as u32 + 1;
            let revive = matches!(action, Action::Revive(_));
            if !revive && (w.tech_hidden(u.owner, id) || !w.tech_allowed(u.owner, id, next)) {
                return None;
            }
            let (x, y) = parse_pos(&ob.list("Buttonpos"))?;
            // Research shows the next level's tip, art, and cost.
            let (tip, uber, icon, costs) = if w.objects.upgrades.get(id).is_some() {
                let level = w.players[u.owner as usize].tech_researched.get(&id).copied().unwrap_or(0) as u32;
                let art = level_entry(&ob.list("Art"), level + 1);
                let costs = [0, ob.int("goldbase") + ob.int("goldmod") * level as i32, ob.int("lumberbase") + ob.int("lumbermod") * level as i32, 0];
                (level_entry(&ob.list("Tip"), level + 1), level_entry(&ob.list("Ubertip"), level + 1), art, costs)
            } else if w.hero_token_pays(u.owner, id) {
                // A hero token pays: no gold or lumber (FUN_6f402c30).
                (ob.str("Tip"), ob.str("Ubertip"), ob.str("Art"), [0, 0, 0, ob.int("fused")])
            } else {
                (ob.str("Tip"), ob.str("Ubertip"), ob.str("Art"), [0, ob.int("goldcost"), ob.int("lumbercost"), ob.int("fused")])
            };
            let level = if w.objects.upgrades.get(id).is_some() { w.players[u.owner as usize].tech_researched.get(&id).copied().unwrap_or(0) as u32 + 1 } else { w.requirement_level(u.owner, id) };
            Some(Button::new(x, y, icon, hotkey(&ob.str("Hotkey")), action, enabled).with_tip(tip, fill_placeholders(w, &uber), costs).with_requirements(db, w, u.owner, id, level))
        };
        if page == Page::Build {
            // Gold and lumber are checked when the button is used (Nogold, Nolumber).
            for b in o.list("Builds") {
                let id = id_from_str(b.trim());
                if let Some(btn) = object_button(id, Action::Build(id), true) {
                    out.push(btn);
                }
            }
            out.extend(self.command(db, "CmdCancel", Action::Cancel, true));
            return out;
        }
        if page == Page::Learn {
            for b in o.list("heroAbilList") {
                // What Engineering Upgrade made of the skill, when it has.
                let id = w.learned_id(h, id_from_str(b.trim()));
                let Some(ab) = w.objects.abilities.get(id) else { continue };
                let Some((x, y)) = parse_pos(&ab.list("Researchbuttonpos")) else { continue };
                let icon = ab.str("Researchart");
                let icon = if icon.is_empty() { ab.str("Art") } else { icon };
                // Learning shows the next level's Researchtip and Researchubertip.
                let next = w.ability_level(h, id) + 1;
                let tip = level_entry(&ab.list("Researchtip"), next).replace("%d", &next.to_string());
                let uber = fill_placeholders(w, &ab.str("Researchubertip"));
                let mut b = Button::new(x, y, icon, hotkey(&ab.str("Researchhotkey")), Action::Learn(id), w.can_learn(h, id)).with_tip(tip, uber, [0; 4]);
                // One it cannot learn yet lists its requirements and the hero level it
                // needs (CAbilityHero::vf166).
                if !w.learn_allowed(h, id) {
                    b = b.with_requirements(db, w, u.owner, id, 0);
                    if b.requires.is_empty() {
                        b.requires = db.string("REQUIRESTOOLTIP");
                    }
                    b.requires.push_str(&format!("\n - {} {}", db.string("REQUIREDLEVELTOOLTIP"), w.learn_level(h, id)));
                    b.enabled = false;
                }
                out.push(b);
            }
            out.extend(self.command(db, "CmdCancel", Action::Cancel, true));
            return out;
        }
        // A spell book's page: its spells in the order chosen from the top left, and
        // Cancel (CAbilitySpellBook::vf108, the slots at 0x6fce5dd0).
        if let Page::Book(book) = page {
            const SLOTS: [(u32, u32); 11] = [(0, 0), (1, 0), (2, 0), (3, 0), (0, 1), (1, 1), (2, 1), (3, 1), (0, 2), (1, 2), (2, 2)];
            let spells = u.extra.books.iter().find(|b| b.0 == book).map(|b| b.1.clone()).unwrap_or_default();
            for (&s, &at) in spells.iter().zip(SLOTS.iter()) {
                out.extend(self.ability_buttons(db, w, h, s, Some(at)));
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
            // An artillery weapon shells the ground too (as the original's Mortar Team
            // card shows).
            let artillery = w3_game::sim::unit_weapons(w, u).iter().any(|(_, wp)| wp.weapon_type == "artillery" || wp.weapon_type == "aline");
            if artillery {
                out.extend(self.command(db, "CmdAttackGround", Action::Order { order: "attackground", targeted: true }, true));
            }
        }
        // Every hero shows Hero Abilities, points or not (CAbilityHero::vf108); its
        // skills are dimmed while none can be learned.  The original leaves out units
        // flagged 0x40000000, which cannot gain levels or skills: illusions here; and a
        // hero with every skill at its top level and no points (measured: a level 10
        // Death Knight with all ten points spent shows none, one at level 10 with points
        // or at level 3 with none spent and more to learn shows it).
        if u.is_hero && u.extra.illusion.is_none() && !(u.skill_points == 0 && w.all_skills_learned(h)) {
            // Its unspent skill points in the corner (measured: a new Paladin's shows 1).
            let points = u.skill_points;
            out.extend(self.command(db, "CmdSelectSkill", Action::LearnMenu, true).into_iter().map(|mut b| {
                b.number = (points > 0).then_some(points);
                b
            }));
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
        // An uprooted ancient produces nothing until it roots again.
        let trains = if u.extra.uprooted { Vec::new() } else { o.list("Trains") };
        let researches = if u.extra.uprooted { Vec::new() } else { o.list("Researches") };
        let upgrades = if u.extra.uprooted { Vec::new() } else { o.list("Upgrade") };
        for t in trains {
            let id = id_from_str(t.trim());
            out.extend(object_button(id, Action::Train(id), true));
        }
        for r in researches {
            let id = id_from_str(r.trim());
            out.extend(object_button(id, Action::Research(id), true));
        }
        // A structure cannot upgrade while it produces.
        for r in upgrades {
            let id = id_from_str(r.trim());
            out.extend(object_button(id, Action::Upgrade(id), u.queue.is_empty()));
        }
        for &(a, _) in &u.abilities {
            // A spell book's spells are on its own page.
            if w.book_of(h, a).is_some() {
                continue;
            }
            out.extend(self.ability_buttons(db, w, h, a, None));
        }
        // An altar offers its owner's dead heroes, where their train buttons would be.
        for hero in w.revivable(h) {
            let Some(hu) = w.unit(hero) else { continue };
            let (g, l, _) = w.revive_cost(hero);
            let p = &w.players[u.owner as usize];
            let ok = p.get_state(1) >= g && p.get_state(2) >= l && u.revive.is_none();
            // Revivetip with the hero's revival cost.
            let tip = w.objects.units.get(hu.type_id).map(|o| o.str("Revivetip")).unwrap_or_default();
            out.extend(object_button(hu.type_id, Action::Revive(hero), ok).map(|b| {
                let uber = b.ubertip.clone();
                b.with_tip(tip, uber, [0, g, l, 0])
            }));
        }
        // Units that train units or revive heroes have rally (CUnit::vf107 adds
        // CAbilityRally), shown as CmdRally.
        let trains_units = o.list("Trains").iter().any(|t| w.objects.units.get(id_from_str(t.trim())).is_some());
        if trains_units {
            out.extend(self.command(db, "CmdRally", Action::Order { order: "setrally", targeted: true }, true));
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
    // Passive buttons' disabled forms are kept with the command buttons' (DISPASBTN).
    match lower.find("commandbuttons\\").map(|i| (i, "commandbuttons\\".len())).or_else(|| lower.find("passivebuttons\\").map(|i| (i, "passivebuttons\\".len()))) {
        Some((i, n)) => {
            let file = &icon[i + n..];
            format!("{}CommandButtonsDisabled\\DIS{file}", &icon[..i])
        }
        None => icon.to_string(),
    }
}
