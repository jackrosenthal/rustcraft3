//! A unit's own record (`+w3u`'s second, CUnit's `FUN_6f696490`), read against the quiet
//! test map's 71 units at 5 seconds: its agent flags, its handlers (who listens to it,
//! and for what), its widget, its triggers' events, and its 109 fields.

use w3_formats::w3z::{Field, First, Handlers, Ref, Second, Sprite, Timing, Widget};

use super::{rf, NONE};
use crate::types::Action;
use crate::world::World;
use crate::codes;

/// The events (less 0xd0000) an agent of a kind listens to on the unit it serves, as
/// it registers when made (read: the quiet test map's units' registries, and traced as
/// made at `FUN_6f0725b0`).  Provisional for kinds not read there: none.
fn listened(code: &[u8; 4]) -> &'static [u16] {
    match code {
        codes::ability::id::MOVE => &[0x144, 0x161, 0x166, 0x16b, 0x16c, 0x16d, 0x16e, 0x16f, 0x170, 0x171, 0x172, 0x173, 0x174, 0x175, 0x176, 0x177, 0x178, 0x17f, 0x18e, 0x193, 0x29d],
        codes::ability::id::ATTACK => &[0x144, 0x148, 0x149, 0x14a, 0x14b, 0x14c, 0x14d, 0x14e, 0x14f, 0x161, 0x163, 0x165, 0x166, 0x168, 0x169, 0x16a, 0x1a2, 0x1a5, 0x2a3, 0x2a4],
        codes::ability::id::CREEP_SLEEP => &[0x1a2, 0x1d5, 0x1d6],
        codes::ability::id::BUILD_HUMAN | codes::ability::id::BUILD_ORC | codes::ability::id::BUILD_NIGHT_ELF => &[0x144, 0x161, 0x179, 0x17a, 0x1a2, 0x29c, 0x2a5, 0x2a6],
        codes::ability::id::BUILD_UNDEAD => &[0x144, 0x161, 0x162, 0x179, 0x17a, 0x1a2, 0x29c, 0x2a5, 0x2a6],
        codes::ability::id::HARVEST => &[0x144, 0x150, 0x151, 0x152, 0x161, 0x166, 0x167, 0x182, 0x183, 0x184, 0x185, 0x186, 0x187, 0x191, 0x1c5],
        codes::ability::id::HARVEST_LUMBER => &[0x144, 0x150, 0x151, 0x152, 0x161, 0x166, 0x167, 0x184, 0x185, 0x186, 0x187, 0x1c5],
        codes::ability::id::ACOLYTE_HARVEST => &[0x144, 0x166, 0x182, 0x183],
        codes::ability::id::MILITIA => &[0x144, 0x188],
        // Spells cast on their own (autocast).
        codes::ability::id::REPAIR_ORC | codes::ability::id::ABOLISH_MAGIC | codes::ability::id::HEAL_CREEP_NORMAL | codes::ability::id::INNER_FIRE | codes::ability::id::RESTORATION | codes::ability::id::HEAL | codes::ability::id::RENEW => &[0x144, 0x148, 0x14c, 0x15f, 0x160, 0x161, 0x1a2, 0x2a3, 0x2a4],
        // Spells cast at a target, and one cast at none.
        codes::ability::id::THUNDER_BOLT_CREEP | codes::ability::id::CHIEFTAIN_SHOCK_WAVE | codes::ability::id::ENSNARE | codes::ability::id::CANNIBALIZE | codes::ability::id::SACRIFICE_ACOLYTE | codes::ability::id::UNSUMMON => &[0x144, 0x15f, 0x160, 0x161],
        codes::ability::id::THUNDER_CLAP_CREEP => &[0x144, 0x15f, 0x160],
        codes::ability::id::INVENTORY => &[0x144, 0x17c, 0x17d, 0x17e, 0x1a0, 0x1a2, 0x1a7, 0x299],
        codes::ability::id::REGENERATE_ON_BLIGHT => &[0x1a0, 0x1a2, 0x1a7],
        codes::ability::id::WANDER => &[0x148, 0x14c, 0x177, 0x178, 0x1a2],
        codes::ability::id::ALARM => &[0x1a2],
        codes::ability::id::ON_FIRE_HUMAN | codes::ability::id::ON_FIRE_UNDEAD | codes::ability::id::ON_FIRE_ORC | codes::ability::id::ON_FIRE_NIGHT_ELF | codes::ability::id::BLIGHTED_GOLD_MINE => &[0x1a0],
        codes::ability::id::INVULNERABLE => &[0x1a1, 0x1a7],
        codes::ability::id::MANA_REGENERATION_AURA | codes::ability::id::PALADIN_DEVOTION_AURA | codes::ability::id::ARCH_MAGE_BRILLIANCE_AURA | codes::ability::id::DEATH_KNIGHT_UNHOLY_AURA | codes::ability::id::CHIEFTAIN_ENDURANCE_AURA | codes::ability::id::PRIESTESS_TRUESHOT_AURA | codes::ability::id::DREADLORD_VAMPIRIC_AURA | codes::ability::id::KEEPER_THORNS_AURA | codes::ability::id::AURA_WAR_DRUMS | codes::ability::id::COMMAND_AURA | codes::ability::id::AURA_REGENERATION_WARD => &[0x1a0, 0x1a1, 0x1a2, 0x1a7],
        codes::ability::id::SELL_UNITS => &[0x1a2],
        codes::ability::id::NEUTRAL_BUILDING => &[0x1a5],
        codes::ability::id::QUEUE => &[0x1a0, 0x1a1, 0x1a2],
        codes::ability::id::UPGRADE_STRUCTURE => &[0x1a1, 0x1a2],
        _ => &[],
    }
}

/// The unit's own: its life, its orders, its sight (each made, CUnit's own).
const SELF_EVENTS: [u16; 18] = [0x145, 0x146, 0x147, 0x148, 0x14c, 0x153, 0x154, 0x155, 0x156, 0x157, 0x158, 0x159, 0x15a, 0x162, 0x164, 0x19d, 0x1a1, 0x1d4];

/// A buff's on its bearer, and on the aura's bearer that laid it.
const BUFF_EVENTS: [u16; 3] = [0x1a1, 0x1a2, 0x1a7];

/// The handler a listener of an ability calls, by the ability's code: an aura's and the
/// alarm's on units in range, Ensnare's, a shop's and an inventory's.
fn pil_handler(code: &[u8; 4]) -> u32 {
    match code {
        codes::ability::id::ENSNARE | codes::ability::id::ARCH_MAGE_BLIZZARD | codes::ability::id::PIT_LORD_RAIN_OF_FIRE => 0xd_01bf,
        codes::ability::id::NEUTRAL_BUILDING | codes::ability::id::INVENTORY => 0xd_01aa,
        codes::ability::id::ALARM => 0xd_01c8,
        // (A flyer's burrow detection's, its look: read, Forestwalk's whelps'.)
        codes::ability::id::BURROW_DETECTION_FLYERS => 0xd_01b4,
        _ => 0xd_01c7,
    }
}

/// The armor types in the engine's order (`defType`).
const DEFENSE_TYPES: [&str; 8] = ["small", "medium", "large", "fort", "normal", "hero", "divine", "none"];

impl World {
    /// A model's stands: its sequences named "Stand" and a variation's number alone
    /// (read: a Forest Troll's "Stand - 1" to "- 3", a ghoul's "Stand -2", "Stand", and
    /// "Stand -3", not "Stand Ready" nor "Stand Lumber"), by index.
    fn stand_sequences(&self, model: &str) -> Vec<u32> {
        self.named_sequences(model, "stand")
    }

    /// A unit's stands as its sprite holds them: its model's plain stands; else those
    /// with the fewest more words, all of them its animation properties or the tags a
    /// stand may carry (read: a Guard Tower's 'Stand Upgrade First Ready Attack', a
    /// Sea Giant's 'Stand Ready', a Tree of Life's 'Stand Alternate Upgrade First
    /// Second', a spider crab's 'Stand - Var1' to 'Var3', a Chaos Kodo Beast's 'Stand
    /// Channel').  None for a model without a
    /// stand (read: a Locust's).
    fn unit_stands(&self, model: &str, o: &crate::objects::Object) -> Vec<u32> {
        let plain = self.stand_sequences(model);
        if !plain.is_empty() {
            return plain;
        }
        let Ok(data) = self.chain.read(model) else { return Vec::new() };
        let Ok(m) = w3_formats::mdx::Model::parse(&data) else { return Vec::new() };
        let props: Vec<String> = o.str("animProps").split(',').map(|s| s.trim().to_ascii_lowercase()).filter(|s| !s.is_empty()).collect();
        // (A Kodo Beast's 'Stand Channel - 1' and '- 3', as a Chaos Kodo's read.)
        const TAGS: [&str; 8] = ["ready", "attack", "alternate", "upgrade", "first", "second", "third", "channel"];
        let mut best: Option<(usize, Vec<u32>)> = None;
        for (k, s) in m.sequences.iter().enumerate() {
            let words: Vec<String> = s
                .name
                .split(|c: char| c.is_whitespace() || c == '-')
                .map(|w| w.to_ascii_lowercase())
                .filter(|w| !w.is_empty() && !w.chars().all(|c| c.is_ascii_digit()) && !(w.starts_with("var") && w[3..].chars().all(|c| c.is_ascii_digit())))
                .collect();
            if !words.iter().any(|w| w == "stand") {
                continue;
            }
            let extra: Vec<&String> = words.iter().filter(|w| *w != "stand").collect();
            if !extra.iter().all(|w| props.contains(w) || TAGS.contains(&w.as_str())) || !props.iter().all(|p| words.contains(p)) {
                continue;
            }
            match &mut best {
                Some((n, v)) if *n == extra.len() => v.push(k as u32),
                Some((n, _)) if *n < extra.len() => {}
                _ => best = Some((extra.len(), vec![k as u32])),
            }
        }
        best.map_or(Vec::new(), |b| b.1)
    }

    /// A model's sequences named by a phrase (numbers and dashes aside), by index.
    pub(crate) fn named_sequences_phrase(&self, model: &str, phrase: &str) -> Vec<u32> {
        let Ok(data) = self.chain.read(model) else { return Vec::new() };
        let Ok(m) = w3_formats::mdx::Model::parse(&data) else { return Vec::new() };
        let want: Vec<String> = phrase.split_whitespace().map(|w| w.to_ascii_lowercase()).collect();
        m.sequences
            .iter()
            .enumerate()
            .filter(|(_, s)| s.name.split(|c: char| c.is_whitespace() || c == '-').filter(|w| !w.is_empty() && !w.chars().all(|c| c.is_ascii_digit())).map(|w| w.to_ascii_lowercase()).collect::<Vec<_>>() == want)
            .map(|(k, _)| k as u32)
            .collect()
    }

    /// A model's sequences named only so (numbers and dashes aside), by index.
    pub(crate) fn named_sequences(&self, model: &str, name: &str) -> Vec<u32> {
        // (A model named .mdl is read from its .mdx.)
        let model = match model.len().checked_sub(4).filter(|&k| model[k..].eq_ignore_ascii_case(".mdl")) {
            Some(k) => format!("{}.mdx", &model[..k]),
            None => model.to_string(),
        };
        let Ok(data) = self.chain.read(&model) else { return Vec::new() };
        let Ok(m) = w3_formats::mdx::Model::parse(&data) else { return Vec::new() };
        m.sequences
            .iter()
            .enumerate()
            .filter(|(_, s)| {
                let words: Vec<&str> = s.name.split(|c: char| c.is_whitespace() || c == '-').filter(|w| !w.is_empty() && !w.chars().all(|c| c.is_ascii_digit())).collect();
                words.len() == 1 && words[0].eq_ignore_ascii_case(name)
            })
            .map(|(k, _)| k as u32)
            .collect()
    }

    /// The unit's record as the port writes it, by its slot (provisional throughout what
    /// the local player sees: its sprite's animation, tint word, and flags, its
    /// widget's shorts).
    pub fn unit_own_record(&self, h: u32, rallies: &[(Ref, Ref)]) -> Option<(u32, Second)> {
        let u = self.unit(h)?;
        let o = self.objects.units.get(u.type_id)?;
        let first = &u.extra.object_slots[0];
        if first.len() < 10 {
            return None;
        }
        let me = first[0];
        let code_of = |s: u32| self.tags.live.get(&s).map(|e| e.0);
        let building = u.is_building;
        let hidden = u.hidden;
        let invulnerable = u.abilities.iter().any(|(a, _)| a == codes::ability::id::INVULNERABLE);
        let computer = self.players.get(u.owner as usize).is_some_and(|p| p.controller == crate::types::map_control::COMPUTER && p.slot_state == crate::types::slot_state::PLAYING);
        let creep = u.owner == crate::types::PLAYER_NEUTRAL_AGGRESSIVE;
        let types: Vec<String> = o.list("type").iter().map(|t| t.trim().to_ascii_lowercase()).collect();
        let is = |t: &str| types.iter().any(|x| x == t);

        // The agent's flags (+0x20), 0x200 not while it has an order.
        let order = self.saved_order(h);
        // (A worker in a gold mine both: read, 0x7, its game handler kept.)
        let in_gold_mine = order.as_ref().is_some_and(|o| o.in_gold_mine);
        let mut own = 0x600 | if in_gold_mine { 0x7 } else if hidden { 0x5 } else { 0x6 };
        if order.is_some() {
            own &= !0x200;
        }
        // (Under a move order 0x20000000: read, a peasant sent to a point.)
        if order.as_ref().is_some_and(|o| o.state.kind == 7) {
            own |= 0x2000_0000;
        }
        if !building {
            own |= 0x40;
        }
        if invulnerable {
            own |= 0x8;
        }
        // (A unit whose collision is an odd count of 16: read, every unit of the quiet
        // test map and of Forestwalk, those of 16, 31, and 48 and not 32 or 47;
        // provisional: what it means, likely its pathing's alignment.)
        if !building && (o.real("collision") / 16.0) as i32 & 1 == 1 {
            own |= 0x1000;
        }
        // What it does: a worker (0x100000), what it harvests (gold 0x8000, a haunted
        // mine's 0x10000, lumber 0x40000), it trains (0x200000).
        if is("peon") {
            own |= 0x10_0000;
        }
        for (a, _) in &u.abilities {
            match self.objects.abilities.get(*a).map(|ab| ab.str("code").trim().to_string()).as_deref() {
                Some(codes::ability::HARVEST) => own |= 0x4_8000,
                Some(codes::ability::HARVEST_LUMBER) => own |= 0x4_0000,
                Some(codes::ability::ACOLYTE_HARVEST) => own |= 0x1_0000,
                _ => {}
            }
        }

        // Who listens to it, each with its stamp: as made, in the order made.
        // (Its held items' abilities among them.)
        let held = self.held_item_objects(h);
        let mut abilities: Vec<(u32, u32, [u8; 4])> = u.extra.object_slots[1].iter().chain(&u.extra.object_slots[2]).filter_map(|&(s, b)| code_of(s).filter(|c| c[0] == b'A' && c != codes::class::UNIT_AI).map(|c| (s, b, c))).collect();
        abilities.extend(held.iter().filter(|x| x.1[0] == b'A').map(|x| (x.0 .0, x.0 .1, x.1)));
        abilities.sort_by_key(|a| a.1);
        let mut agents: Vec<(Ref, (u32, u32), Vec<u16>)> = vec![(rf(me), (me.1, 0), SELF_EVENTS.to_vec())];
        for &(s, b, c) in &abilities {
            let mut events = listened(&c).to_vec();
            // (A computer's structure's alarm listens for its owner's AI too, read: the
            // undead's Necropolis and Haunted Gold Mine.)
            if &c == codes::ability::id::ALARM && computer {
                events.push(0x1a5);
            }
            // (A haunted mine's listens as it starts to serve, after its AIs.)
            if !events.is_empty() && &c != codes::ability::id::BLIGHTED_GOLD_MINE {
                agents.push((Ref(s, b), (b, 0), events));
            }
        }
        let made = abilities.iter().map(|a| a.1).max().unwrap_or(me.1);
        if let Some(a) = abilities.iter().find(|a| &a.2 == codes::ability::id::BLIGHTED_GOLD_MINE) {
            agents.push((Ref(a.0, a.1), (made, 9), listened(codes::ability::id::BLIGHTED_GOLD_MINE).to_vec()));
        }
        if abilities.iter().any(|a| &a.2 == codes::ability::id::QUEUE) {
            own |= 0x20_0000;
        }
        // A structure's own again after its abilities.
        if building {
            agents.push((rf(me), (made, 1), vec![0x15e]));
        }
        // Its rally point (a second table's agent), with its abilities.
        if let Some(&(r, _)) = rallies.iter().find(|(_, owner)| *owner == rf(me)) {
            let at = abilities.iter().find(|a| &a.2 == codes::ability::id::UPGRADE_STRUCTURE).map_or(made, |a| a.1);
            agents.push((r, (at, 1), vec![0x1a0, 0x1a2]));
        }
        // The AIs that watch it, as it joins them.
        let player_ai = |p: u8| {
            let computers: Vec<u8> = (0..crate::types::PLAYER_NEUTRAL_AGGRESSIVE as u8).filter(|&q| self.players.get(q as usize).is_some_and(|pl| pl.controller == crate::types::map_control::COMPUTER && pl.slot_state == crate::types::slot_state::PLAYING)).collect();
            let k = if p == crate::types::PLAYER_NEUTRAL_AGGRESSIVE { Some(computers.len()) } else { computers.iter().position(|&q| q == p) };
            k.and_then(|k| self.tags.player_ais.get(k)).copied()
        };
        // A gold mine's town site (its own, or for a haunted mine the one it replaced)
        // and each computer's AI; a computer's town hall's site after its AI.
        let mine = u.abilities.iter().any(|(a, _)| matches!(a, codes::ability::id::GOLD_MINE | codes::ability::id::BLIGHTED_GOLD_MINE | codes::ability::id::ENTANGLED_GOLD_MINE));
        let nearest_site = || self.tags.sites.iter().min_by(|a, b| (a.0 - u.x).hypot(a.1 - u.y).total_cmp(&(b.0 - u.x).hypot(b.1 - u.y))).map(|s| s.2);
        if mine {
            let site = u.extra.site_ai.or_else(|| if u.abilities.iter().any(|(a, _)| a == codes::ability::id::BLIGHTED_GOLD_MINE) { nearest_site() } else { None });
            if let Some(site) = site {
                agents.push((rf(site), (made, 2), vec![0x1a0, 0x1a4]));
            }
            if !computer {
                for k in 0..self.tags.player_ais.len().saturating_sub(1) {
                    agents.push((rf(self.tags.player_ais[k]), (made, 3 + k as u32), vec![0x1a0]));
                }
            }
        }
        if computer && is("townhall") {
            if let Some(site) = nearest_site() {
                agents.push((rf(site), (made, 4), vec![0x1a0]));
            }
        }
        if creep && !building {
            if let Some(c) = u.extra.camp_ai {
                agents.push((rf(c), (made, 2), vec![0x1a0, 0x1a2]));
            }
        }
        if (creep && !building) || computer {
            if let Some(a) = player_ai(u.owner) {
                agents.push((rf(a), (made, 3), vec![0x1a0, 0x1a2]));
            }
        }
        // Buffs on it (an aura's on its bearer, made with it), and those its auras laid
        // on others.
        let mut buffs: Vec<(u32, u32)> = u.extra.object_slots[2].iter().copied().filter(|&(s, _)| code_of(s).is_some_and(|c| c[0] == b'B')).collect();
        for b in u.buffs.iter().filter(|b| self.buff_kept(b)) {
            let t = self.tagged(b.slot);
            if !buffs.contains(&t) {
                buffs.push(t);
            }
        }
        for t in buffs {
            // (Devotion Aura's on another listens to it not for 0x1a2: read, the ogres'.)
            let events = if code_of(t.0) == Some(*codes::buff::id::DEVOTION_AURA) && !u.extra.object_slots[2].contains(&t) { vec![0x1a1, 0x1a7] } else { BUFF_EVENTS.to_vec() };
            agents.push((rf(t), (t.1, 0), events));
        }
        for &v in &self.unit_order {
            if v == h {
                continue;
            }
            for b in self.unit(v).into_iter().flat_map(|w| w.buffs.iter()).filter(|b| self.buff_kept(b) && b.caster == h) {
                let t = self.tagged(b.slot);
                agents.push((rf(t), (t.1, 0), vec![0x1a2]));
            }
        }
        // Woken from creep sleep, its abilities with orders listen anew, the last made
        // first, all but for 0x166.
        if let Some(w) = u.extra.woke_at {
            let mut woken: Vec<(Ref, (u32, u32), Vec<u16>)> = Vec::new();
            for a in agents.iter_mut().rev() {
                let ability = abilities.iter().any(|x| Ref(x.0, x.1) == a.0);
                if ability && a.2.contains(&0x144) {
                    let again: Vec<u16> = a.2.iter().copied().filter(|&e| e != 0x166).collect();
                    a.2.retain(|&e| e == 0x166);
                    woken.push((a.0, (w, woken.len() as u32), again));
                }
            }
            agents.extend(woken);
        }
        // A worker walking home to it with its load: the worker's movement listens for
        // 0x1a2 and 0x1a4, as its walk began (read: Forestwalk's town hall at 12
        // seconds).
        for &v in &self.unit_order {
            let Some(w) = self.unit(v) else { continue };
            if !w.extra.gold_run.as_ref().is_some_and(|r| matches!(r.phase, crate::goldrun::Phase::ToHall(hall) if hall == h)) {
                continue;
            }
            let Some((clu, _, _)) = self.saved_order(v).and_then(|o| o.walk) else { continue };
            let amov = w.extra.object_slots.iter().flatten().copied().find(|&(s, b)| self.tags.live.get(&s).is_some_and(|e| &e.0 == codes::ability::id::MOVE && e.1 == b));
            if let Some(a) = amov {
                agents.push((rf(a), (clu.1, 0), vec![0x1a2, 0x1a4]));
            }
        }
        // At work in a haunted mine, its harvest listens for 0x1a2, as the work began.
        if let Some(o) = order.as_ref().filter(|o| o.mine) {
            if let Some(a) = abilities.iter().find(|a| &a.2 == codes::ability::id::ACOLYTE_HARVEST) {
                agents.push((Ref(a.0, a.1), (o.order.1, 0), vec![0x1a2]));
            }
        }
        agents.sort_by_key(|a| a.1);
        let mut events = vec![Vec::new(); 0x400];
        for (r, _, es) in &agents {
            for &e in es {
                events[e as usize].push((*r, 0xd_0000 + e as u32));
            }
        }
        let mut fixed: [Vec<(Ref, u32)>; 5] = Default::default();
        if !hidden || in_gold_mine {
            fixed[0].push((rf(self.game_object()), 0x8_0267));
        }
        if let Some(a) = abilities.iter().find(|a| &a.2 == codes::ability::id::MOVE) {
            // (Walking, its walk's events too: read, Forestwalk's peasants on their way.)
            if order.as_ref().is_some_and(|o| o.walking) {
                fixed[1].push((Ref(a.0, a.1), 0xd_0196));
                fixed[2].push((Ref(a.0, a.1), 0xd_0198));
            }
            fixed[3].push((Ref(a.0, a.1), 0xd_0199));
            fixed[4].push((Ref(a.0, a.1), 0xd_019a));
        }
        // Its listeners' observers, keyed by the listener: a hash of four buckets by
        // slot, each in the order made.
        let ability_ref = |c: &[u8; 4]| abilities.iter().find(|a| &a.2 == c).map(|a| Ref(a.0, a.1));
        let mut keyed: Vec<((u32, u32), (Ref, u32))> = Vec::new();
        if let Some(&d) = u.extra.object_slots[1].iter().find(|&&(s, _)| code_of(s) == Some(*codes::class::THRESHOLD_LISTENER)) {
            keyed.push((d, (rf(me), 0xd_019f)));
        }
        for &(s, c) in &u.extra.spell_owners {
            if code_of(s) != Some(*codes::class::THRESHOLD_LISTENER) {
                continue;
            }
            let rising = u.extra.mana_listeners.iter().any(|&(t, x)| x == s && t.is_finite());
            if let Some(a) = ability_ref(&c) {
                keyed.push((self.tagged(s), (a, if rising { 0xd_01b6 } else { 0xd_01b7 })));
            }
        }
        // (A held item's ability's listener on mana, its ability's.)
        for (k, x) in held.iter().enumerate() {
            if &x.1 == codes::class::THRESHOLD_LISTENER {
                if let Some(a) = held[..k].iter().rev().find(|y| y.2 == x.2 && y.1[0] == b'A') {
                    keyed.push((x.0, (rf(a.0), 0xd_01b7)));
                }
            }
        }
        let mut pils: Vec<(u32, [u8; 4])> = Vec::new();
        let rest = &u.extra.object_slots[2];
        // (Not a kept slot another object has taken since: read, a flyer's making's,
        // let go as it slept.)
        for (k, &(s, b)) in rest.iter().enumerate() {
            if self.tagged(s).1 == b && code_of(s) == Some(*codes::class::IN_RANGE_LISTENER) {
                if let Some(c) = rest[..k].iter().rev().filter(|&&(x, b)| self.tagged(x).1 == b).filter_map(|&(x, _)| code_of(x)).find(|c| c[0] == b'A') {
                    pils.push((s, c));
                }
            }
        }
        pils.extend(u.extra.alarm_pil.map(|s| (s, *codes::ability::id::ALARM)));
        pils.extend(u.extra.ensnare_pil.map(|p| (p.1, *codes::ability::id::ENSNARE)));
        pils.extend(u.extra.item_pil.map(|p| (p.0, *codes::ability::id::INVENTORY)));
        pils.extend(u.extra.spell_pils.iter().copied());
        for (s, c) in pils {
            if code_of(s) == Some(*codes::class::IN_RANGE_LISTENER) {
                if let Some(a) = ability_ref(&c) {
                    keyed.push((self.tagged(s), (a, pil_handler(&c))));
                }
            }
        }
        keyed.sort_by_key(|k| (k.0 .0 & 3, k.0 .1));
        let handlers = Handlers { keyed: keyed.into_iter().map(|(l, x)| (l.0, vec![x])).collect(), fixed, events };

        // Its widget: its sprite, owner, type, what the local player sees, and a
        // structure's footprint and place.
        let tint = 0xff00_0000 | (o.int("red").clamp(0, 255) as u32) << 16 | (o.int("green").clamp(0, 255) as u32) << 8 | o.int("blue").clamp(0, 255) as u32;
        // (Its animation as made: the first of its model's stands, from its start;
        // provisional, the running game's random choice and time.)
        // (Its model's _V1 version where its file's flags have 2: read, a Sorceress's
        // and a Lich's.)
        let model = if o.int("fileVerFlags") & 2 != 0 { format!("{}_V1.mdx", o.str("file")) } else { format!("{}.mdx", o.str("file")) };
        let stands = self.unit_stands(&model, o);
        let seen = u.owner == self.local_player || self.unit_visible_to(h, self.local_player);
        // (At work in a haunted mine, its Stand Work Gold: read, an acolyte's sequence 4
        // at 0x400, its current one 0xfffe.)
        let working = order.as_ref().filter(|o| o.mine || o.summoning).map(|_| self.named_sequences_phrase(&model, "stand work gold"));
        let summoning = order.as_ref().is_some_and(|o| o.summoning);
        let sprite = Sprite {
            // (The local player's 0x200000 more; provisional: others' too at times, read
            // of far creeps and a gold mine seen, not yet told apart.)
            real: f32::from_bits(0x0344_0000 | if building { 0x0008_0008 } else { 0 } | if u.owner == self.local_player { 0x0020_0000 } else { 0 }),
            short: if working.is_some() { 0xfffe } else { stands.first().map_or(0xffff, |&k| k as u16) },
            byte: 255,
            word: tint,
            reals: [o.real("modelScale"), 1.0],
            words: [4, 0],
            last_real: f32::from_bits(150),
            last_byte: 1,
            // (A summoner's flags 2, 1, 0 and its work from 0: read, the acolyte
            // summoning a Crypt.)
            flags: if summoning { [2, 1, 0] } else { [1, 1, 1] },
            // (A model without a stand one entry, empty.)
            entries: if stands.is_empty() && working.is_none() { vec![(0.0, 4, Vec::new())] } else { vec![(0.0, 4, stands.clone()), working.map_or((0.0, 4, stands), |seq| (if summoning { 0.0 } else { f32::from_bits(0x400) }, 4, seq))] },
            model,
        };
        let footprint: Vec<Ref> = u.extra.footprint_slots.iter().map(|&s| rf(self.tagged(s))).collect();
        let widget = Widget {
            sprite: Some(sprite),
            owner: u.owner as u32,
            word: u32::from_be_bytes(u.type_id),
            // (The local player's own 1; 0x8001 where it does not see it, else 0: read,
            // Forestwalk's human hall and peasants, 1, 0, and their gold mine, 4095, 0.)
            shorts: [if u.owner == self.local_player { 1 } else if building && u.owner as usize >= crate::types::PLAYER_NEUTRAL_AGGRESSIVE as usize { 4095 } else { 0 }, if seen { 0 } else { 0x8001 }],
            refs: (!footprint.is_empty()).then(|| (footprint, [u.x, u.y])),
            bytes: [0, 0],
            last_real: o.real("occH"),
            byte: 0,
        };

        // Its triggers' events on it: 0x80234 to 0x80258, then 0x80319 to 0x80329.
        let mut lists = vec![Vec::new(); 37 + 17];
        for id in self.handles.ids() {
            let Some(crate::types::Obj::Event(r)) = self.handles.get(id) else { continue };
            let crate::types::EventKind::Unit { unit, event } = r.kind else { continue };
            if unit != h {
                continue;
            }
            let Some(&own_slot) = self.tags.handle_ids.get(&id) else { continue };
            let e = event as u32 + 0x8_0200;
            let k = match e {
                0x8_0234..=0x8_0258 => e - 0x8_0234,
                0x8_0319..=0x8_0329 => 37 + e - 0x8_0319,
                _ => continue,
            };
            lists[k as usize].push((rf(own_slot), e));
        }

        // Its fields.
        let w = Field::Word;
        let none3 = || [w(u32::MAX), w(u32::MAX), w(0)];
        let slot_ref = |s: Option<(u32, u32)>| s.map_or(NONE, rf);
        let (life_rate, mana_rate) = self.regen_rates(h).unwrap_or((0.0, 0.0));
        let rcm: Vec<(u32, u32)> = u.extra.object_slots[1].iter().copied().filter(|&(s, _)| code_of(s) == Some(*codes::class::CONTINUOUS_MODIFIER)).collect();
        let death = u.extra.object_slots[1].iter().copied().find(|&(s, _)| code_of(s) == Some(*codes::class::THRESHOLD_LISTENER));
        // The newest of its abilities and buffs (its abilities' chain's head, the AI's
        // unit list aside).
        let newest = abilities.iter().map(|a| (a.0, a.1)).chain(u.buffs.iter().filter(|b| self.buff_kept(b)).map(|b| self.tagged(b.slot))).max_by_key(|a| a.1);
        let ai_unit = u.extra.ai_slots.iter().copied().find(|&(s, _)| code_of(s) == Some(*codes::class::UNIT_AI));
        let mut kinds = 0u32;
        for (bit, name) in [(2, "undead"), (8, "mechanical"), (16, "peon"), (32, "sapper"), (64, "townhall"), (128, "ancient"), (256, "neutral")] {
            if is(name) {
                kinds |= bit;
            }
        }
        let targets = if building { 8 } else { 2 };
        let defense = DEFENSE_TYPES.iter().position(|&t| t.eq_ignore_ascii_case(o.str("defType").trim())).unwrap_or(1) as u32;
        // Its flags (+0x5c, as CUnit's `FUN_6f68a060` sets them): its corpse's (its type's
        // deathType: 0x1000 none raised, 0x200 it decays), an AI's (0x4), hidden (0x8),
        // a structure (0x10000), a flyer (0x20000000), a gold mine (0x20000), a neutral
        // building of Neutral Passive's (0x40000), a critter of its (0x80000: those that
        // wander, provisional), and what it takes back (Return Resources' DataA, lumber
        // 0x4000, and DataB, gold 0x8000; `FUN_6f517990`).
        let death_type = o.int("deathType");
        // (0x1 none while it has an order.)
        let mut flags = if order.is_some() { 0 } else { 0x1 };
        if death_type & 2 != 0 {
            flags |= 0x200;
        }
        if death_type & 1 == 0 {
            flags |= 0x1000;
        }
        if computer || creep {
            flags |= 0x4;
        }
        if hidden {
            flags |= 0x8;
        }
        if building {
            flags |= 0x1_0000;
        }
        if o.str("movetp").trim().eq_ignore_ascii_case("fly") {
            flags |= 0x2000_0000;
        }
        if u.abilities.iter().any(|(a, _)| matches!(a, codes::ability::id::GOLD_MINE | codes::ability::id::BLIGHTED_GOLD_MINE | codes::ability::id::ENTANGLED_GOLD_MINE)) {
            flags |= 0x2_0000;
        }
        let passive = u.owner as usize == crate::types::PLAYER_NEUTRAL_AGGRESSIVE as usize + 3;
        if passive && is("neutral") {
            flags |= 0x4_0000;
        }
        if passive && u.abilities.iter().any(|(a, _)| a == codes::ability::id::WANDER) {
            flags |= 0x8_0000;
        }
        for (a, _) in &u.abilities {
            let Some(ab) = self.objects.abilities.get(*a) else { continue };
            if ab.str("code").trim() == codes::ability::RETURN {
                if ab.level_real("DataA", 1) != 0.0 {
                    flags |= 0x4000;
                }
                if ab.level_real("DataB", 1) != 0.0 {
                    flags |= 0x8000;
                }
            }
        }
        // (Its walk's speed for its animation, a structure's 200.)
        let speed = if o.real("walk") > 0.0 { o.real("walk") } else { 200.0 };
        // Its detection's record (`Cdet`) and its shared vision's (`Csha`), once made.
        let detection = u.extra.detected_by.first().map(|d| self.tagged(d.1)).filter(|t| self.tags.live.get(&t.0).is_some_and(|e| &e.0 == codes::class::DETECTION));
        let vision = u.extra.vision_record.filter(|&(s, b)| self.tagged(s).1 == b);
        let two = |t: Option<(u32, u32)>| t.map_or([w(u32::MAX), w(u32::MAX)], |t| [w(t.0), w(t.1)]);
        let mut fields = vec![Field::Timer(NONE, 0, Timing(None))];
        fields.extend(two(detection));
        fields.push(w(0));
        fields.extend(two(vision));
        fields.extend([w(0), w(0)]);
        fields.push(Field::Ref(slot_ref(u.extra.day_ref)));
        // (Its sight's radius in the fog's cells of 128, rounded (`FUN_6f24f7e0` of
        // `FUN_6f699ef0`), a computer's or neutral's no farther than it acquires
        // targets: read, every type of the quiet test map's.)
        let night = self.night_sight;
        let mut radius = if self.ultravision(u) { o.real("sight") } else { o.real("sight") + (o.real("nsight") - o.real("sight")) * night };
        let acquire = o.real("acquire");
        if (computer || u.owner >= crate::types::PLAYER_NEUTRAL_AGGRESSIVE) && acquire.abs() >= 0.001 {
            radius = radius.min(acquire);
        }
        fields.push(Field::Short((radius / 128.0 + 0.5) as u16));
        // Its current task, timer, order, and the order after (`saved_order`).
        let three = |t: (u32, u32)| [w(t.0), w(t.1), w(0)];
        fields.extend(order.as_ref().map_or_else(none3, |o| three(o.task)));
        fields.push(order.as_ref().and_then(|o| o.timer.clone()).unwrap_or(Field::Timer(NONE, 0, Timing(None))));
        fields.extend(order.as_ref().map_or_else(none3, |o| three(o.order)));
        fields.extend(order.as_ref().map_or_else(none3, |o| three(o.next)));
        fields.extend(newest.map_or_else(none3, |n| [w(n.0), w(n.1), w(0)]));
        for _ in 0..5 {
            fields.extend(none3());
        }
        fields.extend(ai_unit.map_or_else(none3, |a| [w(a.0), w(a.1), w(0)]));
        fields.push(Field::Ref(slot_ref(death)));
        fields.push(Field::Real(life_rate));
        // (On blight, its blight's.)
        let life_rcm = u.extra.blight_rcm.map(|s| self.tagged(s)).or(rcm.first().copied());
        fields.push(Field::Ref(slot_ref(life_rcm)));
        fields.push(Field::Real(mana_rate));
        fields.push(Field::Ref(slot_ref(rcm.get(1).copied().filter(|_| u.max_mana > 0.0))));
        // (Its armor with its buffs': read, Devotion Aura's on the ogres.)
        fields.push(Field::Real(o.real("def") + self.buff_mods(h).2));
        fields.push(Field::Real(0.0));
        fields.extend([w(u.owner as u32), w(flags), w(0x0f12), w(invulnerable as u32)]);
        // (Its order's state words: `SavedOrder::state`.)
        let st = order.as_ref().map(|o| o.state).unwrap_or_default();
        fields.extend((52..66).map(|k| w(if k == 57 { st.inside_word } else { 0 })));
        fields.push(w(st.kind));
        fields.extend([w(!building as u32), w(0)]);
        fields.extend([Field::Real(0.0), Field::Real(0.0), Field::Real(0.0)]);
        fields.extend([w(kinds), w(targets), w(defense), w(u32::MAX), w(u32::MAX)]);
        fields.extend([Field::Ref(NONE), Field::Ref(NONE), Field::Ref(NONE)]);
        fields.extend([Field::Real(o.real("impactZ")), Field::Real(o.real("launchX")), Field::Real(o.real("launchY")), Field::Real(o.real("launchZ"))]);
        fields.extend([Field::Byte(0), Field::Byte(st.walking as u8), w(0), w(0), w(0), w(0), Field::Ref(NONE), Field::Byte(0)]);
        fields.extend([w(tint), w(st.after_tint[0]), w(st.after_tint[1]), w(st.after_tint[2]), w(st.after_tint[3]), w(0), w(0), w(0)]);
        // (A spell immune unit's two words (+0x15c, +0x160: read, the granite golems' and a
        // red dragon's), its being built on another (isBuildOn, +0x60), and a flag word
        // (+0x280: 0x10 for all read, 0x8 for a type with elevation points).)
        let immune = u.abilities.iter().any(|(a, _)| self.objects.abilities.get(*a).is_some_and(|ab| ab.str("code").trim() == codes::ability::MAGIC_IMMUNITY));
        let built_on = if o.int("isBuildOn") != 0 { 0x100 } else { 0 };
        let elevated = if o.int("elevPts") > 0 { 8 } else { 0 };
        fields.extend([w(speed.to_bits()), w(speed.to_bits()), w(1f32.to_bits()), w(0), w(immune as u32), w(immune as u32), w(built_on), w(0), w(0x10 | elevated | if st.inside { 0x80 } else { 0 })]);
        Some((me.0, Second::Unit { own, handlers, widget, lists, fields }))
    }
}

impl World {
    /// The unit's ability chain: its abilities, buffs, and AI's unit list (`uref`), in
    /// the order made (by birth), each with its kind (read: a creep's Amov, Aatk, an
    /// aura and its own buff, ACsp, then its uref; a peasant's AHbu first).
    pub(crate) fn ability_chain(&self, h: u32, rallies: &[(Ref, Ref)]) -> Vec<((u32, u32), [u8; 4])> {
        let Some(u) = self.unit(h) else { return Vec::new() };
        let code_of = |s: u32| self.tags.live.get(&s).map(|e| e.0);
        let mut chain: Vec<((u32, u32), [u8; 4])> = Vec::new();
        let add = |t: (u32, u32), c: [u8; 4], chain: &mut Vec<((u32, u32), [u8; 4])>| {
            if !chain.iter().any(|x| x.0 == t) {
                chain.push((t, c));
            }
        };
        for &(s, b) in u.extra.object_slots[1].iter().chain(&u.extra.object_slots[2]) {
            // (Only those still its own: a slot another object took since is not.)
            if self.tagged(s).1 != b {
                continue;
            }
            if let Some(c) = code_of(s).filter(|c| (c[0] == b'A' && c != codes::class::UNIT_AI) || c[0] == b'B') {
                add((s, b), c, &mut chain);
            }
        }
        for b in u.buffs.iter().filter(|b| self.buff_kept(b)) {
            let t = self.tagged(b.slot);
            if let Some(c) = code_of(b.slot) {
                add(t, c, &mut chain);
            }
        }
        for &(s, b) in &u.extra.ai_slots {
            if code_of(s) == Some(*codes::class::UNIT_AI_REF) && self.tagged(s).1 == b {
                add((s, b), *codes::class::UNIT_AI_REF, &mut chain);
            }
        }
        if let Some(p) = self.build_parts(h) {
            add(p[0], *codes::ability::id::BUILD_IN_PROGRESS, &mut chain);
        }
        for (t, c, _) in self.held_item_objects(h) {
            if c[0] == b'A' {
                add(t, c, &mut chain);
            }
        }
        // (Its order's tasks, after its abilities: read, a harvesting acolyte's; and a
        // waiting worker's place in its mine's queue after them.)
        if let Some(q) = self.saved_order(h).and_then(|o| o.queue) {
            add(q, *codes::class::UNIT_AI_REF, &mut chain);
        }
        if let Some(o) = self.saved_order(h) {
            for (slot, first, _) in &o.records {
                if let First::Agent { object, .. } = first {
                    if let Some(c) = self.tags.live.get(slot).map(|e| e.0).filter(|c| is_task(c)) {
                        add(rf2(object.links[0].2), c, &mut chain);
                    }
                }
            }
        }
        chain.sort_by_key(|x| x.0 .1);
        // Its rally point (the second table's agent) right after its rally ability.
        let me = u.extra.object_slots[0].first().copied().unwrap_or_default();
        if let (Some(&(r, _)), Some(k)) = (rallies.iter().find(|(_, owner)| *owner == rf(me)), chain.iter().position(|x| &x.1 == codes::ability::id::RALLY)) {
            chain.insert(k + 1, ((r.0, r.1), *codes::class::RALLY_POINT));
        }
        chain
    }

    /// The one each of a unit's abilities and buffs follows in its list, by the chain's
    /// index: the chain's order, but an aura's buff on its bearer, made just after the
    /// aura, comes before it (read: a troll's Trueshot Aura follows its buff, which
    /// follows its attack); a buff on another follows the last ability made before it,
    /// and none follows such a buff.
    pub(crate) fn chain_older(&self, h: u32, chain: &[((u32, u32), [u8; 4])]) -> Vec<Option<(u32, u32)>> {
        let own: Vec<(u32, u32)> = self.unit(h).map(|u| u.extra.object_slots[2].clone()).unwrap_or_default();
        let mut order: Vec<usize> = Vec::new();
        for (k, (t, c)) in chain.iter().enumerate() {
            if c[0] == b'A' {
                order.push(k);
            } else if c[0] == b'B' && own.contains(t) {
                match order.iter().rposition(|&j| chain[j].1[0] == b'A') {
                    Some(at) => order.insert(at, k),
                    None => order.push(k),
                }
            }
        }
        let mut out = vec![None; chain.len()];
        for (j, &k) in order.iter().enumerate().skip(1) {
            out[k] = Some(chain[order[j - 1]].0);
        }
        for (k, (t, c)) in chain.iter().enumerate() {
            if c[0] == b'B' && !own.contains(t) {
                out[k] = chain[..k].iter().rev().find(|c| c.1[0] == b'A').map(|c| c.0);
            }
        }
        out
    }

    /// A unit's current order's object (`ordt`) and its live tasks, by their making: a
    /// walk's (as `Tags::move_issued` and `player_move_issued` take them: the order, a
    /// block of tasks, the tree's six tasks from the third; its cluster and path last).
    /// Its objects were made one after another, so a slot another object has taken
    /// since is told by its birth.
    pub(crate) fn order_slots(&self, h: u32) -> Option<((u32, u32), Vec<(u32, u32)>)> {
        let u = self.unit(h)?;
        let tags = &u.extra.order_tags;
        if !matches!(tags.len(), 17 | 18) || !matches!(u.action, Action::Move { .. }) {
            return None;
        }
        if self.tags.live.get(&tags[0]).is_none_or(|e| &e.0 != codes::class::ORDER_TASKS) {
            return None;
        }
        let order = self.tagged(tags[0]);
        let tasks = (2..8).map(|k| self.tagged(tags[k])).filter(|t| t.1 > order.1 && t.1 <= order.1 + 17).collect();
        Some((order, tasks))
    }

    /// A unit's order's own records (read against the training test map's wanderers at
    /// 10 seconds): the order (`ordt`: 0, the order's id, its flags (a walk's 15), none,
    /// 0, its target point, its target (none), five 0, two 0.0; none, 0, and two
    /// references none) and its task tree, each task 0, the one before it (the first's
    /// none), 0, and its kind's event, then its own: the order tree's head (`tskA`,
    /// 0xd0162, 0), the order's (`tskO`, 0xd0166, the order's id), three steps (0xd0148,
    /// 0xd014a, 0xd0165), and the walk's point (`tsk.`, 0xd016b, its x, y, and 0.0; a
    /// reference none).  The order's first record too (the tasks' are their unit's
    /// chain's, after its abilities and AI places).  Not yet written: the walk's
    /// cluster and path, so a fresh save leaves a walk out.  Provisional: a player's
    /// walk's flags.
    pub fn order_records(&self, h: u32) -> Vec<(u32, Option<w3_formats::w3z::First>, Second)> {
        let Some(u) = self.unit(h) else { return Vec::new() };
        let Some((order, tasks)) = self.order_slots(h) else { return Vec::new() };
        let Action::Move { x, y } = u.action else { return Vec::new() };
        const MOVE: u32 = 851986;
        let w = Field::Word;
        let events = || Handlers { keyed: Vec::new(), fixed: Default::default(), events: vec![Vec::new(); 0x400] };
        let mut out = Vec::new();
        let own = vec![w(0), w(MOVE), w(15), w(u32::MAX), w(u32::MAX), w(0), Field::Real(x), Field::Real(y), w(u32::MAX), w(u32::MAX), w(0), w(0), w(0), w(0), w(0), Field::Real(0.0), Field::Real(0.0)];
        let tail = vec![w(u32::MAX), w(u32::MAX), w(0), Field::Ref(NONE), Field::Ref(NONE)];
        out.push((order.0, Some(super::agent_first(order, codes::class::ORDER_TASKS, &[])), Second::Agent { own, handlers: events(), tail }));
        let kinds: [(u32, Vec<Field>, Vec<Field>); 6] = [
            (0xd_0162, vec![w(0)], Vec::new()),
            (0xd_0166, vec![w(MOVE)], Vec::new()),
            (0xd_0148, Vec::new(), Vec::new()),
            (0xd_014a, Vec::new(), Vec::new()),
            (0xd_0165, Vec::new(), Vec::new()),
            (0xd_016b, vec![Field::Real(x), Field::Real(y), Field::Real(0.0)], vec![Field::Ref(NONE)]),
        ];
        for (k, &t) in tasks.iter().enumerate() {
            let Some((event, extra, tail)) = kinds.get(k + 6 - tasks.len()) else { continue };
            let before = if k > 0 { rf(tasks[k - 1]) } else { NONE };
            let mut own = vec![w(0), w(before.0), w(before.1), w(0), w(*event)];
            own.extend(extra.iter().cloned());
            out.push((t.0, None, Second::Agent { own, handlers: events(), tail: tail.clone() }));
        }
        out
    }

    /// The agent records of a unit's ability chain (read against the quiet test map's):
    /// each its class's, its properties (the `` `rel `` objects made right after it),
    /// and in its first link set the one made after it and the one before, and the
    /// unit.
    pub fn ability_agent_records(&self, h: u32, rallies: &[(Ref, Ref)]) -> Vec<(u32, w3_formats::w3z::First)> {
        let Some(u) = self.unit(h) else { return Vec::new() };
        let Some(&me) = u.extra.object_slots[0].first() else { return Vec::new() };
        let chain = self.ability_chain(h, rallies);
        let slots: Vec<(u32, u32)> = u.extra.object_slots[1].iter().chain(&u.extra.object_slots[2]).copied().collect();
        let mut out = Vec::new();
        for (k, &(t, code)) in chain.iter().enumerate() {
            if &code == codes::class::RALLY_POINT {
                continue;
            }
            let build = self.build_parts(h).filter(|_| &code == codes::ability::id::BUILD_IN_PROGRESS);
            let props: Vec<(u32, u32)> = match slots.iter().position(|&x| x == t) {
                // (A structure being built's its real.)
                _ if build.is_some() => build.map(|p| vec![p[1]]).unwrap_or_default(),
                // (A hero's its two flags.)
                _ if &code == codes::ability::id::HERO => self.hero_parts(h).map(|p| p.1.to_vec()).unwrap_or_default(),
                Some(i) => slots[i + 1..].iter().copied().take_while(|&(s, b)| self.tagged(s).1 == b && self.tags.live.get(&s).is_some_and(|e| &e.0 == codes::class::REAL)).collect(),
                None => Vec::new(),
            };
            let mut rec = super::agent_first(t, &code, &props);
            if let w3_formats::w3z::First::Agent { object, links, .. } = &mut rec {
                let newer = chain.get(k + 1).map_or(NONE, |c| rf(c.0));
                let older = if k > 0 { rf(chain[k - 1].0) } else { NONE };
                object.links[0] = (w3_formats::w3z::Link(newer, 0), w3_formats::w3z::Link(older, 0), rf(t), rf(me));
                // (Its modifier's and listener's heads: the rise's.)
                if let Some(p) = build {
                    links[2] = w3_formats::w3z::Link(rf(p[2]), 0);
                    links[3] = w3_formats::w3z::Link(rf(p[3]), 0);
                }
            }
            out.push((t.0, rec));
        }
        out
    }
}

impl World {
    /// A structure's blight growth's objects (Abli's: `Xbli`, its `` `pos ``, two
    /// `^pmr`), first records and Xbli's second: an agent with its position (the
    /// structure's place in pathing cells; its coarse region unflagged, words 1; its
    /// fine one as a tree's), and its own fields: 0, 1, the ability's Area, Dur, and
    /// DataA, the steps grown, and 2 once grown (its timer then none).  Provisional:
    /// one still growing (1, its timer).
    pub fn blight_growth_records(&self, h: u32) -> (Vec<(u32, w3_formats::w3z::First)>, Vec<(u32, w3_formats::w3z::Second)>) {
        use w3_formats::w3z::{Field, First, Handlers, Second, Timing};
        let (mut firsts, mut seconds) = (Vec::new(), Vec::new());
        let Some(u) = self.unit(h) else { return (firsts, seconds) };
        let Some(p) = self.pathing.as_ref() else { return (firsts, seconds) };
        let Some((a, lvl)) = self.ability_with_code(h, codes::ability::BLIGHT_GROWTH) else { return (firsts, seconds) };
        if u.extra.blight_slots.len() != 5 {
            return (firsts, seconds);
        }
        let t = |k: usize| self.tagged(u.extra.blight_slots[k]);
        let (xbli, pos, r1, r2) = (t(1), t(2), t(3), t(4));
        let [coarse, fine] = self.pathing_map_objects();
        let (cx, cy) = ((u.x - p.origin[0]) / crate::pathing::CELL, (u.y - p.origin[1]) / crate::pathing::CELL);
        let (row, col) = (cy.floor() as u32, cx.floor() as u32);
        firsts.push((xbli.0, super::agent_first(xbli, codes::class::BLIGHT_GROWTH, &[pos])));
        firsts.push((pos.0, First::Pos { base: super::property_base(pos, xbli), values: [cx, cy, 0.0, 0.0, f32::MAX, 0.0, 0.0], refs: [rf(r1), rf(r2)] }));
        firsts.push((r1.0, First::Region { refs: [rf(coarse), rf(pos)], word: 0, bounds: [row >> 3, col >> 3, (row >> 3) + 1, (col >> 3) + 1], words: [0, 1, 0] }));
        firsts.push((r2.0, First::Region { refs: [rf(fine), rf(pos)], word: 0x0100_0000, bounds: [row, col, row + 1, col + 1], words: [0, 1, 0] }));
        let o = self.objects.abilities.get(a);
        let (area, dur, step) = o.map_or((0.0, 0.0, 0.0), |o| (o.level_real("Area", lvl), o.level_real("Dur", lvl), o.level_real("DataA", lvl)));
        let grown = u.extra.blight_radius >= area;
        // (The area as the disc rounds it to the 128 grid: a Necropolis's 960 is 1024,
        // its steps 16.)
        let rounded = (area / 128.0).round() * 128.0;
        let steps = if step <= 0.0 { 0 } else if grown { (rounded / step) as u32 } else { (u.extra.blight_radius / step) as u32 };
        seconds.push((xbli.0, Second::Agent {
            own: vec![Field::Word(0), Field::Word(1), Field::Real(rounded), Field::Real(dur), Field::Real(step), Field::Word(steps), Field::Word(if grown { 2 } else { 1 })],
            handlers: Handlers { keyed: Vec::new(), fixed: Default::default(), events: vec![Vec::new(); 0x400] },
            tail: vec![Field::Timer(NONE, 0, Timing(None))],
        }));
        (firsts, seconds)
    }

    /// A haunted mine's work slots and their circles (Abgm's objects after its two reals:
    /// five `rslt`, then five of `+EIP`, `` `pos ``, and two `^pmr`), first and second
    /// records.  A slot (`rslt`): its place, its angle out from the mine (a quarter turn
    /// less k fifths of a turn, as summed), its circle, and the acolyte at it (none).  Its
    /// circle (`+EIP`, the ability's Effectart): an agent with its position (the slot's
    /// place floored to quarter units, in pathing cells; its coarse region flagged
    /// 0x02000000, 0x60000001; its fine one as a tree's), its own (0, -1, -1, 0), no
    /// handlers, its sprite (as the quiet test map's: its render state's words not
    /// the port's), and its place with the ground's height.  Provisional: an acolyte at
    /// a slot, the sprite's state.
    pub fn mine_circle_records(&self, h: u32) -> (Vec<(u32, w3_formats::w3z::First)>, Vec<(u32, w3_formats::w3z::Second)>) {
        use w3_formats::w3z::{Field, First, Handlers, Link, Second, Sprite};
        let (mut firsts, mut seconds) = (Vec::new(), Vec::new());
        let Some(u) = self.unit(h) else { return (firsts, seconds) };
        let Some((abgm, _)) = self.ability_with_code(h, codes::ability::BLIGHTED_GOLD_MINE) else { return (firsts, seconds) };
        let Some(p) = self.pathing.as_ref() else { return (firsts, seconds) };
        let objs = &u.extra.object_slots[2];
        let Some(i) = objs.iter().position(|&(s, _)| self.tags.live.get(&s).is_some_and(|e| &e.0 == codes::ability::id::BLIGHTED_GOLD_MINE)) else { return (firsts, seconds) };
        if objs.len() < i + 28 {
            return (firsts, seconds);
        }
        let slots = self.mine_slots_pub(h);
        let n = slots.len().max(1);
        let model = self.profile_art(abgm, "Effectart");
        let [coarse, fine] = self.pathing_map_objects();
        let none_handlers = || Handlers { keyed: Vec::new(), fixed: Default::default(), events: vec![Vec::new(); 0x400] };
        let (mx, my) = (u.x, u.y);
        let radius = self.objects.abilities.get(abgm).map_or(200.0, |a| a.level_real("DataD", 1));
        use w3_jass::softfloat as sf;
        for k in 0..5.min(slots.len()) {
            if slots[k].is_none() {
                continue;
            }
            // (`FUN_6f4d7e40`, in the engine's soft floats: the turn's k nths, x by its
            // sine and y by its cosine times the radius, the angle out a quarter turn less
            // it.)
            let a = sf::div(sf::mul(std::f32::consts::TAU.to_bits(), sf::itof(k as i32)), sf::itof(n as i32));
            let x = f32::from_bits(sf::add(mx.to_bits(), sf::mul(radius.to_bits(), sf::sin(a))));
            let y = f32::from_bits(sf::add(my.to_bits(), sf::mul(radius.to_bits(), sf::cos(a))));
            let angle = f32::from_bits(sf::sub(std::f32::consts::FRAC_PI_2.to_bits(), a));
            let rslt = objs[i + 3 + k];
            let eip = objs[i + 8 + 4 * k];
            let (pos, r1, r2) = (objs[i + 9 + 4 * k], objs[i + 10 + 4 * k], objs[i + 11 + 4 * k]);
            let worker = (u32::MAX, u32::MAX);
            seconds.push((rslt.0, Second::Agent {
                own: vec![Field::Real(x), Field::Real(y), Field::Real(angle), Field::Word(eip.0), Field::Word(eip.1), Field::Word(0), Field::Word(worker.0), Field::Word(worker.1), Field::Word(0)],
                handlers: Handlers { keyed: Vec::new(), fixed: Default::default(), events: Vec::new() },
                tail: Vec::new(),
            }));
            let (ex, ey) = ((x * 4.0).floor() / 4.0, (y * 4.0).floor() / 4.0);
            let (cx, cy) = ((ex - p.origin[0]) / crate::pathing::CELL, (ey - p.origin[1]) / crate::pathing::CELL);
            let (row, col) = (cy.floor() as u32, cx.floor() as u32);
            firsts.push((eip.0, super::agent_first(eip, codes::class::SPECIAL_EFFECT, &[pos])));
            firsts.push((pos.0, First::Pos { base: super::property_base(pos, eip), values: [cx, cy, 0.0, 0.0, f32::MAX, 0.0, 0.0], refs: [rf(r1), rf(r2)] }));
            firsts.push((r1.0, First::Region { refs: [rf(coarse), rf(pos)], word: 0x0200_0000, bounds: [row >> 3, col >> 3, (row >> 3) + 1, (col >> 3) + 1], words: [0, 0x6000_0001, 0] }));
            firsts.push((r2.0, First::Region { refs: [rf(fine), rf(pos)], word: 0x0100_0000, bounds: [row, col, row + 1, col + 1], words: [0, 1, 0] }));
            let sprite = Sprite { model: model.clone(), real: 5.7599255e-37, short: 0, byte: 255, word: u32::MAX, reals: [1.0, 1.0], words: [0, 0], last_real: 2.1e-43, last_byte: 0, flags: [2, 0, 1], entries: vec![(0.0, 0, vec![0]), (0.0, 4, vec![1])] };
            let z = self.ground_z(ex, ey);
            seconds.push((eip.0, Second::Agent {
                own: vec![Field::Word(0), Field::Word(u32::MAX), Field::Word(u32::MAX), Field::Word(0)],
                handlers: none_handlers(),
                tail: vec![Field::Sprite(Some(sprite)), Field::Word(0), Field::Timer(NONE, 0, w3_formats::w3z::Timing(None)), Field::Word(0), Field::List(vec![vec![Field::Word(0)]]), Field::List((0..10).map(|_| vec![Field::Word(0)]).collect()), Field::Real(ex), Field::Real(ey), Field::Real(z)],
            }));
            let _ = Link(NONE, 0);
        }
        (firsts, seconds)
    }

    /// A structure's footprint's three regions (`^pmr` on the pathing map, as a tree's):
    /// flags 0xc2 and 0x10 over its pathing footprint's cells, 0x08 over where none may
    /// build; each bounded by the building footprint's first row and column and their
    /// last plus 2, its cells' count, and 0x10000000 (the search mark the running
    /// game's, written 0).
    pub fn structure_region_records(&self, h: u32) -> Vec<(u32, w3_formats::w3z::First)> {
        let Some(u) = self.unit(h) else { return Vec::new() };
        if u.extra.footprint_slots.len() != 3 {
            return Vec::new();
        }
        let maps = self.pathing_map_objects();
        let me = u.extra.object_slots[0].first().copied().unwrap_or((u32::MAX, u32::MAX));
        let outline = if u.build_cells.is_empty() { &u.pathing_cells } else { &u.build_cells };
        let rows = outline.iter().map(|c| c.1 as u32);
        let cols = outline.iter().map(|c| c.0 as u32);
        let bounds = [rows.clone().min().unwrap_or(0), cols.clone().min().unwrap_or(0), rows.max().unwrap_or(0) + 2, cols.max().unwrap_or(0) + 2];
        let mut out = Vec::new();
        for (k, flags) in [0xc2u32, 0x10, 0x08].into_iter().enumerate() {
            let n = if k < 2 { u.pathing_cells.len() } else { u.build_cells.len() } as u32;
            let slot = u.extra.footprint_slots[k];
            let words = [0, n, 0x1000_0000];
            out.push((slot, w3_formats::w3z::First::Region { refs: [rf(maps[1]), rf(me)], word: 0x0100_0000 | flags, bounds, words }));
        }
        out
    }

    /// A unit's abilities' indexed reals (`` `rel ``, each an ability's own, made after
    /// it): stamped, listed on the ability, each its value, rate, and bounds as made (read
    /// on the quiet test map: the gold mine's and Abgm's first -0 unbounded, Abgm's
    /// second 0 unbounded, Select's, the queue's, Upgrade's, and the builds' 0 up to the
    /// largest real; provisional: their values later).
    pub fn ability_real_records(&self, h: u32, rallies: &[(Ref, Ref)]) -> Vec<(u32, w3_formats::w3z::First)> {
        let Some(u) = self.unit(h) else { return Vec::new() };
        let chain = self.ability_chain(h, rallies);
        let slots: Vec<(u32, u32)> = u.extra.object_slots[1].iter().chain(&u.extra.object_slots[2]).copied().collect();
        let mut out = Vec::new();
        for &(t, code) in &chain {
            let Some(i) = slots.iter().position(|&x| x == t) else { continue };
            let props: Vec<(u32, u32)> = slots[i + 1..].iter().copied().take_while(|&(s, b)| self.tagged(s).1 == b && self.tags.live.get(&s).is_some_and(|e| &e.0 == codes::class::REAL)).collect();
            for k in 0..props.len() {
                // (Abgm's second 0, not -0.)
                let values = match (&code, k) {
                    (codes::ability::id::GOLD_MINE | codes::ability::id::BLIGHTED_GOLD_MINE, 0) => [-0.0, 0.0, -f32::MAX, f32::MAX],
                    (codes::ability::id::BLIGHTED_GOLD_MINE, _) => [0.0, 0.0, -f32::MAX, f32::MAX],
                    _ => [0.0, 0.0, 0.0, f32::MAX],
                };
                let base = self.stamped(super::listed_property_base(&props, k, t));
                out.push((props[k].0, w3_formats::w3z::First::Real { base, values }));
            }
        }
        out
    }
}

/// An ability's targets as the engine holds them (+0x4c), by kind, read in the quiet
/// test map's and Forestwalk's saves (its data's targs1 as bits: ground 0x2, air 0x4,
/// structure 0x8, ward 0x10, tree 0x40, bridge 0x400, self 0x1000, friend 0x6000,
/// neutral 0x8000, enemy 0x10000, vulnerable 0x100000, invulnerable 0x200000, alive
/// 0x1000000, dead 0x2000000, organic 0x4000000, mechanical 0x8000000, nonancient
/// 0x40000000; provisional: the defaults for what its data leaves out, and 0 for the
/// kinds not read).
fn ability_targets(code: &[u8; 4]) -> u32 {
    match code {
        codes::ability::id::ALARM => 0x0031_0006,
        codes::ability::id::ITEM_TOWN_PORTAL => 0x0030_0028,
        codes::ability::id::INVULNERABLE | codes::ability::id::WANDER | codes::ability::id::GOLD_MINE | codes::ability::id::MILITIA_CONVERSION | codes::ability::id::MILITIA | codes::ability::id::MAGIC_IMMUNITY | codes::ability::id::NEUTRAL_DETECTION_REVEAL_ABILITY | codes::ability::id::NEUTRAL_BUILDING | codes::ability::id::RETURN => 0xfdd1_f00e,
        codes::ability::id::PRIESTESS_TRUESHOT_AURA | codes::ability::id::PALADIN_DEVOTION_AURA | codes::ability::id::DEATH_KNIGHT_UNHOLY_AURA => 0x0030_7006,
        codes::ability::id::DREADLORD_VAMPIRIC_AURA => 0x0430_7006,
        codes::ability::id::CURSE | codes::ability::id::DREADLORD_SLEEP => 0x0401_8006,
        codes::ability::id::BLOODLUST => 0x0400_f006,
        codes::ability::id::FAERIE_FIRE | codes::ability::id::DEMON_HUNTER_MANA_BURN | codes::ability::id::BLADE_MASTER_CRITICAL_STRIKE => 0x0001_8006,
        codes::ability::id::REJUVINATION => 0x0430_f006,
        codes::ability::id::ANIMATE_DEAD_CREEP => 0x0200_0006,
        codes::ability::id::DEVOUR_DRAGON_CREEP => 0x0481_8002,
        codes::ability::id::RAISE_DEAD => 0xffd1_f00e,
        codes::ability::id::SHADOW_MELD | codes::ability::id::BURROW_DETECTION_FLYERS | codes::ability::id::SPAWN_SPIDER | codes::ability::id::BURROW => 0xfdd1_f00e,
        codes::ability::id::WEB => 0x0001_8004,
        codes::ability::id::DEATH_KNIGHT_DEATH_COIL => 0x4431_e006,
        codes::ability::id::CRIPPLE => 0x0401_8006,
        codes::ability::id::UNHOLY_FRENZY => 0x0400_0006,
        codes::ability::id::EXHUME | codes::ability::id::MEAT_DROP => 0xfdd1_f00e,
        codes::ability::id::MEAT_LOAD => 0x0280_0002,
        codes::ability::id::CARGO_HOLD_AMTC => 0x0200_0000,
        codes::ability::id::PLAGUE_TOSS => 0x0401_81ca,
        codes::ability::id::ALLIED_BUILDING => 0xfdd1_f00e,
        codes::ability::id::SPIDER_ATTACK => 0x0001_013a,
        codes::ability::id::CARGO_HOLD_DEVOUR => 0x0031_8002,
        codes::ability::id::THUNDER_BOLT_CREEP => 0x0401_8006,
        codes::ability::id::THUNDER_CLAP_CREEP => 0x0000_8002,
        codes::ability::id::CHIEFTAIN_SHOCK_WAVE => 0x0001_000a,
        codes::ability::id::ABOLISH_MAGIC => 0x0030_0056,
        codes::ability::id::MANA_REGENERATION_AURA => 0x0430_0006,
        codes::ability::id::CANNIBALIZE => 0xffd1_f006,
        codes::ability::id::ENSNARE => 0x0081_8006,
        codes::ability::id::HARVEST | codes::ability::id::HARVEST_LUMBER => 0x0300_0040,
        codes::ability::id::INNER_FIRE => 0x0000_f006,
        codes::ability::id::SACRIFICE_ACOLYTE | codes::ability::id::UNSUMMON => 0x0000_2008,
        codes::ability::id::HEAL_CREEP_NORMAL => 0x4430_f006,
        codes::ability::id::REPAIR_ORC | codes::ability::id::RESTORATION => 0x4b30_640e,
        codes::ability::id::VENOM_SPEARS => 0x0400_0006,
        codes::ability::id::ACOLYTE_HARVEST | codes::ability::id::BLIGHTED_GOLD_MINE | codes::ability::id::BLIGHT_GROWTH | codes::ability::id::INVENTORY => 0xfdd1_f00e,
        _ => 0,
    }
}

impl World {
    /// The second record of an ability on a unit's chain, for the kinds read so far
    /// (read against the quiet test map's): CAbility's part (`FUN_6f42e830`: its flags,
    /// the ability made before it, five words, its targets, its code, a word, a timer,
    /// and a word), then its kind's.  None for kinds not read.
    /// An aura's buff's own record (CBuff; read against the quiet test map's Trueshot
    /// and Devotion Aura buffs): CAbility's part (its flags 0x1000000 on its bearer,
    /// 0x80000000 on another; 0x290 Devotion's, 0xa90 Trueshot's, provisional the
    /// rest's), 0, 0.0, 0, 0, its aura's targets, three 0.0, its aura's DataA, its
    /// bearer, 0, and Devotion's armor given; its art (the second table's, as `art` has
    /// it), 0, its timer (its span from its last start, event 0xd01c4, once), its span,
    /// and Trueshot's bonus per weapon (as an attack's).
    fn buff_own_record(&self, h: u32, t: (u32, u32), code: &[u8; 4], older: Option<(u32, u32)>, art: &std::collections::HashMap<u32, Field>) -> Option<Second> {
        // (The aura buffs read; others, Brilliance's among them, by their templates.)
        if !matches!(code, codes::buff::id::TRUESHOT_AURA | codes::buff::id::AURA_COMMAND | codes::buff::id::WAR_DRUMS | codes::buff::id::DEVOTION_AURA | codes::buff::id::UNHOLY_AURA | codes::buff::id::THORNS_AURA | codes::buff::id::VAMPIRIC_AURA | codes::buff::id::ENDURANCE_AURA | codes::buff::id::HEALING_WARD_AURA) {
            return None;
        }
        let u = self.unit(h)?;
        let b = u.buffs.iter().find(|b| b.slot == t.0 && self.buff_kept(b)).or_else(|| u.buffs.iter().find(|b| b.slot == 0 && &b.id == code && b.caster == h))?;
        let aura = self.objects.abilities.get(b.ability)?;
        let aura_code: [u8; 4] = self.ability_code(b.ability).as_bytes().try_into().ok()?;
        let data_a = aura.level_real("DataA", b.level.max(1));
        let caster = self.unit(b.caster).and_then(|c| c.extra.object_slots[0].first().copied()).unwrap_or((u32::MAX, u32::MAX));
        let ai = self.players.get(u.owner as usize).is_some_and(|p| p.controller == crate::types::map_control::COMPUTER && p.slot_state == crate::types::slot_state::PLAYING) || u.owner == crate::types::PLAYER_NEUTRAL_AGGRESSIVE;
        let base: u32 = match code {
            codes::buff::id::DEVOTION_AURA => 0x290,
            codes::buff::id::TRUESHOT_AURA => 0xa90,
            codes::buff::id::UNHOLY_AURA => 0x690,
            _ => 0x10,
        };
        let flags = base | if ai { 0x2 } else { 0 } | if b.caster == h { 0x100_0000 } else { 0x8000_0000 };
        let w = Field::Word;
        let r = Field::Real;
        let none = || Field::Timer(NONE, 0, Timing(None));
        let prev = older.map_or(NONE, rf);
        let mut own = vec![w(flags), w(prev.0), w(prev.1), w(0), w(0), w(0), w(0), w(0), w(0), w(u32::from_be_bytes(*code)), w(0), none(), w(0)];
        own.extend([w(0), r(0.0), w(0), w(0), w(ability_targets(&aura_code)), r(0.0), r(0.0), r(0.0)]);
        if code == codes::buff::id::UNHOLY_AURA {
            // (Unholy Aura's: 0.0, its bearer, 0.0, and its DataA and DataB: read,
            // Forestwalk's.)
            own.extend([r(0.0), w(caster.0), w(caster.1), r(0.0), r(data_a), r(aura.level_real("DataB", b.level.max(1)))]);
        } else {
            own.extend([r(data_a), w(caster.0), w(caster.1), w(0)]);
        }
        if code == codes::buff::id::DEVOTION_AURA {
            own.push(r(b.armor));
        }
        // (Endurance's two reals more, provisional its DataA and DataB.)
        if code == codes::buff::id::ENDURANCE_AURA {
            own.extend([r(data_a), r(aura.level_real("DataB", b.level.max(1)))]);
        }
        let timer = match self.tags.timer_objects.get(&t.0).and_then(|v| v.iter().rev().find(|x| x.1 == "532b50")) {
            Some(&(n, _, at, span)) => Field::Timer(rf(t), 0, Timing(Some((f32::from_bits(crate::efloat::add(at.to_bits(), span.to_bits())), span, n, 0xd_01c4, 0)))),
            None => none(),
        };
        let mut tail = vec![art.get(&t.0).cloned().unwrap_or(Field::List(Vec::new())), w(0), timer, Field::Short(0), Field::Short(0), r(crate::ability::AURA_BUFF_TIME)];
        // (A bonus a weapon, Trueshot's, Command's, and War Drums': read, Hills of
        // Glory's Command Aura's 3 and 1; provisional, the others' values.)  Thorns'
        // and Vampiric's the aura base's save, its extended timer alone.
        let plain = matches!(code, codes::buff::id::THORNS_AURA | codes::buff::id::VAMPIRIC_AURA);
        if plain {
            return Some(Second::Agent { own, handlers: Handlers { keyed: Vec::new(), fixed: Default::default(), events: vec![Vec::new(); 0x400] }, tail });
        }
        if !matches!(code, codes::buff::id::DEVOTION_AURA | codes::buff::id::UNHOLY_AURA | codes::buff::id::ENDURANCE_AURA | codes::buff::id::HEALING_WARD_AURA) {
            let o = self.objects.units.get(u.type_id)?;
            let weapons = o.int("weapsOn");
            let bonus = |k: u32| {
                let on = weapons & (1 << (k - 1)) != 0;
                let least = if on { o.int(&format!("dmgplus{k}")).max(0) + o.int(&format!("dice{k}")).max(0) } else { 1 };
                vec![w(f32::from_bits(crate::efloat::mul(crate::efloat::from_int(least), data_a.to_bits())).ceil() as u32)]
            };
            tail.push(Field::List(vec![bonus(1), bonus(2)]));
        }
        // Unholy Aura's modifier made with it; the healing ward's aura's as its
        // (`FUN_6f42e550`; provisional, read of its class alone).
        if code == codes::buff::id::UNHOLY_AURA || code == codes::buff::id::HEALING_WARD_AURA {
            tail.push(Field::Ref(self.tags.buff_modifiers.get(&t.0).map_or(NONE, |&m| rf(self.tagged(m)))));
        }
        tail.push(none());
        Some(Second::Agent { own, handlers: Handlers { keyed: Vec::new(), fixed: Default::default(), events: vec![Vec::new(); 0x400] }, tail })
    }

    pub fn ability_own_record(&self, h: u32, t: (u32, u32), code: &[u8; 4], older: Option<(u32, u32)>, rallies: &[(Ref, Ref)], art: &std::collections::HashMap<u32, Field>) -> Option<Second> {
        // (A task on its chain is its order's: `saved_order`.)
        if is_task(code) {
            return None;
        }
        // (A buff the aura buffs' writer does not take is written from its kind's
        // template, below.)
        if code[0] == b'B' {
            if let Some(r) = self.buff_own_record(h, t, code, older, art) {
                return Some(r);
            }
        }
        let u = self.unit(h)?;
        let o = self.objects.units.get(u.type_id)?;
        let computer = self.players.get(u.owner as usize).is_some_and(|p| p.controller == crate::types::map_control::COMPUTER && p.slot_state == crate::types::slot_state::PLAYING);
        let ai = computer || u.owner == crate::types::PLAYER_NEUTRAL_AGGRESSIVE;
        let w = Field::Word;
        let timer = || Field::Timer(NONE, 0, Timing(None));
        // Its handlers keyed by another object, with their events.
        let mut keyed: Vec<(u32, Vec<(Ref, u32)>)> = Vec::new();
        // A periodic agent timer it started (by its function's name): its next time due
        // after now, its period, its count, and the event it sends.
        let periodic = |name: &str, period: f32, event: u32| {
            let started = self.tags.timer_objects.get(&t.0).and_then(|v| v.iter().find(|x| x.1 == name)).map(|x| (x.0, x.2));
            // (Its period: the kind's.)
            match started {
                Some((count, at)) => {
                    // (Its due time stepped by its period in the engine's arithmetic.)
                    let mut due = crate::efloat::add(at.to_bits(), period.to_bits());
                    while f32::from_bits(due) <= self.time {
                        due = crate::efloat::add(due, period.to_bits());
                    }
                    Field::Timer(rf(t), 2, Timing(Some((f32::from_bits(due), period, count, event, 1))))
                }
                None => timer(),
            }
        };
        let weapons = o.int("weapsOn");
        let peon = o.list("type").iter().any(|t| t.trim().eq_ignore_ascii_case("peon"));
        let base_flags: u32 = match code {
            codes::ability::id::INVULNERABLE => 0x8000_0010,
            // (Spells, read: targeted 0x0, autocast 0x8080000 with 0x80 for heal, repair
            // 0x9400010, restore 0x9080010, Lightning Shield-like 0x0, Cannibalize
            // 0x80010.)
            codes::ability::id::THUNDER_BOLT_CREEP | codes::ability::id::THUNDER_CLAP_CREEP | codes::ability::id::CHIEFTAIN_SHOCK_WAVE | codes::ability::id::ENSNARE => 0,
            codes::ability::id::SACRIFICE_ACOLYTE | codes::ability::id::UNSUMMON => 0x10,
            codes::ability::id::CANNIBALIZE => 0x8_0010,
            codes::ability::id::HEAL_CREEP_NORMAL => 0x808_0080,
            codes::ability::id::ABOLISH_MAGIC | codes::ability::id::INNER_FIRE => 0x808_0000,
            // (Read, Forestwalk's creeps': the autocast spells 0x8080080, Mana Burn 0x10,
            // Shadowmeld 0x110, the cargo hold 0x90.)
            codes::ability::id::CURSE | codes::ability::id::BLOODLUST | codes::ability::id::FAERIE_FIRE | codes::ability::id::RAISE_DEAD => 0x808_0080,
            codes::ability::id::ANIMATE_DEAD_CREEP | codes::ability::id::DEVOUR_DRAGON_CREEP | codes::ability::id::DREADLORD_SLEEP | codes::ability::id::REJUVINATION | codes::ability::id::BLADE_MASTER_CRITICAL_STRIKE | codes::ability::id::PURCHASE_ITEM => 0,
            codes::ability::id::SHADOW_MELD => 0x110,
            codes::ability::id::CARGO_HOLD_DEVOUR => 0x90,
            // (Web 0x180010, Burrow 0x200010: read, the computer's Crypt Fiends at 300
            // seconds; provisional, the bits' meaning, their research not yet done.)
            codes::ability::id::WEB => 0x18_0010,
            codes::ability::id::MEAT_LOAD => 0x808_0010,
            codes::ability::id::BURROW => 0x20_0010,
            codes::ability::id::REPAIR_ORC => 0x940_0010,
            codes::ability::id::RESTORATION => 0x908_0010,
            codes::ability::id::BUILD_HUMAN | codes::ability::id::BUILD_UNDEAD | codes::ability::id::BUILD_ORC | codes::ability::id::BUILD_NIGHT_ELF => 0x2000_0010,
            codes::ability::id::HARVEST | codes::ability::id::HARVEST_LUMBER => 0x8000_0010,
            codes::ability::id::ACOLYTE_HARVEST => 0x210,
            codes::ability::id::CREEP_SLEEP => 0x400,
            codes::ability::id::BLIGHT_GROWTH => 0x290,
            // (An aura, read: a creep's 0x90000080, a fountain's mana regeneration
            // 0x90000090.)
            codes::ability::id::PRIESTESS_TRUESHOT_AURA | codes::ability::id::PALADIN_DEVOTION_AURA | codes::ability::id::ARCH_MAGE_BRILLIANCE_AURA | codes::ability::id::DEATH_KNIGHT_UNHOLY_AURA | codes::ability::id::CHIEFTAIN_ENDURANCE_AURA | codes::ability::id::DREADLORD_VAMPIRIC_AURA | codes::ability::id::KEEPER_THORNS_AURA | codes::ability::id::AURA_WAR_DRUMS | codes::ability::id::COMMAND_AURA | codes::ability::id::AURA_REGENERATION_WARD => 0x9000_0080,
            codes::ability::id::MANA_REGENERATION_AURA => 0x9000_0090,
            codes::ability::id::VENOM_SPEARS | codes::ability::id::MAGIC_IMMUNITY => 0,
            // (A hero's, read: Forestwalk's computer's Dreadlord's 0x92 with its AI's.)
            codes::ability::id::RALLY | codes::ability::id::MILITIA_CONVERSION | codes::ability::id::MILITIA | codes::ability::id::HERO => 0x90,
            // (An item's Town Portal, read: 0x2022 with its holder's AI's.)
            codes::ability::id::ITEM_TOWN_PORTAL => 0x2020,
            codes::ability::id::SELL_UNITS | codes::ability::id::SELL_ITEMS | codes::ability::id::MAKE_ITEMS => 0x1000_0010,
            // (Regeneration on blight, as it stands on blight 0x80.)
            codes::ability::id::REGENERATE_ON_BLIGHT => 0x10 | if u.extra.blight_rcm.is_some() { 0x80 } else { 0 },
            // (An attack: two weapons 0x100000, not a worker's 0x20004000, one never
            // asleep 0x2000000: read, every unit of the quiet test map's.)
            codes::ability::id::ATTACK => 0x8_0010 | if weapons & 2 != 0 { 0x10_0000 } else { 0 } | if peon { 0 } else { 0x2000_4000 } | if u.extra.woke_at.is_none() { 0x200_0000 } else { 0 },
            _ => 0x10,
        };
        // (A hero's skill 0x10 more: read, a Dreadlord's Sleep 0x12 where a creep's is 2.
        // A hero's abilities made with it 0x80 more, its attack without 0x4000 and
        // 0x2000000: read, a Dreadlord's Aatk 0x20080092, AInv 0x92; provisional, one
        // hero read.)
        let skill = u.is_hero && u.abilities.iter().any(|(a, _)| self.objects.abilities.get(*a).is_some_and(|o| o.int("hero") != 0 && crate::objects::id_from_str(o.str("code").trim()) == *code));
        let made_with = u.is_hero && !skill && u.extra.object_slots.iter().flatten().any(|&x| x == t);
        let base_flags = if made_with && code == codes::ability::id::ATTACK { base_flags & !0x0200_4000 } else { base_flags };
        // (A unit with an order its attack 0x80 more and 0x2000000 none: read, acolytes
        // and peasants at work.)
        let base_flags = if code == codes::ability::id::ATTACK && self.saved_order(h).is_some() { (base_flags & !0x0200_0000) | 0x80 } else { base_flags };
        // (A unit with an order its repair 0x100000 more: read, Forestwalk's peasants
        // mining and sent to a point, 0x9500010 where idle ones' are 0x9400010.)
        let ordered = self.saved_order(h);
        let base_flags = if code == codes::ability::id::REPAIR_ORC && ordered.is_some() { base_flags | 0x10_0000 } else { base_flags };
        // (Walking to a point under its move order, its movement the order's: 0x80,
        // 0x800, and 0x4: read, a peasant's 0x894.)
        let base_flags = if code == codes::ability::id::MOVE && ordered.as_ref().is_some_and(|o| o.state.kind == 7 && o.walk.is_some()) { base_flags | 0x884 } else { base_flags };
        let flags = base_flags | if ai { 0x2 } else { 0 } | if skill { 0x10 } else { 0 } | if made_with { 0x80 } else { 0 };
        let prev = older.map_or(NONE, rf);
        // (Its id, not its kind's code: a shop's Ane2, a creep's ACvs and ACmi, a town
        // hall's Argl; the last word Call to Arms' 1.)
        let id = u.abilities.iter().map(|(a, _)| *a).find(|a| self.objects.abilities.get(*a).is_some_and(|o| crate::objects::id_from_str(o.str("code").trim()) == *code)).unwrap_or(*code);
        let last = if code == codes::ability::id::MILITIA_CONVERSION { 1 } else { 0 };
        // (The kinds written by family, their targets from their data.)
        let targets = match ability_family(code) {
            Some(_) => self.objects.abilities.get(id).map_or(0xfdd1_f00e, |a| data_targets(&a.level_str("targs", 1).split(',').map(str::to_string).collect::<Vec<_>>())),
            None => ability_targets(code),
        };
        let mut own = vec![w(flags), w(prev.0), w(prev.1), w(0), w(0), w(0), w(0), w(0), w(targets), w(u32::from_be_bytes(id)), w(0), timer(), w(last)];
        let tail = match code {
            // CAbilityMove (`FUN_6f5fec80`): two references, its speed and 1.0, words,
            // two reals, references; its two timers.
            // (On a gold run its run's factor, and walking 0x4 more: read, a peasant's
            // 1.2.)
            codes::ability::id::MOVE => {
                let run = u.extra.gold_run.as_ref();
                let factor = run.map_or(1.0, |r| f32::from_bits(r.factor));
                if run.is_some_and(|r| matches!(r.phase, crate::goldrun::Phase::ToMine | crate::goldrun::Phase::ToHall(_))) {
                    if let Some(Field::Word(flags)) = own.first_mut() {
                        *flags |= 0x4;
                    }
                }
                // (Walking home, the hall it walks to: read, a peasant with its load.)
                let to = run.and_then(|r| match r.phase {
                    crate::goldrun::Phase::ToHall(hall) => self.unit(hall).and_then(|v| v.extra.object_slots[0].first().copied()),
                    _ => None,
                });
                let to = to.map_or(NONE, rf);
                own.extend([w(to.0), w(to.1), w(0), Field::Real(o.real("spd")), Field::Real(factor), w(0), w(0), w(u32::MAX), w(u32::MAX), w(0), Field::Real(0.0), Field::Real(0.0), w(u32::MAX), w(u32::MAX), w(0), w(0)]);
                vec![timer(), timer()]
            }
            // CAbilityInvulnerable: a word; an empty list and a word.
            codes::ability::id::INVULNERABLE => {
                own.push(w(0));
                vec![Field::List(Vec::new()), w(0)]
            }
            // Regeneration on blight: its look at the ground every 2 seconds (event
            // 0xd01c1).
            codes::ability::id::REGENERATE_ON_BLIGHT => vec![periodic("436780", 2.0, 0xd_01c1)],
            // Passives with an order (spell immunity, a creep's poison, detection): its
            // order (852290) and words; detection its own order too.
            codes::ability::id::VENOM_SPEARS | codes::ability::id::MAGIC_IMMUNITY | codes::ability::id::NEUTRAL_DETECTION_REVEAL_ABILITY => {
                match code {
                    codes::ability::id::VENOM_SPEARS => own.push(w(0)),
                    codes::ability::id::NEUTRAL_DETECTION_REVEAL_ABILITY => own.push(w(852_630)),
                    _ => {}
                }
                vec![w(852_290), w(0), w(0), w(0), w(0)]
            }
            // A shop's (Aneu): its customers' (twelve players' none), its listener.
            codes::ability::id::NEUTRAL_BUILDING => {
                let pil = u.extra.object_slots[2].iter().map(|&(s, _)| s).skip_while(|&s| s != t.0).find(|&s| self.tags.live.get(&s).is_some_and(|e| &e.0 == codes::class::IN_RANGE_LISTENER)).map_or(NONE, |s| rf(self.tagged(s)));
                vec![Field::List((0..12).map(|_| vec![Field::Ref(NONE)]).collect()), Field::Ref(NONE), Field::List((0..12).map(|_| vec![w(u32::MAX), w(u32::MAX), w(0)]).collect()), Field::Ref(pil), timer()]
            }
            codes::ability::id::RETURN => Vec::new(),
            // A graveyard's (CAbilityGraveyard): its ten corpse places empty, its Cool,
            // DataC, and DataA, its ten corpses' units, no timer (read: Forestwalk's
            // undead computer's at 160 seconds, 15, 250, 5, five corpses).  Without the
            // record the original's loader read past it and hung.  Provisional: its
            // corpses, none kept yet.
            codes::ability::id::GRAVEYARD => {
                let o = self.objects.abilities.get(*codes::ability::id::GRAVEYARD);
                let data = |f: &str| o.map_or(0.0, |o| o.level_real(f, 1));
                let mut tail: Vec<Field> = (0..10).map(|_| Field::List(vec![vec![Field::Ref(NONE), w(0)]])).collect();
                tail.extend([Field::Real(o.map_or(0.0, |o| o.level_real("Cool", 1))), Field::Real(data("DataC")), w(data("DataA") as u32), Field::List((0..10).map(|_| vec![Field::Ref(NONE)]).collect()), timer()]);
                tail
            }
            // A hero's (CAbilityHero, read against Forestwalk's undead computer's
            // Dreadlord at 160 seconds): 0, its skill points (+0x90, as
            // GetHeroSkillPoints reads), its strength, then its strength, the
            // life it gives (25 a point), the armor its agility gives (0.3 a point,
            // less 2), the mana its intelligence gives (15 a point), 0, the attack
            // speed its agility gives (0.02 a point); its agility, its strength's,
            // intelligence's, and agility's gains a level; its level, 0x1000a, 0; its
            // skills, their levels most, and the levels they need (five places); its
            // strength's and intelligence's regeneration modifiers (`hero_parts`), and
            // its place among its owner's heroes; its level and intelligence are its
            // flags.  Provisional: the meaning of most.  Without the record the
            // original's loader read past it and hung.
            codes::ability::id::HERO => {
                let ob = o;
                let hero_rcms = self.hero_parts(h).map_or([None, None], |p| p.2);
                // (Its place among its owner's heroes as made, the player's count then:
                // `CAbilityHero::vf188`, the player's +0x334; provisional, from the
                // heroes it still has.)
                let birth = |v: u32| self.unit(v).and_then(|w| w.extra.object_slots[0].first().map(|f| f.1)).unwrap_or(0);
                let ordinal = self.unit_order.iter().filter(|&&v| self.unit(v).is_some_and(|w| w.is_hero && w.owner == u.owner) && birth(v) <= birth(h)).count() as u32;
                let (s, a, i) = (u.str_.floor(), u.agi.floor(), u.int.floor());
                let skills: Vec<crate::objects::Id> = ob.list("heroAbilList").iter().map(|x| crate::objects::id_from_str(x.trim())).take(5).collect();
                let at = |k: usize, f: &dyn Fn(crate::objects::Id) -> u32| vec![w(skills.get(k).map_or(0, |&id| f(id)))];
                let levels = |id: crate::objects::Id| self.objects.abilities.get(id).map_or(0, |o| o.int("levels").max(0) as u32);
                let needs = |id: crate::objects::Id| self.objects.abilities.get(id).map_or(0, |o| o.int("reqLevel").max(1) as u32);
                vec![
                    w(0), w(u.skill_points.max(0) as u32), w(s as u32),
                    Field::Real(s), Field::Real(s * 25.0), Field::Real(a * 0.3 - 2.0), Field::Real(i * 15.0), Field::Real(0.0), Field::Real(a * self.misc.get_str("Misc", "AgiAttackSpeedBonus").and_then(|v| crate::objects::engine_real(v.trim())).unwrap_or(0.02)),
                    w(a as u32), Field::Real(ob.real("STRplus")), Field::Real(ob.real("INTplus")), Field::Real(ob.real("AGIplus")),
                    w(u.level.max(1) as u32), w(0x1_000a), w(0),
                    Field::List((0..5).map(|k| at(k, &|id| u32::from_be_bytes(id))).collect()),
                    Field::List((0..5).map(|k| at(k, &levels)).collect()),
                    Field::List((0..5).map(|k| at(k, &needs)).collect()),
                    // (Its strength's and intelligence's regeneration modifiers.)
                    Field::Ref(hero_rcms[0].map_or(NONE, rf)), Field::Ref(hero_rcms[1].map_or(NONE, rf)), w(ordinal),
                ]
            }
            // Rally: no point set; its rally point (the second table's).
            codes::ability::id::RALLY => {
                own.extend([w(u32::MAX), w(u32::MAX), w(0), Field::Real(0.0), Field::Real(0.0), w(0), w(0), w(0), w(0)]);
                let me = u.extra.object_slots[0].first().copied().unwrap_or_default();
                let r = rallies.iter().find(|(_, owner)| *owner == rf(me)).map_or(NONE, |x| x.0);
                vec![Field::List(vec![vec![Field::Ref(r)]])]
            }
            codes::ability::id::MILITIA_CONVERSION => vec![timer()],
            // Militia: the unit's type and the militia's (its DataB).
            codes::ability::id::MILITIA => {
                let to = u.abilities.iter().filter_map(|(a, _)| self.objects.abilities.get(*a)).find(|ab| ab.str("code").trim() == codes::ability::MILITIA).map_or([0; 4], |ab| crate::objects::id_from_str(ab.level_str("DataB", 1).trim()));
                own.extend([w(u32::from_be_bytes(u.type_id)), w(u32::from_be_bytes(to))]);
                Vec::new()
            }
            codes::ability::id::UPGRADE_STRUCTURE => {
                own.extend([Field::Real(0.0), Field::Real(0.0)]);
                vec![Field::Ref(NONE), Field::Ref(NONE)]
            }
            // A queue (Aque): idle, its seven places empty.
            codes::ability::id::QUEUE => {
                own.extend([Field::Real(0.0), Field::Real(0.0), w(0), Field::List((0..7).map(|_| vec![w(0)]).collect()), Field::List((0..7).map(|_| vec![w(u32::MAX), w(u32::MAX), w(0)]).collect()), Field::List((0..7).map(|_| vec![w(0)]).collect())]);
                vec![Field::Ref(NONE), Field::Ref(NONE), Field::Ref(NONE), Field::Ref(NONE)]
            }
            // Spells (CAbilitySpell): its cooldown left, its unit's cast point and
            // backswing (a repair's none and the largest real), references; its order
            // (852290), words, its listener on the unit's mana, two timers; an
            // auto-target spell's look every 3 seconds (event 0xd01c2), a repair's more.
            codes::ability::id::THUNDER_BOLT_CREEP | codes::ability::id::THUNDER_CLAP_CREEP | codes::ability::id::CHIEFTAIN_SHOCK_WAVE | codes::ability::id::ENSNARE | codes::ability::id::CANNIBALIZE | codes::ability::id::SACRIFICE_ACOLYTE | codes::ability::id::UNSUMMON | codes::ability::id::HEAL_CREEP_NORMAL | codes::ability::id::ABOLISH_MAGIC | codes::ability::id::INNER_FIRE | codes::ability::id::RESTORATION | codes::ability::id::REPAIR_ORC | codes::ability::id::CURSE | codes::ability::id::BLOODLUST | codes::ability::id::FAERIE_FIRE | codes::ability::id::WEB | codes::ability::id::RAISE_DEAD | codes::ability::id::ANIMATE_DEAD_CREEP | codes::ability::id::DEMON_HUNTER_MANA_BURN | codes::ability::id::DEVOUR_DRAGON_CREEP | codes::ability::id::DREADLORD_SLEEP | codes::ability::id::REJUVINATION | codes::ability::id::DEATH_KNIGHT_DEATH_COIL | codes::ability::id::EXHUME | codes::ability::id::MEAT_LOAD | codes::ability::id::MEAT_DROP | codes::ability::id::CRIPPLE | codes::ability::id::UNHOLY_FRENZY => {
                // (Restore as Repair.)
                let repair = matches!(code, codes::ability::id::REPAIR_ORC | codes::ability::id::RESTORATION);
                // (Devour's backswing its own, 0.02: read, Forestwalk's drakes'.)
                let (point, back) = if repair {
                    (0.0, f32::MAX)
                } else if code == codes::ability::id::DEVOUR_DRAGON_CREEP {
                    (o.real("castpt"), 0.02)
                } else {
                    (o.real("castpt"), o.real("castbsw"))
                };
                own.extend([Field::Real(0.0), Field::Real(point), Field::Real(back), Field::Real(0.0), w(u32::MAX), w(u32::MAX), w(0), w(0), Field::Real(0.0), w(u32::MAX), w(u32::MAX), w(0), w(0), Field::Real(0.0), Field::Real(0.0), w(0), w(0), w(0), w(0), Field::Real(0.0), Field::Real(0.0)]);
                if repair {
                    own.extend([w(u32::MAX), w(u32::MAX), w(0), Field::Real(0.0), Field::Real(0.0), Field::Real(0.0), w(u32::MAX), w(u32::MAX), w(0), Field::Real(0.0), w(u32::MAX), w(u32::MAX), w(0)]);
                }
                // (Meat Drop's the spell's first nine alone.)
                if code == codes::ability::id::MEAT_DROP {
                    own.truncate(own.len() - 12);
                }
                // Devour's word, Mana Burn's none and 0.0, Raise Dead's two target masks
                // (read, Forestwalk's creeps').
                match code {
                    codes::ability::id::DEVOUR_DRAGON_CREEP => own.push(w(0)),
                    codes::ability::id::DEMON_HUNTER_MANA_BURN => own.extend([w(u32::MAX), w(u32::MAX), Field::Real(0.0)]),
                    codes::ability::id::RAISE_DEAD => own.extend([w(0xfed1_f00e), w(0xfdd1_f006)]),
                    _ => {}
                }
                // (One of its own listeners, whose records are written with the unit's.)
                let own_listener = |s: u32| u.extra.mana_listeners.iter().any(|l| l.1 == s) || u.extra.spell_timers.contains(&s) || u.extra.object_slots[2].iter().any(|x| x.0 == s);
                let listener = u.extra.spell_owners.iter().find(|x| &x.1 == code && own_listener(x.0) && self.tags.live.get(&x.0).is_some_and(|e| &e.0 == codes::class::THRESHOLD_LISTENER)).map_or(NONE, |x| rf(self.tagged(x.0)));
                let mut tail = vec![w(852_290), w(0), w(0), w(0), w(0), Field::Ref(listener), timer(), timer()];
                if matches!(code, codes::ability::id::HEAL_CREEP_NORMAL | codes::ability::id::ABOLISH_MAGIC | codes::ability::id::INNER_FIRE | codes::ability::id::RESTORATION | codes::ability::id::REPAIR_ORC | codes::ability::id::CURSE | codes::ability::id::BLOODLUST | codes::ability::id::FAERIE_FIRE | codes::ability::id::WEB | codes::ability::id::RAISE_DEAD | codes::ability::id::MEAT_LOAD) {
                    tail.push(match self.tags.autotarget_timers.get(&(h, *code)) {
                        Some(&(count, due, period)) if due > self.time - 0.0205 => Field::Timer(rf(t), 0, Timing(Some((due, period, count, 0xd_01c2, 0)))),
                        _ => timer(),
                    });
                }
                // Exhume's corpse every Dur seconds from its making (event 0xd01c3,
                // repeating; read, a Meat Wagon's at 600 seconds; provisional, its count).
                if code == codes::ability::id::EXHUME {
                    let period = u.abilities.iter().filter_map(|(a, _)| self.objects.abilities.get(*a)).find(|ab| ab.str("code").trim().as_bytes() == code).map_or(0.0, |ab| ab.level_real("Dur", 1));
                    tail.push(match self.made_abs(t.0) {
                        Some(made) if period > 0.0 => {
                            let mut due = crate::efloat::add(made, period.to_bits());
                            while f32::from_bits(due) <= self.time {
                                due = crate::efloat::add(due, period.to_bits());
                            }
                            Field::Timer(rf(t), 2, Timing(Some((f32::from_bits(due), period, 0, 0xd_01c3, 1))))
                        }
                        _ => timer(),
                    });
                }
                // Animate Dead's and Mana Burn's look every 2 seconds (event 0xd01bf);
                // Devour's third timer and Raise Dead's fourth (none: read, at rest).
                match code {
                    codes::ability::id::ANIMATE_DEAD_CREEP => tail.push(periodic("6060e0", 2.0, 0xd_01bf)),
                    codes::ability::id::DEMON_HUNTER_MANA_BURN => tail.push(periodic("606c60", 2.0, 0xd_01bf)),
                    codes::ability::id::DEVOUR_DRAGON_CREEP | codes::ability::id::RAISE_DEAD => tail.push(timer()),
                    _ => {}
                }
                if repair {
                    tail.extend([timer(), timer(), timer(), timer(), Field::Ref(NONE)]);
                }
                // Ensnare's in-range listener.
                if code == codes::ability::id::ENSNARE {
                    tail.push(Field::Ref(u.extra.ensnare_pil.map_or(NONE, |p| rf(self.tagged(p.1)))));
                }
                // Cannibalize's look for corpses every 2 seconds (event 0xd01bf).
                if code == codes::ability::id::CANNIBALIZE {
                    // (A reference and two target masks: read, a ghoul's.)
                    own.extend([w(u32::MAX), w(u32::MAX), w(0), w(0xf6d1_f002), w(0xfdd1_f006)]);
                    tail.extend([Field::Ref(NONE), Field::Ref(NONE), periodic("606220", 2.0, 0xd_01bf)]);
                }
                tail
            }
            // Building: nothing begun; but the humans' one more reference (read: the
            // orcs', night elves', and neutral's as the undead's), the undead's a short of
            // 1.
            codes::ability::id::BUILD_HUMAN | codes::ability::id::BUILD_UNDEAD | codes::ability::id::BUILD_ORC | codes::ability::id::BUILD_NIGHT_ELF | codes::ability::id::BUILD_NEUTRAL => {
                own.extend([Field::Real(0.0), Field::Real(0.0), w(0), w(0), w(0)]);
                let undead = code == codes::ability::id::BUILD_UNDEAD;
                if code != codes::ability::id::BUILD_HUMAN {
                    own.extend([w(u32::MAX), w(u32::MAX), w(0)]);
                }
                vec![Field::Ref(NONE), Field::Ref(NONE), w(0), Field::Short(undead as u16), Field::List(Vec::new())]
            }
            // Harvest (gold and lumber, or lumber): its DataC and DataB, its unit's
            // attack's point and backswing, its DataA; eight references and timers
            // empty, its area, its timers.
            // On a gold run (read: Forestwalk's peasants at 12 seconds): its hall, its
            // run's speed bonus, last travel time, and bonus before, its mine, and its
            // round trip's timer (10 seconds, event -1); its flags 0x40000000 once it
            // has been in, 0x1, and 0x800 walking or 0x9000 inside.
            codes::ability::id::HARVEST | codes::ability::id::HARVEST_LUMBER => {
                let ab = u.abilities.iter().filter_map(|(a, _)| self.objects.abilities.get(*a)).find(|ab| ab.str("code").trim().as_bytes() == code);
                let data = |f: &str| ab.map_or(0.0, |ab| ab.level_real(f, 1));
                let gold = code == codes::ability::id::HARVEST;
                let run = u.extra.gold_run.as_ref().filter(|r| gold && !r.lumber);
                // (The hall it walks to, else the last it took a load to, none before.)
                let hall = run.and_then(|r| match r.phase {
                    crate::goldrun::Phase::ToHall(hall) => Some(hall),
                    _ => r.hall,
                });
                let hall = hall.and_then(|v| self.unit(v)).and_then(|v| v.extra.object_slots[0].first().copied()).map_or(NONE, rf);
                // (Its gold carried the fifth: read, 10 on its way home, none yet in the
                // mine.)
                let in_mine = run.is_some_and(|r| matches!(r.phase, crate::goldrun::Phase::InMine(_)));
                // (On a lumber run its lumber: read, 10 on its way home.)
                let lumber_run = u.extra.gold_run.as_ref().is_some_and(|r| r.lumber);
                let carried = if lumber_run { u.carry_lumber.max(0) as u32 } else if gold && !in_mine { u.carry_gold.max(0) as u32 } else { 0 };
                own.extend([w(data("DataC") as u32), w(data("DataB") as u32), w(0), w(0), w(carried), w(hall.0), w(hall.1), w(0), Field::Real(o.real("dmgpt1")), Field::Real(o.real("backSw1")), w(data("DataA") as u32)]);
                // (Chopping, the run's lumber: 2 third, its load fifth (each chop's lumber
                // as it lands), its tree twice in its tail, its next chop's damage point
                // on its timer, its flags 0x40003185 more: read, Forestwalk's peasants
                // chopping.)
                let chop = u.extra.gold_run.as_ref().filter(|r| r.lumber && matches!(r.phase, crate::goldrun::Phase::Chopping(_)));
                // (Walking to its tree or home with its lumber: 0x40000981 more, its tree
                // twice, no timer: read.)
                let to_tree = u.extra.gold_run.as_ref().filter(|r| r.lumber && matches!(r.phase, crate::goldrun::Phase::ToMine | crate::goldrun::Phase::ToHall(_)));
                if to_tree.is_some() {
                    if let Some(Field::Word(flags)) = own.first_mut() {
                        *flags |= 0x4000_0981;
                    }
                }
                if chop.is_some() {
                    let n = own.len();
                    own[n - 9] = w(2);
                    let chopped = chop.map_or(0, |r| self.chops_done(h, r.chop_start));
                    let per = self.run_harvest(h).map_or(1, |(a, l)| self.ability_real(a, "DataA", l).max(1.0) as i32);
                    own[n - 7] = w((u.carry_lumber + chopped as i32 * per).max(0) as u32);
                    // (In its backswing, 0x4000 more: read, Forestwalk's peasants at 12, 15,
                    // and 19 seconds.)
                    let backswing = chop.and_then(|r| self.next_chop(h, r.chop_start)).is_some_and(|c| c.2 == 0xd_01b2);
                    if let Some(Field::Word(flags)) = own.first_mut() {
                        *flags |= 0x4000_3185 | if backswing { 0x4000 } else { 0 };
                    }
                }
                if gold {
                    let f = f32::from_bits;
                    own.extend(match run {
                        Some(r) => [Field::Real(f(r.bonus)), Field::Real(f(r.travel)), Field::Real(f(r.bonus_before))],
                        None => [Field::Real(0.0), Field::Real(0.0), Field::Real(0.0)],
                    });
                }
                if let Some(r) = run {
                    // (Waiting at the mine as inside it.)
                    let inside = matches!(r.phase, crate::goldrun::Phase::InMine(_) | crate::goldrun::Phase::Waiting(_));
                    if let Some(Field::Word(flags)) = own.first_mut() {
                        *flags |= 0x1 | if r.entered { 0x4000_0000 } else { 0 } | if inside { 0x9000 } else { 0x800 };
                    }
                }
                let mine = run.and_then(|r| self.unit(r.mine)).and_then(|m| m.extra.object_slots[0].first().copied()).map_or(NONE, rf);
                let tree = chop.or(to_tree).and_then(|r| match self.handles.get(r.mine) {
                    Some(crate::types::Obj::Destructable(d)) => Some(rf(self.tagged(d.oid.0))),
                    _ => None,
                });
                let (named, at) = match tree {
                    Some(t) => (t, t),
                    None => (NONE, mine),
                };
                let mut tail = vec![Field::List((0..8).map(|_| vec![w(u32::MAX), w(u32::MAX), w(0)]).collect()), Field::List((0..8).map(|_| vec![timer()]).collect()), w(named.0), w(named.1), w(0), w(at.0), w(at.1), w(0), Field::Real(data("Area"))];
                tail.push(timer());
                // (Its swing's part now, from its swings' start.)
                tail.push(match chop.and_then(|r| self.next_chop(h, r.chop_start)) {
                    Some((due, len, event)) => Field::Timer(rf(t), 0, Timing(Some((f32::from_bits(due), f32::from_bits(len), 0, event, 0)))),
                    None => timer(),
                });
                tail.push(timer());
                if gold {
                    tail.push(match run.and_then(|r| r.timer_due) {
                        Some(due) => Field::Timer(NONE, 0, Timing(Some((f32::from_bits(due), 10.0, 0, u32::MAX, 0)))),
                        None => timer(),
                    });
                }
                tail
            }
            // A haunted mine's gathering (Aaha): its range, a reference; a timer.
            codes::ability::id::ACOLYTE_HARVEST => {
                let ab = u.abilities.iter().filter_map(|(a, _)| self.objects.abilities.get(*a)).find(|ab| ab.str("code").trim() == codes::ability::ACOLYTE_HARVEST);
                own.extend([Field::Real(ab.map_or(0.0, |ab| ab.level_real("Rng", 1))), w(u32::MAX), w(u32::MAX), w(0)]);
                vec![timer()]
            }
            // A gold mine (Agld): its gold, 1.0, 1, references, 15 and 2.0, the haunted
            // mine over it; its shaft (`shft`).
            codes::ability::id::GOLD_MINE => {
                let over = self.unit_order.iter().copied().find(|&v| v != h && self.unit(v).is_some_and(|w| !w.dead && w.abilities.iter().any(|(a, _)| a == codes::ability::id::BLIGHTED_GOLD_MINE) && (w.x - u.x).abs() < 1.0 && (w.y - u.y).abs() < 1.0)).and_then(|v| self.unit(v)).and_then(|w| w.extra.object_slots[0].first().copied());
                let over = over.map_or(NONE, rf);
                // (Once a worker has been in, 1, the player it shows the mine to, and its
                // fog modifier, `+fgm`: read, Forestwalk's human peasants' mine.)
                let fog = u.extra.mine_fog.filter(|(s, _)| self.tags.live.get(s).is_some_and(|e| &e.0 == codes::class::FOG_MODIFIER));
                let fog_ref = fog.map_or(NONE, |(s, _)| rf(self.tagged(s)));
                // (Its queue's first and last waiting worker's places.)
                let queue: Vec<(u32, u32)> = u.extra.mine_queue.iter().filter_map(|&v| self.saved_order(v).and_then(|o| o.queue)).collect();
                let (first_q, last_q) = (queue.first().map_or(NONE, |&q| rf(q)), queue.last().map_or(NONE, |&q| rf(q)));
                own.extend([w(u.resource.max(0) as u32), Field::Real(1.0), w(1), w(fog.is_some() as u32), w(first_q.0), w(first_q.1), w(0), w(last_q.0), w(last_q.1), w(0), w(fog.map_or(15, |f| f.1 as u32)), Field::Real(2.0), w(over.0), w(over.1), w(0), w(fog_ref.0), w(fog_ref.1), w(0)]);
                let shaft = u.extra.object_slots[2].iter().map(|&(s, _)| s).skip_while(|&s| s != t.0).find(|&s| self.tags.live.get(&s).is_some_and(|e| &e.0 == codes::class::MINE_SHAFT)).map_or(NONE, |s| rf(self.tagged(s)));
                vec![Field::Ref(NONE), Field::List(vec![vec![Field::Ref(shaft)]]), timer()]
            }
            // Creep sleep (`FUN_6f5224c0`, `FUN_6f5230a0`): 0.1; its sleep's timer, its
            // check every 2 seconds (event 0xd01bf), and a list.
            codes::ability::id::CREEP_SLEEP => {
                own.push(Field::Real(0.1));
                // (The sleep's timer, as it fell asleep, else its check's: read, the
                // count of the night's creeps' second.)
                let sleep = periodic("517be0", 2.0, 0xd_01bf);
                vec![timer(), if matches!(sleep, Field::Timer(r, ..) if r != NONE) { sleep } else { periodic("524e00", 2.0, 0xd_01bf) }, Field::List(Vec::new())]
            }
            // Fire (CAbilityOnFire): a reference, and its look (event 0xd01c1) every 1.5
            // seconds and a two-thousandth of its maximum life, at most 3
            // (`FUN_6f41baf0`: read, a town hall's 2.25, a haunted mine's 1.975).
            codes::ability::id::ON_FIRE_HUMAN | codes::ability::id::ON_FIRE_UNDEAD | codes::ability::id::ON_FIRE_ORC | codes::ability::id::ON_FIRE_NIGHT_ELF => {
                use crate::efloat::{add, div, from_int, mul};
                let period = f32::from_bits(add(mul(u.max_life.to_bits(), div(from_int(1), from_int(2000))), div(from_int(3), from_int(2)))).clamp(1.5, 3.0);
                vec![w(u32::MAX), w(u32::MAX), w(0), periodic("41baf0", period, 0xd_01c1)]
            }
            // Wander (CAbilityWander): its wait (event 0xd01bf), due at its start and
            // the wait drawn, once.
            codes::ability::id::WANDER => {
                let timer = match u.extra.wander_timer {
                    Some((count, at, wait)) => Field::Timer(rf(t), 0, Timing(Some((f32::from_bits(crate::efloat::add(at.to_bits(), wait.to_bits())), wait, count, 0xd_01bf, 0)))),
                    None => timer(),
                };
                vec![timer]
            }
            // CAbilityAttack (`FUN_6f49bd60`): its weapons' data, its acquisition range,
            // its place, and its two timers.
            codes::ability::id::ATTACK => return Some(self.attack_record(h, t, own)),
            // CAbilityAlarm (`FUN_6f42d040`): its timer and its in-range listener.
            codes::ability::id::ALARM => vec![timer(), Field::Ref(u.extra.alarm_pil.map_or(NONE, |s| rf(self.tagged(s))))],
            // Sell Units (CAbilitySellUnit, own `FUN_6f42e970`): two 0.0; two references
            // (none), twelve slots, each a sold unit's (its id, its count, 0.0, and its
            // most as reals, its restock, and its first coming: extended timers, the
            // coming's its stockStart from the shop's making in chunks, `timer_chunks`,
            // event 0xd01c1; 1 while it is yet to come) or empty (0, 0.0, 0.0, the
            // largest real, no timers, 0); then 0xd0142 and four 0.  (Read: the quiet
            // test map's Goblin Laboratory's three at 5 seconds; provisional: a slot
            // after its unit has come, its restock running.)
            codes::ability::id::SELL_UNITS => {
                own.extend([Field::Real(0.0), Field::Real(0.0)]);
                let (_, units) = self.shop_goods(h);
                let starts: Vec<(u32, f32)> = self.tags.timer_objects.get(&t.0).map(|v| v.iter().filter(|x| x.1 == "41c010").map(|x| (x.0, x.2)).collect()).unwrap_or_default();
                let idle = || [timer(), Field::Short(0), Field::Short(0), Field::Real(0.0)];
                let mut tail = vec![Field::Ref(NONE), Field::Ref(NONE)];
                for k in 0..12 {
                    let mut slot = Vec::new();
                    match units.get(k) {
                        Some(&id) => {
                            let start = self.objects.units.get(id).map_or(0.0, |o| o.real("stockStart"));
                            let (count, max, arriving) = self.shops.stock.get(&(h, id)).map_or((0, 1, start > 0.0), |s| (s.count, s.max, s.arriving));
                            slot.extend([w(u32::from_be_bytes(id)), Field::Real(count as f32), Field::Real(0.0), Field::Real(max as f32)]);
                            slot.extend(idle());
                            if arriving {
                                let (chunks, rest) = super::timer_chunks(start);
                                let first = if chunks > 0 { super::TIMER_CHUNK } else { start };
                                let (n, at) = starts.get(k).copied().unwrap_or((0, 0.0));
                                let due = f32::from_bits(crate::efloat::add(at.to_bits(), first.to_bits()));
                                slot.push(Field::Timer(rf(t), if chunks > 0 { 0x10 } else { 0 }, Timing(Some((due, first, n, 0xd_01c1, u32::from(chunks > 0))))));
                                slot.extend([Field::Short(chunks.min(0xffff) as u16), Field::Short(chunks.min(0xffff) as u16), Field::Real(rest)]);
                            } else {
                                slot.extend(idle());
                            }
                            slot.push(w(u32::from(arriving)));
                        }
                        None => {
                            slot.extend([w(0), Field::Real(0.0), Field::Real(0.0), Field::Real(f32::MAX)]);
                            slot.extend(idle());
                            slot.extend(idle());
                            slot.push(w(0));
                        }
                    }
                    tail.push(Field::List(vec![slot]));
                }
                tail.extend([w(0xd_0142), w(0), w(0), w(0), w(0)]);
                tail
            }
            // Blight growth (CAbilityBlightGrowth): its Area as given, its Xbli, 0.
            codes::ability::id::BLIGHT_GROWTH => {
                let xbli = u.extra.blight_slots.get(1).map_or((u32::MAX, u32::MAX), |&s| self.tagged(s));
                let area = self.ability_with_code(h, codes::ability::BLIGHT_GROWTH).and_then(|(a, l)| self.objects.abilities.get(a).map(|o| o.level_real("Area", l))).unwrap_or(0.0);
                own.extend([Field::Real(area), w(xbli.0), w(xbli.1), w(0)]);
                Vec::new()
            }
            // An item's Town Portal (read: the melee start's scroll on Forestwalk's
            // computer's Dreadlord): its casting time, three 0, its item, three 0, none;
            // ten 0, 852290, four 0, its listener on mana, two timers, three none.
            codes::ability::id::ITEM_TOWN_PORTAL => {
                let held = self.held_item_objects(h);
                let k = held.iter().position(|x| x.0 == t)?;
                let item = self.item(held[k].2)?;
                let rtl = held[k + 1..].iter().take_while(|x| x.1[0] != b'A' && x.2 == held[k].2).find(|x| &x.1 == codes::class::THRESHOLD_LISTENER).map_or(NONE, |x| rf(x.0));
                let cast = self.objects.abilities.get(*code).map_or(0.0, |a| a.level_real("Cast", 1));
                own.extend([Field::Real(cast), w(0), w(0), w(0), Field::Ref(rf(item.oid)), w(0), w(0), w(0), Field::Ref(NONE)]);
                let mut tail: Vec<Field> = (0..10).map(|_| w(0)).collect();
                tail.extend([w(852_290), w(0), w(0), w(0), w(0), Field::Ref(rtl), timer(), timer(), Field::Ref(NONE), Field::Ref(NONE), Field::Ref(NONE)]);
                tail
            }
            // Build In Progress (read: Forestwalk's undead computer's Crypt and Graveyard):
            // 1.0, its build time, its builder's build ability, 0, eight 0.0; its rise's
            // listener (keyed to its event 0xd01c4) and modifier, 60.0, its life's
            // modifier, two timers.
            codes::ability::id::BUILD_IN_PROGRESS => {
                let p = self.build_parts(h)?;
                let by = u.extra.build_by.unwrap_or((u32::MAX, u32::MAX));
                own.extend([Field::Real(1.0), Field::Real(u.build_time), w(by.0), w(by.1), w(0)]);
                own.extend((0..8).map(|_| Field::Real(0.0)));
                keyed.push((p[3].0, vec![(rf(t), 0xd_01c4)]));
                vec![Field::Ref(rf(p[3])), Field::Ref(rf(p[2])), w(60f32.to_bits()), Field::Ref(rf(p[4])), timer(), timer()]
            }
            // A haunted mine's (CAbilityBlightedGoldMine): its gold, the hidden gold mine,
            // 0.0, DataA, DataB, its payout period (DataB over the share of its ring
            // slots at work, by the engine's division: `FUN_6f4e8c30`; read, 2.5000002
            // with two of five), DataC, the acolytes at work; none, its payout's
            // modifier and threshold listener (that keyed to its event 0xd01c4), no
            // timer, its work slots.
            codes::ability::id::BLIGHTED_GOLD_MINE => {
                let (a, l) = self.ability_with_code(h, codes::ability::BLIGHTED_GOLD_MINE)?;
                let d = self.objects.abilities.get(a)?;
                let hidden = u.extra.mine.and_then(|m| self.unit(m)).and_then(|m| m.extra.object_slots[0].first().copied()).unwrap_or((u32::MAX, u32::MAX));
                let working = u.extra.payout.map_or(0, |p| p.1);
                let data_b = d.level_real("DataB", l);
                let period = if working == 0 {
                    0.0
                } else {
                    use crate::efloat as ef;
                    let share = ef::div(ef::from_int(working as i32), ef::from_int(self.mine_slots_pub(h).len().max(1) as i32));
                    f32::from_bits(ef::div(data_b.to_bits(), share))
                };
                own.extend([w(u.resource as u32), w(hidden.0), w(hidden.1), Field::Real(0.0), w(d.level_real("DataA", l) as u32), Field::Real(data_b), Field::Real(period), w(d.level_real("DataC", l) as u32), w(working)]);
                let objs = &u.extra.object_slots[2];
                let i = objs.iter().position(|&x| x == t)?;
                let slots = objs.get(i + 3..i + 8).map(|v| v.iter().map(|&r| vec![Field::Ref(rf(r))]).collect()).unwrap_or_default();
                let (rcm, rtl) = match u.extra.ai_order.as_slice() {
                    &[m, l] => (rf(self.tagged(m)), rf(self.tagged(l))),
                    _ => (NONE, NONE),
                };
                if rtl != NONE {
                    keyed.push((rtl.0, vec![(Ref(t.0, t.1), 0xd_01c4)]));
                }
                vec![Field::Ref(NONE), Field::Ref(rcm), Field::Ref(rtl), timer(), Field::List(slots)]
            }
            // An inventory (CAbilityInventory): its six slots, 0, its pickup listener
            // (`` `pil ``), six 0, six timers (none).  (Provisional: items' cooldowns.)
            codes::ability::id::INVENTORY => {
                let pil = u.extra.item_pil.map_or(NONE, rf);
                // (Each slot its item, read: the melee start's scroll in the first.)
                let slots = u.inventory.iter().map(|&i| match self.item(i).map(|it| it.oid).filter(|o| self.tags.live.get(&o.0).is_some_and(|e| e.1 == o.1)) {
                    Some(o) => vec![w(o.0), w(o.1), w(0)],
                    None => vec![w(u32::MAX), w(u32::MAX), w(0)],
                }).collect();
                vec![Field::List(slots), w(0), Field::Ref(pil), Field::List((0..6).map(|_| vec![w(0)]).collect()), Field::List((0..6).map(|_| vec![timer()]).collect())]
            }
            // An aura (CAbilityAura): 0, its Area and DataA; its art on the unit (the
            // second table's, as `art` has it), its owner, its range listener (`` `pil ``
            // after it), its refresh every 2 seconds, 0xd0142, and four 0.
            codes::ability::id::PRIESTESS_TRUESHOT_AURA | codes::ability::id::PALADIN_DEVOTION_AURA | codes::ability::id::MANA_REGENERATION_AURA | codes::ability::id::ARCH_MAGE_BRILLIANCE_AURA | codes::ability::id::DEATH_KNIGHT_UNHOLY_AURA | codes::ability::id::CHIEFTAIN_ENDURANCE_AURA | codes::ability::id::DREADLORD_VAMPIRIC_AURA | codes::ability::id::KEEPER_THORNS_AURA | codes::ability::id::AURA_WAR_DRUMS | codes::ability::id::COMMAND_AURA | codes::ability::id::AURA_REGENERATION_WARD => {
                let (a, l) = u.abilities.iter().copied().find(|(a, _)| self.objects.abilities.get(*a).is_some_and(|o| crate::objects::id_from_str(o.str("code").trim()) == *code))?;
                let d = self.objects.abilities.get(a)?;
                own.extend([w(0), Field::Real(d.level_real("Area", l.max(1) as u32)), Field::Real(d.level_real("DataA", l.max(1) as u32))]);
                let objs = &u.extra.object_slots[2];
                let pil = objs.iter().position(|&x| x == t).and_then(|i| objs.get(i + 1)).map_or(NONE, |&r| rf(r));
                vec![art.get(&t.0).cloned().unwrap_or(Field::List(Vec::new())), w(u.owner as u32), Field::Ref(pil), periodic("40b5e0", 2.0, 0xd_01c1), w(0xd_0142), w(0), w(0), w(0), w(0)]
            }
            // Shadowmeld (CAbilityShadowMeld): its DataC, DataA, and DataB (read, 0.5,
            // 1.5000001 as the data's 1.5 is read, 2.5); its order, words, and two timers.
            // By day its third count is 1 (read, at 5 seconds; provisional: as the day
            // begins).
            codes::ability::id::SHADOW_MELD => {
                let (a, l) = self.ability_with_code(h, codes::ability::SHADOW_MELD)?;
                let d = self.objects.abilities.get(a)?;
                if !self.is_night() {
                    own[6] = w(1);
                }
                own.extend([Field::Real(d.level_real("DataC", l)), Field::Real(d.level_real("DataA", l)), Field::Real(d.level_real("DataB", l))]);
                vec![w(852_290), w(0), w(0), w(0), w(0), timer(), timer()]
            }
            // Burrow detection (CAbilityBurrowDetector): its range and 12 (provisional);
            // its order, words, none, and its in-range object (`` `pil ``).
            codes::ability::id::BURROW_DETECTION_FLYERS => {
                let range = self.ability_with_code(h, codes::ability::BURROW_DETECTION_FLYERS).map_or(900.0, |(a, l)| self.ability_real(a, "Rng", l));
                own.extend([Field::Real(range), w(12)]);
                let pil = u.extra.flyer_pil.map_or(NONE, |s| rf(self.tagged(s)));
                vec![w(852_290), w(0), w(0), w(0), w(0), Field::Ref(NONE), Field::Ref(pil)]
            }
            // Critical Strike: 1 (provisional); its order and words.
            codes::ability::id::BLADE_MASTER_CRITICAL_STRIKE => {
                own.push(w(1));
                vec![w(852_290), w(0), w(0), w(0), w(0)]
            }
            // The neutral interaction (Anei, as neutral detection): 0; its order and words.
            codes::ability::id::SELECT_USER => {
                own.push(w(0));
                vec![w(852_290), w(0), w(0), w(0), w(0)]
            }
            // Purchase Item: its in-range object (`` `pil `` after it).
            codes::ability::id::PURCHASE_ITEM => {
                let objs = &u.extra.object_slots[2];
                let pil = objs.iter().position(|&x| x == t).and_then(|i| objs.get(i + 1)).map_or(NONE, |&r| rf(r));
                vec![Field::Ref(pil)]
            }
            codes::ability::id::SPAWN_SPIDER | codes::ability::id::SPIDER_ATTACK => Vec::new(),
            // Plague Toss: the buff base's tail alone.
            codes::ability::id::PLAGUE_TOSS => vec![w(852_290), w(0), w(0), w(0), w(0)],
            // Allied Building (CAbilityAllied): twelve places empty, none, twelve pairs
            // and words empty, its in-range listener (the `` `pil `` after it), and a
            // timer (read, the undead computer's Tomb of Relics at 450 seconds).
            codes::ability::id::ALLIED_BUILDING => {
                let objs = &u.extra.object_slots[2];
                let pil = objs.iter().position(|&x| x == t).and_then(|i| objs.get(i + 1)).filter(|r| self.tags.live.get(&r.0).is_some_and(|e| &e.0 == codes::class::IN_RANGE_LISTENER)).map_or(NONE, |&r| rf(r));
                vec![Field::List((0..12).map(|_| vec![Field::Ref(NONE)]).collect()), Field::Ref(NONE), Field::List((0..12).map(|_| vec![w(u32::MAX), w(u32::MAX), w(0)]).collect()), Field::Ref(pil), timer()]
            }
            // Burrow, a morph (CAbilityBurrow): the spell's first fields, two reals, its
            // two forms, a real, none, and 0; its
            // order and words, its listener on mana, five timers, the morph's state at
            // rest (words, reals, an empty list of pairs, two words in a list, a short),
            // and its look every second (event 0xd01bf; read, the computer's Crypt
            // Fiends at 300 seconds).
            codes::ability::id::BURROW => {
                // (Its data's DataA and UnitID, the two forms: read, 'ucry' and 'ucrm'.)
                let ab = u.abilities.iter().filter_map(|(a, _)| self.objects.abilities.get(*a)).find(|ab| ab.str("code").trim().as_bytes() == code);
                let form = |f: &str| ab.map_or(u.type_id, |ab| crate::objects::id_from_str(ab.level_str(f, 1).trim()));
                let (now, then) = (form("DataA"), form("UnitID"));
                own.extend([Field::Real(0.0), Field::Real(o.real("castpt")), Field::Real(o.real("castbsw")), Field::Real(0.0), w(u32::MAX), w(u32::MAX), w(0), w(0), Field::Real(0.0)]);
                own.extend([Field::Real(0.0), Field::Real(0.0), w(u32::from_be_bytes(now)), w(u32::from_be_bytes(then)), Field::Real(0.0), w(u32::MAX), w(0)]);
                let listener = u.extra.spell_owners.iter().find(|x| &x.1 == code && self.tags.live.get(&x.0).is_some_and(|e| &e.0 == codes::class::THRESHOLD_LISTENER)).map_or(NONE, |x| rf(self.tagged(x.0)));
                let mut tail = vec![w(852_290), w(0), w(0), w(0), w(0), Field::Ref(listener), timer(), timer(), timer(), timer(), timer()];
                tail.extend([w(0), w(0), w(0), w(0), w(0), Field::Real(0.0), w(0), Field::Real(0.0), Field::Real(0.0), Field::Real(0.0), w(0), Field::Real(0.0), Field::List(Vec::new()), Field::Real(0.0), Field::Real(0.0), Field::Real(0.0), Field::List(vec![vec![w(0)], vec![w(0)]]), Field::Real(0.0), Field::Short(0)]);
                tail.push(periodic("606200", 1.0, 0xd_01bf));
                tail
            }
            // The cargo hold (CAbilityDevourCargo): 0.0, three 0, ten places empty in
            // each of its lists, two timers, two lists of ten unbounded pairs (read: an
            // empty hold).
            codes::ability::id::CARGO_HOLD_DEVOUR | codes::ability::id::CARGO_HOLD_AMTC => {
                let empty = || Field::List((0..10).map(|_| vec![w(u32::MAX), w(u32::MAX), Field::Real(0.0)]).collect());
                let zeros = || Field::List((0..10).map(|_| vec![w(0)]).collect());
                let far = || Field::List((0..10).map(|_| vec![Field::Real(f32::MAX), Field::Real(f32::MAX)]).collect());
                vec![Field::Real(0.0), w(0), w(0), w(0), empty(), empty(), zeros(), zeros(), timer(), timer(), far(), far()]
            }
            // Sell Items: as Sell Units, its items (each yet to arrive its coming's timer,
            // in its own order: read, Forestwalk's shop's eight of eleven).
            // (Make Items as Sell Items, a word more: 0xc0, read on the undead computer's
            // Tomb of Relics at 450 seconds; provisional.)
            codes::ability::id::SELL_ITEMS | codes::ability::id::MAKE_ITEMS => {
                own.extend([Field::Real(0.0), Field::Real(0.0)]);
                if code == codes::ability::id::MAKE_ITEMS {
                    own.push(w(0xc0));
                }
                let (items, _) = self.shop_goods(h);
                let starts: Vec<(u32, f32)> = self.tags.timer_objects.get(&t.0).map(|v| v.iter().filter(|x| x.1 == "41c010").map(|x| (x.0, x.2)).collect()).unwrap_or_default();
                let idle = || [timer(), Field::Short(0), Field::Short(0), Field::Real(0.0)];
                let mut tail = vec![Field::Ref(NONE), Field::Ref(NONE)];
                let mut coming = 0;
                for k in 0..12 {
                    let mut slot = Vec::new();
                    match items.get(k) {
                        Some(&id) => {
                            let start = self.objects.items.get(id).map_or(0.0, |o| o.real("stockStart"));
                            let (count, max, arriving) = self.shops.stock.get(&(h, id)).map_or((0, 1, start > 0.0), |s| (s.count, s.max, s.arriving));
                            slot.extend([w(u32::from_be_bytes(id)), Field::Real(count as f32), Field::Real(0.0), Field::Real(max as f32)]);
                            slot.extend(idle());
                            if arriving {
                                let (chunks, rest) = super::timer_chunks(start);
                                let first = if chunks > 0 { super::TIMER_CHUNK } else { start };
                                let (n, at) = starts.get(coming).copied().unwrap_or((0, 0.0));
                                coming += 1;
                                let due = f32::from_bits(crate::efloat::add(at.to_bits(), first.to_bits()));
                                slot.push(Field::Timer(rf(t), if chunks > 0 { 0x10 } else { 0 }, Timing(Some((due, first, n, 0xd_01c1, u32::from(chunks > 0))))));
                                slot.extend([Field::Short(chunks.min(0xffff) as u16), Field::Short(chunks.min(0xffff) as u16), Field::Real(rest)]);
                            } else {
                                slot.extend(idle());
                            }
                            slot.push(w(u32::from(arriving)));
                        }
                        None => {
                            slot.extend([w(0), Field::Real(0.0), Field::Real(0.0), Field::Real(f32::MAX)]);
                            slot.extend(idle());
                            slot.extend(idle());
                            slot.push(w(0));
                        }
                    }
                    tail.push(Field::List(vec![slot]));
                }
                tail.extend([w(0xd_0142), w(0), w(0), w(0), w(0)]);
                tail
            }
            // The kinds of the families above: as the spells, Plague Toss, or Spider
            // Attack (their values provisional: the defaults of a spell at rest).
            _ => match ability_family(code) {
                Some(f @ (2 | 3 | 9)) => {
                    own.extend([Field::Real(0.0), Field::Real(o.real("castpt")), Field::Real(o.real("castbsw")), Field::Real(0.0), w(u32::MAX), w(u32::MAX), w(0), w(0), Field::Real(0.0)]);
                    if f != 9 {
                        own.extend([w(u32::MAX), w(u32::MAX), w(0), w(0), Field::Real(0.0), Field::Real(0.0), w(0), w(0), w(0), w(0), Field::Real(0.0), Field::Real(0.0)]);
                    }
                    let listener = u.extra.spell_owners.iter().find(|x| &x.1 == code && self.tags.live.get(&x.0).is_some_and(|e| &e.0 == codes::class::THRESHOLD_LISTENER)).map_or(NONE, |x| rf(self.tagged(x.0)));
                    let mut tail = vec![w(852_290), w(0), w(0), w(0), w(0), Field::Ref(listener), timer(), timer()];
                    if f == 3 {
                        tail.push(timer());
                    }
                    tail
                }
                Some(5) => vec![w(852_290), w(0), w(0), w(0), w(0)],
                Some(_) => Vec::new(),
                // The kinds whose layouts were fitted to the original's records: their
                // templates at rest (`kinds`), the spell's listener its own, a repeating
                // timer restarted from now.
                None => {
                    let listener = u.extra.spell_owners.iter().find(|x| &x.1 == code && self.tags.live.get(&x.0).is_some_and(|e| &e.0 == codes::class::THRESHOLD_LISTENER)).map_or(NONE, |x| rf(self.tagged(x.0)));
                    let now = self.time;
                    let restart = |period: f32, event: u32, kind: u32| Field::Timer(rf(t), kind, Timing(Some((f32::from_bits(crate::efloat::add(now.to_bits(), period.to_bits())), period, 0, event, 1))));
                    // (Robo-Goblin a morph as Burrow with two fields more, the poison buff
                    // its class's fields empty: as their functions read, no record of the
                    // original's yet; provisional.)
                    let manual = match code {
                        codes::ability::id::TINKERER_ROBO_GOBLIN_LEVEL_0 => Some((
                            vec![Field::Real(0.0), Field::Real(o.real("castpt")), Field::Real(o.real("castbsw")), Field::Real(0.0), w(u32::MAX), w(u32::MAX), w(0), w(0), Field::Real(0.0), Field::Real(0.0), Field::Real(0.0), w(u32::from_be_bytes(u.type_id)), w(u32::from_be_bytes(u.type_id)), Field::Real(0.0), w(u32::MAX), w(0), w(0), Field::Real(0.0)],
                            {
                                let mut tl = vec![w(852_290), w(0), w(0), w(0), w(0), Field::Ref(listener), timer(), timer(), timer(), timer(), timer()];
                                tl.extend([w(0), w(0), w(0), w(0), w(0), Field::Real(0.0), w(0), Field::Real(0.0), Field::Real(0.0), Field::Real(0.0), w(0), Field::Real(0.0), Field::List(Vec::new()), Field::Real(0.0), Field::Real(0.0), Field::Real(0.0), Field::List(vec![vec![w(0)], vec![w(0)]]), Field::Real(0.0), Field::Short(0), timer()]);
                                tl
                            },
                        )),
                        codes::buff::id::POISON_BPOI => Some((
                            vec![w(0), Field::Real(0.0), w(0), w(0), w(0), Field::Real(0.0), Field::Real(0.0), Field::Real(0.0), Field::Real(0.0), w(u32::MAX), w(u32::MAX), w(0), Field::Real(0.0), Field::Real(0.0), Field::Real(0.0)],
                            // (Its second timer extended too: read, the original's reader 8
                            // bytes past the plain one.)
                            vec![Field::List(Vec::new()), w(0), timer(), Field::Short(0), Field::Short(0), Field::Real(0.0), timer(), Field::Short(0), Field::Short(0), Field::Real(0.0)],
                        )),
                        _ => None,
                    };
                    let (mut extra, mut tail) = match manual {
                        Some(m) => m,
                        None => {
                            // (Its companion of a kind: the unit's object of it made after
                            // the ability, as its own records name.)
                            let objs: Vec<(u32, u32)> = u.extra.object_slots[2].iter().chain(&u.extra.object_slots[1]).copied().collect();
                            let at = objs.iter().position(|&x| x.0 == t.0).map_or(0, |i| i + 1);
                            let companion = |kind: [u8; 4]| objs[at.min(objs.len())..].iter().chain(&objs[..at.min(objs.len())]).find(|x| self.tags.live.get(&x.0).is_some_and(|e| e.0 == kind && e.1 == x.1)).map_or(Field::Ref(NONE), |&x| Field::Ref(rf(x)));
                            match super::kinds::kind_template(code, Field::Ref(listener), &timer, &restart, &companion) {
                                Some(k) => k,
                                // (A kind no record of the original's was read for: its
                                // fields at rest as its layout has them, a spell's order
                                // and listener its own; provisional throughout.)
                                None => {
                                    let (own_layout, tail_layout) = super::kind_layouts::kind_layout(code)?;
                                    let mut tail = w3_formats::w3z::default_fields(tail_layout);
                                    if tail_layout.replace(' ', "").starts_with("wwwwwr") {
                                        tail[0] = w(852_290);
                                        tail[5] = Field::Ref(listener);
                                    }
                                    (w3_formats::w3z::default_fields(own_layout), tail)
                                }
                            }
                        }
                    };
                    // A buff's timer runs to its end (CBuff vf211's, event 0xd01c4, as the
                    // auras'); one showing its bearer names the player shown
                    // (`FUN_6f4ec0d0`'s word at +0xcc).
                    if let Some(b) = u.buffs.iter().find(|b| b.slot == t.0 && self.buff_kept(b) && &b.id == code) {
                        let started = self.tags.timer_objects.get(&t.0).and_then(|v| v.iter().rev().find(|x| x.1 == "532b50")).map(|x| (x.0, x.2, x.3));
                        let shaped = matches!(tail.get(..6), Some([Field::List(_), Field::Word(_), Field::Timer(r, ..), Field::Short(_), Field::Short(_), Field::Real(_)]) if r.0 == u32::MAX);
                        if let Some((n, at, span)) = started.filter(|_| shaped) {
                            let (chunks, rest) = crate::w3zsave::timer_chunks(span);
                            tail[2] = Field::Timer(rf(t), 0, Timing(Some((f32::from_bits(crate::efloat::add(at.to_bits(), span.to_bits())), span, n, 0xd_01c4, 0))));
                            tail[3] = Field::Short(chunks.min(0xffff) as u16);
                            tail[4] = Field::Short(chunks.min(0xffff) as u16);
                            tail[5] = Field::Real(rest);
                        }
                        if let (Some(p), true) = (b.vision_player, crate::ability::SHOWS_BEARER.contains(&code)) {
                            if extra.len() > 8 {
                                extra[8] = w(p as u32);
                            }
                        }
                    }
                    own.extend(extra);
                    tail
                }
            },
        };
        let handlers = Handlers { keyed, fixed: Default::default(), events: vec![Vec::new(); 0x400] };
        Some(Second::Agent { own, handlers, tail })
    }
}

/// A weapon's kind as the engine numbers it (weapTp; provisional past normal and missile).
fn weapon_kind(s: &str) -> u32 {
    match s.trim().to_ascii_lowercase().as_str() {
        "normal" => 1,
        "missile" => 2,
        "artillery" => 3,
        "instant" => 4,
        "msplash" => 5,
        "mbounce" => 6,
        "mline" => 7,
        "aline" => 8,
        _ => 0,
    }
}

/// An attack type as the engine numbers it (common.j's ATTACK_TYPE order).
fn attack_kind(s: &str) -> u32 {
    match s.trim().to_ascii_lowercase().as_str() {
        "spells" => 0,
        "pierce" => 2,
        "siege" => 3,
        "magic" => 4,
        "chaos" => 5,
        "hero" => 6,
        _ => 1,
    }
}

/// A weapon's sound as the engine numbers it (common.j's WEAPON_TYPE order).
fn weapon_sound(s: &str) -> u32 {
    const NAMES: [&str; 24] = ["", "metallightchop", "metalmediumchop", "metalheavychop", "metallightslice", "metalmediumslice", "metalheavyslice", "metalmediumbash", "metalheavybash", "metalmediumstab", "metalheavystab", "woodlightslice", "woodmediumslice", "woodheavyslice", "woodlightbash", "woodmediumbash", "woodheavybash", "woodlightstab", "woodmediumstab", "clawlightslice", "clawmediumslice", "clawheavyslice", "axemediumchop", "rockheavybash"];
    let s = s.trim().to_ascii_lowercase();
    NAMES.iter().position(|&n| !n.is_empty() && n == s).unwrap_or(0) as u32
}

/// Whether a kind is a task's (an order's steps, on its unit's chain).
fn is_task(code: &[u8; 4]) -> bool {
    matches!(code, codes::class::TASK | codes::class::TASK_A | codes::class::TASK_T | b"tsk." | codes::class::TASK_O)
}

/// A reference's tag and birth.
fn rf2(r: Ref) -> (u32, u32) {
    (r.0, r.1)
}

/// A weapon's targets as bits (as abilities' +0x4c; 1 for none).
/// The abilities whose classes' own fields and saves are those of a kind already read
/// (their classes by their vf7's code, their vf18 and vf14 from the vtables): 2 the
/// spell's fields and its tail with two timers (`FUN_6f42f3e0`, `FUN_6f42e2b0`), 3
/// with three (`FUN_6f42d2a0`), 9 the spell's first fields and two timers
/// (`FUN_6f42f4b0`), 5 the ability's alone and the buff base's tail (`FUN_6f42d5f0`),
/// 0 the ability's alone (`FUN_6f03eda0`).
fn ability_family(code: &[u8; 4]) -> Option<u8> {
    match code {
        codes::ability::id::BREATH_OF_FROST_CREEP | codes::ability::id::KEEPER_ENTANGLING_ROOTS | codes::ability::id::WARDEN_FAN_OF_KNIVES | codes::ability::id::KEEPER_FORCE_OF_NATURE | codes::ability::id::WARDEN_SHADOW_STRIKE | codes::ability::id::BLOOD_MAGE_BANISH | codes::ability::id::MOUNTAIN_KING_THUNDER_BOLT | codes::ability::id::MOUNTAIN_KING_THUNDER_CLAP | codes::ability::id::ITEM_DISPEL_CHAIN | codes::ability::id::ITEM_DETECT_AOE | codes::ability::id::ALCHEMIST_ACID_BOMB | codes::ability::id::BREWMASTER_BREATH_OF_FIRE | codes::ability::id::DARK_RANGER_CHARM | codes::ability::id::PIT_LORD_DOOM | codes::ability::id::FIRE_BOLT | codes::ability::id::SEA_WITCH_FORKED_LIGHTNING | codes::ability::id::FIRELORD_SOUL_BURN | codes::ability::id::TINKERER_SUMMON_FACTORY_LEVEL_0 | codes::ability::id::ALCHEMIST_TRANSMUTE | codes::ability::id::FARSEER_FAR_SIGHT | codes::ability::id::SHADOW_HUNTER_SERPENT_WARD_AOWD | codes::ability::id::CHIEFTAIN_WAR_STOMP | codes::ability::id::DREADLORD_CARRION_SWARM | codes::ability::id::LICH_FROST_NOVA | codes::ability::id::ABSORB_MANA | codes::ability::id::ANTI_MAGIC_SHIELD | codes::ability::id::CARGO_HOLD_DEATH | codes::ability::id::CONTROL_MAGIC | codes::ability::id::COUPLE_INSTANT_ARCHER_ACOI | codes::ability::id::DECOUPLE | codes::ability::id::DEVOUR_MAGIC | codes::ability::id::EAT_TREE | codes::ability::id::SENTRY_WARD | codes::ability::id::GRAB_TREE | codes::ability::id::INVISIBILITY | codes::ability::id::LIGHTNING_SHIELD | codes::ability::id::POSSESSION | codes::ability::id::PURGE | codes::ability::id::STASIS_TRAP | codes::ability::id::TAUNT => Some(2),
        codes::ability::id::REINCARNATION_CREEP | codes::ability::id::WARDEN_SPIRIT_OF_VENGEANCE | codes::ability::id::BLOOD_MAGE_PHOENIX | codes::ability::id::ARCH_MAGE_WATER_ELEMENTAL | codes::ability::id::PIT_LORD_HOWL_OF_TERROR | codes::ability::id::FIRELORD_SUMMON_LAVA_SPAWN | codes::ability::id::BEAST_MASTER_SUMMON_BEAR | codes::ability::id::BEAST_MASTER_SUMMON_QUILBEAST | codes::ability::id::BEAST_MASTER_SUMMON_HAWK | codes::ability::id::WATERY_MINION | codes::ability::id::CHIEFTAIN_REINCARNATION | codes::ability::id::FARSEER_SPIRIT_WOLF | codes::ability::id::BLADE_MASTER_WIND_WALK | codes::ability::id::DROP_INSTANT | codes::ability::id::REVENGE | codes::ability::id::ROAR | codes::ability::id::STAND_DOWN => Some(9),
        codes::ability::id::DEMON_HUNTER_EVASION | codes::ability::id::PIT_LORD_CLEAVING_ATTACK | codes::ability::id::BALLS_OF_FIRE | codes::ability::id::CORROSIVE_BREATH | codes::ability::id::FEEDBACK | codes::ability::id::FLAK_CANNON | codes::ability::id::FROST_ATTACK | codes::ability::id::FREEZING_BREATH | codes::ability::id::FRAG_SHARDS | codes::ability::id::GYROCOPTER_BOMBS | codes::ability::id::SHADOW_MELD_AKAMA | codes::ability::id::IMPALING_BOLT | codes::ability::id::LIQUID_FIRE | codes::ability::id::MOON_GLAIVE | codes::ability::id::ROCKET_ATTACK | codes::ability::id::RESISTANT_SKIN | codes::ability::id::STORM_HAMMERS => Some(5),
        codes::ability::id::COLD_ARROWS | codes::ability::id::PRIESTESS_SEARING_ARROWS | codes::ability::id::DARK_RANGER_BLACK_ARROW | codes::ability::id::FIRELORD_INCINERATE_ANIA | codes::ability::id::PARASITE | codes::ability::id::LICH_FROST_ARMOR_AUTOCAST | codes::ability::id::ORB_OF_ANNIHILATION | codes::ability::id::FRENZY | codes::ability::id::HEAL | codes::ability::id::SLOW | codes::ability::id::SPELL_STEAL => Some(3),
        codes::ability::id::AWAKEN | codes::ability::id::DEATH_DAMAGE_SAPPER | codes::ability::id::GHOST_VISIBLE | codes::ability::id::FROST_BREATH | codes::ability::id::SPAWN_SPIDERLING => Some(0),
        _ => None,
    }
}

/// An ability's targets from its data's `targs1` (read against those of the abilities
/// above: ground 0x2, air 0x4, structure 0x8, ward 0x10, item 0x20, tree 0x40, wall
/// 0x80, debris 0x100, decoration 0x200, bridge 0x400, self 0x1000, player 0x2000,
/// allies 0x4000, friend 0x6000, neutral 0x8000, enemy 0x10000, notself 0x1e000, vuln
/// 0x100000, invu 0x200000, hero 0x400000, nonhero 0x800000, alive 0x1000000, dead
/// 0x2000000, organic 0x4000000, mechanical 0x8000000, nonancient 0x40000000; none
/// given 0xfdd1f00e).  Provisional: mechanical's, ancient's, and the sappers' bits.
fn data_targets(list: &[String]) -> u32 {
    let mut bits = 0;
    for t in list {
        bits |= match t.trim().to_ascii_lowercase().as_str() {
            "ground" => 0x2,
            "air" => 0x4,
            "structure" => 0x8,
            "ward" => 0x10,
            "item" => 0x20,
            "tree" => 0x40,
            "wall" => 0x80,
            "debris" => 0x100,
            "decoration" => 0x200,
            "bridge" => 0x400,
            "self" => 0x1000,
            "player" => 0x2000,
            "allies" => 0x4000,
            "friend" => 0x6000,
            "neutral" => 0x8000,
            "enemy" => 0x1_0000,
            "notself" => 0x1_e000,
            "vuln" | "vulnerable" => 0x10_0000,
            "invu" | "invulnerable" => 0x20_0000,
            "hero" => 0x40_0000,
            "nonhero" => 0x80_0000,
            "alive" => 0x100_0000,
            "dead" => 0x200_0000,
            "organic" => 0x400_0000,
            "mechanical" => 0x800_0000,
            "ancient" => 0x2000_0000,
            "nonancient" => 0x4000_0000,
            _ => 0,
        };
    }
    if bits == 0 { 0xfdd1_f00e } else { bits }
}

fn weapon_targets(list: &[String]) -> u32 {
    let mut bits = 0;
    for t in list {
        bits |= match t.trim().to_ascii_lowercase().as_str() {
            "ground" => 0x2,
            "air" => 0x4,
            "structure" => 0x8,
            "ward" => 0x10,
            "item" => 0x20,
            "tree" => 0x40,
            "wall" => 0x80,
            "debris" => 0x100,
            "decoration" => 0x200,
            "bridge" => 0x400,
            _ => 0,
        };
    }
    if bits == 0 { 1 } else { bits }
}

impl World {
    /// An attack's own record (CAbilityAttack, `FUN_6f49bd60`; read against the quiet
    /// test map's 45): after CAbility's part, references; per weapon (two each) its dice,
    /// sides, damage, a bonus (an aura's, a ceiling of its factor times the least
    /// damage: read, Trueshot's on trolls), modifiers, its kind, attack type, sound,
    /// cooldown, damage point and backswing; its targets, its acquisition range, ranges
    /// and range buffers, 600, its periodic timer (event 0xd01ad, every 2 seconds) and
    /// its look for a target's (event 0xd01c1), its place, and lists empty.
    fn attack_record(&self, h: u32, t: (u32, u32), own: Vec<Field>) -> Second {
        let u = self.unit(h).unwrap();
        let o = self.objects.units.get(u.type_id).unwrap();
        let w = Field::Word;
        let r = Field::Real;
        let weapons = o.int("weapsOn");
        // (A weapon is written as its data has it, enabled or not: read, a Dreadlord's
        // second, its missile of 500, where every single-weapon unit read has none.)
        let on = |k: u32| weapons & (1 << (k - 1)) != 0 || !matches!(o.str(&format!("weapTp{k}")).trim(), "" | "_" | "-");
        let per = |f: &dyn Fn(u32) -> Field| Field::List(vec![vec![f(1)], vec![f(2)]]);
        let int = |name: &str, k: u32, absent: u32| if on(k) { o.int(&format!("{name}{k}")).max(0) as u32 } else { absent };
        // (A hero's primary attribute adds to each weapon's damage, and its agility
        // quickens it, in the engine's arithmetic: read, a Dreadlord's 20 strength and
        // 16 agility, 20 and 1.3199999.)
        let primary = self.primary_attribute_pub(h) as u32;
        let quickness = if u.is_hero {
            let per_point = self.misc.get_str("Misc", "AgiAttackSpeedBonus").and_then(|v| crate::objects::engine_real(v.trim())).unwrap_or(0.02);
            let agi = (u.agi + u.item_bonus.agi).floor() as i32;
            f32::from_bits(crate::efloat::add(1f32.to_bits(), crate::efloat::mul(crate::efloat::from_int(agi), per_point.to_bits())))
        } else {
            1.0
        };
        let real = |name: &str, k: u32| if on(k) { o.real(&format!("{name}{k}")) } else { 0.0 };
        let modifiable = |f: &dyn Fn(u32) -> f32| Field::List(vec![vec![r(f(1)), r(0.0), r(f32::MAX)], vec![r(f(2)), r(0.0), r(f32::MAX)]]);
        let plain = || Field::List(vec![vec![r(0.0), r(0.0), r(f32::MAX)], vec![r(0.0), r(0.0), r(f32::MAX)]]);
        let factor: f32 = u.buffs.iter().map(|b| b.damage_factor).sum();
        let bonus = |k: u32| {
            if factor == 0.0 {
                return 0;
            }
            let least = int("dmgplus", k, 0) + int("dice", k, 1);
            f32::from_bits(crate::efloat::mul(crate::efloat::from_int(least as i32), factor.to_bits())).ceil() as u32
        };
        let timer = || Field::Timer(NONE, 0, Timing(None));
        let attack_timer = match u.extra.attack_timer {
            Some((count, at, period)) => {
                let mut due = crate::efloat::add(at.to_bits(), period.to_bits());
                while f32::from_bits(due) <= self.time {
                    due = crate::efloat::add(due, period.to_bits());
                }
                Field::Timer(rf(t), 2, Timing(Some((f32::from_bits(due), period, count, 0xd_01ad, 1))))
            }
            None => timer(),
        };
        // (Its look's due time the port's countdown on from now: each miss waits half a
        // second from the last look's time, not the step's.)
        let look_timer = match u.extra.look_timer {
            Some((count, due, wait)) if due > self.time => Field::Timer(rf(t), 0, Timing(Some((due, wait, count, 0xd_01c1, 0)))),
            _ => timer(),
        };
        let mut tail = vec![w(u32::MAX), w(u32::MAX), w(0), w(u32::MAX), w(u32::MAX), w(0)];
        tail.push(per(&|k| w(int("dice", k, 0))));
        tail.push(per(&|k| w(int("sides", k, 0))));
        tail.push(per(&|k| w(int("dmgplus", k, 0) + if on(k) { primary } else { 0 })));
        tail.push(per(&|k| w(bonus(k))));
        tail.push(plain());
        tail.push(per(&|k| w(if on(k) { weapon_kind(&o.str(&format!("weapTp{k}"))) } else { 0 })));
        tail.push(per(&|k| w(if on(k) { attack_kind(&o.str(&format!("atkType{k}"))) } else { 1 })));
        tail.push(per(&|k| w(if on(k) { weapon_sound(&o.str(&format!("weapType{k}"))) } else { 0 })));
        tail.push(per(&|_| w(1)));
        tail.push(plain());
        tail.push(plain());
        tail.push(per(&|k| r(real("cool", k))));
        tail.push(modifiable(&|k| real("dmgpt", k)));
        tail.push(modifiable(&|k| real("backSw", k)));
        tail.extend([r(quickness), r(-f32::MAX), r(f32::MAX)]);
        tail.extend([timer(), timer(), timer(), timer()]);
        tail.push(per(&|k| w(if on(k) { weapon_targets(&o.list(&format!("targs{k}"))) } else { 1 })));
        tail.push(w(0));
        let splash = self.splash_slots(h);
        tail.push(Field::List(splash.iter().map(|s| vec![Field::Ref(s.map_or(NONE, rf))]).collect()));
        tail.extend([r(u.acquire_range), r(0.0)]);
        tail.push(per(&|k| r(real("rangeN", k))));
        tail.push(per(&|k| r(real("RngBuff", k))));
        tail.push(r(600.0));
        tail.extend([attack_timer, look_timer, timer()]);
        tail.extend([r(u.x), r(u.y)]);
        tail.extend([w(0), w(u32::MAX), w(u32::MAX), w(0), r(0.0)]);
        tail.push(Field::List((0..8).map(|_| vec![w(u32::MAX), w(u32::MAX), w(0)]).collect()));
        tail.push(Field::List((0..8).map(|_| vec![timer()]).collect()));
        tail.extend([w(u32::MAX), w(u32::MAX), w(0), r(0.0), r(0.0), r(0.0), w(0), w(0), w(0), w(0), r(0.0)]);
        let handlers = Handlers { keyed: Vec::new(), fixed: Default::default(), events: vec![Vec::new(); 0x400] };
        Second::Agent { own, handlers, tail }
    }
}

impl World {
    /// A unit's splash data (`spla`, after its attack among its objects), one for each
    /// weapon of a splashing kind, enabled or not, by weapon.
    fn splash_slots(&self, h: u32) -> [Option<(u32, u32)>; 2] {
        let mut out = [None; 2];
        let (Some(u), Some(o)) = (self.unit(h), self.unit(h).and_then(|u| self.objects.units.get(u.type_id))) else { return out };
        let objs = &u.extra.object_slots[2];
        let Some(at) = objs.iter().position(|&(s, _)| self.tags.live.get(&s).is_some_and(|e| &e.0 == codes::ability::id::ATTACK)) else { return out };
        let mut next = objs[at + 1..].iter().copied().filter(|&(s, b)| self.tags.live.get(&s).is_some_and(|e| &e.0 == codes::class::SPLASH && e.1 == b));
        for k in 1..=2 {
            if matches!(o.str(&format!("weapTp{k}")).trim(), "msplash" | "artillery" | "aline" | "mline" | "mbounce") {
                out[k - 1] = next.next();
            }
        }
        out
    }

    /// A unit's splash data's records (CSplashData, the agent base alone; own
    /// `FUN_6f49c330`): its splash targets (none given, the default 0xfdd1f00e), its
    /// full, half, and quarter areas, and its half and quarter factors (read:
    /// Forestwalk's drakes' and whelps').
    pub fn splash_records(&self, h: u32) -> Vec<(u32, First, Second)> {
        let Some(o) = self.unit(h).and_then(|u| self.objects.units.get(u.type_id)) else { return Vec::new() };
        let mut out = Vec::new();
        for (k, slot) in self.splash_slots(h).into_iter().enumerate() {
            let Some(t) = slot else { continue };
            let k = k + 1;
            let list = o.list(&format!("splashTargs{k}"));
            let targets = match weapon_targets(&list) {
                1 => 0xfdd1_f00e,
                bits => bits,
            };
            let real = |name: &str| o.real(&format!("{name}{k}"));
            let own = vec![Field::Word(targets), Field::Real(real("Farea")), Field::Real(real("Harea")), Field::Real(real("Qarea")), Field::Real(real("Hfact")), Field::Real(real("Qfact"))];
            out.push((t.0, super::agent_first(t, codes::class::SPLASH, &[]), Second::Agent { own, handlers: Handlers { keyed: Vec::new(), fixed: Default::default(), events: Vec::new() }, tail: Vec::new() }));
        }
        out
    }

    /// Each gold mine's queue of waiting workers' places, first first, by mine.
    pub(crate) fn mine_queues(&self) -> Vec<Vec<(u32, u32)>> {
        self.unit_order
            .iter()
            .filter_map(|&m| self.unit(m))
            .map(|m| m.extra.mine_queue.iter().filter_map(|&w| self.saved_order(w).and_then(|o| o.queue)).collect::<Vec<_>>())
            .filter(|q| !q.is_empty())
            .collect()
    }

    /// The units' AI lists (`uref`, CUnitRefList's `FUN_6f42f860`; read against the quiet
    /// test map's): four words, the second and third the list made before it of its
    /// owner's units (provisional: by owner), and empty handlers.
    pub fn unit_ref_records(&self) -> Vec<(u32, Second)> {
        let mut lists: Vec<(u8, (u32, u32))> = Vec::new();
        for &h in &self.unit_order {
            let Some(u) = self.unit(h) else { continue };
            for &(s, b) in &u.extra.ai_slots {
                if self.tags.live.get(&s).is_some_and(|e| &e.0 == codes::class::UNIT_AI_REF && e.1 == b) {
                    lists.push((u.owner, (s, b)));
                }
            }
        }
        lists.sort_by_key(|l| l.1 .1);
        let mut out = Vec::new();
        for (k, &(owner, t)) in lists.iter().enumerate() {
            let prev = lists[..k].iter().rev().find(|l| l.0 == owner).map_or(NONE, |l| rf(l.1));
            let handlers = Handlers { keyed: Vec::new(), fixed: Default::default(), events: vec![Vec::new(); 0x400] };
            out.push((t.0, Second::UnitRefs { words: [0, prev.0, prev.1, 0], handlers }));
        }
        // A gold mine's queue: each waiting worker's place naming the next's (read:
        // Forestwalk's mine at 3 seconds, two peasants waiting).
        for q in self.mine_queues() {
            for (k, t) in q.iter().enumerate() {
                let next = q.get(k + 1).map_or(NONE, |&n| rf(n));
                let handlers = Handlers { keyed: Vec::new(), fixed: Default::default(), events: vec![Vec::new(); 0x400] };
                out.push((t.0, Second::UnitRefs { words: [0, next.0, next.1, 0], handlers }));
            }
        }
        out
    }
}

impl World {
    /// The units' AIs (`AI-U`; read against the quiet test map's): fourteen words, the
    /// tenth and eleventh a computer's unit's town site's AI (`AIts`, provisional: the
    /// nearest), and empty handlers.
    pub fn unit_ai_records(&self) -> Vec<(u32, Second)> {
        let mut out = Vec::new();
        for &h in &self.unit_order {
            let Some(u) = self.unit(h) else { continue };
            let computer = self.players.get(u.owner as usize).is_some_and(|p| p.controller == crate::types::map_control::COMPUTER && p.slot_state == crate::types::slot_state::PLAYING);
            for &(s, b) in &u.extra.ai_slots {
                if !self.tags.live.get(&s).is_some_and(|e| &e.0 == codes::class::UNIT_AI && e.1 == b) {
                    continue;
                }
                // (A computer's workers' and structures': read, its acolytes', Necropolis's,
                // and haunted mine's, not its ghoul's.)
                let town = u.is_building || self.objects.units.get(u.type_id).is_some_and(|o| o.list("type").iter().any(|t| t.trim().eq_ignore_ascii_case("peon")));
                let site = if computer && town { self.tags.sites.iter().min_by(|a, c| (a.0 - u.x).hypot(a.1 - u.y).total_cmp(&(c.0 - u.x).hypot(c.1 - u.y))).map_or(NONE, |x| rf(x.2)) } else { NONE };
                let w = Field::Word;
                let own = vec![w(0), w(0), w(0), w(u32::MAX), w(u32::MAX), w(0), w(0), w(u32::MAX), w(u32::MAX), w(0), w(site.0), w(site.1), w(0), w(0)];
                let handlers = Handlers { keyed: Vec::new(), fixed: Default::default(), events: vec![Vec::new(); 0x400] };
                out.push((s, Second::Agent { own, handlers, tail: Vec::new() }));
            }
        }
        out
    }
}

impl World {
    /// A unit's two regions (`^pmr`; read against the quiet test map's): the pathing
    /// map's cells its collision covers (kind 1 and flags 0xca, a structure's none),
    /// a count of them; and the coarse map's 8-cell blocks those fall in, its count's
    /// high byte 0x10 and its owner's number.  Each its map and its position, and the
    /// map's search mark as last met (the running game's: 0 written).
    pub fn unit_region_records(&self, h: u32) -> Vec<(u32, w3_formats::w3z::First)> {
        let Some(u) = self.unit(h) else { return Vec::new() };
        let Some(o) = self.objects.units.get(u.type_id) else { return Vec::new() };
        let Some(p) = self.pathing.as_ref() else { return Vec::new() };
        let first = &u.extra.object_slots[0];
        if first.len() < 4 {
            return Vec::new();
        }
        let [coarse, fine] = self.pathing_map_objects();
        let (cx, cy) = ((u.x - p.origin[0]) / 32.0, (u.y - p.origin[1]) / 32.0);
        // (Its footprint a square of one cell more than its collision's sixteens, the
        // cell it is in ⌊n/2⌋ from its start: read, collision 16 and 31 two cells, 32
        // three, 48 four.)
        // (At most four: read, a town hall's and a Necropolis's.)
        let n = ((o.real("collision") / 16.0) as u32 + 1).min(4);
        let (r0, c0) = ((cy.floor() as u32).saturating_sub(n / 2), (cx.floor() as u32).saturating_sub(n / 2));
        let (r1, c1) = (r0 + n, c0 + n);
        let cells = (r1 - r0) * (c1 - c0);
        // (Its blocks those of the cells its collision's circle reaches, ⌊c - r⌋ to
        // ⌊c + r⌋: read, a sheep's one block where its footprint has two.)
        let r = o.real("collision") / 32.0;
        let (b0, d0) = (((cy - r).floor() as u32) >> 3, ((cx - r).floor() as u32) >> 3);
        let (b1, d1) = ((((cy + r).floor() as u32) >> 3) + 1, (((cx + r).floor() as u32) >> 3) + 1);
        let blocks = (b1 - b0) * (d1 - d0);
        let pos = rf(first[1]);
        let region = |map: (u32, u32), word: u32, bounds: [u32; 4], words: [u32; 3]| w3_formats::w3z::First::Region { refs: [rf(map), pos], word, bounds, words };
        // (A worker on a gold run its run's flags: ghosting 0x08 (0xd6 cleared), its last
        // word 0x20000000; at its mine its ghosting ended, 0xca and 0x10; read,
        // Forestwalk's peasants at 12 seconds.)
        // (A lumber walker ghosting too.)
        let run = u.extra.gold_run.as_ref().filter(|r| !r.lumber || matches!(r.phase, crate::goldrun::Phase::ToMine | crate::goldrun::Phase::ToHall(_)));
        // (Chopping at its tree as a worker at its mine: read, 0xda.)
        let chopping = u.extra.gold_run.as_ref().is_some_and(|r| r.lumber && matches!(r.phase, crate::goldrun::Phase::Chopping(_)));
        let at_mine = run.is_some_and(|r| r.at_mine) || chopping;
        let ghosted = run.is_some_and(|r| r.ghosted) && !at_mine;
        let low = if ghosted { 0x08 } else { 0xca } | if at_mine { 0x10 } else { 0 };
        // A hidden unit's are off the pathing map (read: the gold mine a haunted mine
        // hides): its coarse one marked 0x0c000000, its fine one emptied (a worker in a
        // mine its flags kept).
        if u.hidden {
            return vec![
                (first[2].0, region(coarse, 0x0c00_0000, [b0, d0, b1, d1], [0, (0x10 | u.owner as u32) << 24 | blocks, 0])),
                (first[3].0, region(fine, if run.is_some() { low } else { 0 }, [0xfffe_0c00; 4], [0, 0, 0])),
            ];
        }
        vec![
            (first[2].0, region(coarse, 0, [b0, d0, b1, d1], [0, (0x10 | u.owner as u32) << 24 | blocks, 0])),
            (first[3].0, region(fine, if u.is_building { 0x0100_0000 } else { 0x0100_0000 | low }, [r0, c0, r1, c1], [0, cells, if ghosted { 0x2000_0000 } else { 0 }])),
        ]
    }
}

impl World {
    /// A unit's long-range path (`l^pa`, CLrPath's `FUN_6f167780`; read against the quiet
    /// test map's, none walking a path at 5 seconds): no waypoints; its first word its
    /// owner's number, with 0x8 for the local player's, 0x2 for one that moves and is
    /// not a critter, 0x1 an AI player's that moves (in its high half, by nibbles); a
    /// unit's word 0x2000002; a limit of 700 (a critter's 32) and 400; its unit's fine
    /// region.
    pub fn unit_path_record(&self, h: u32) -> Option<(u32, w3_formats::w3z::First)> {
        let u = self.unit(h)?;
        let o = self.objects.units.get(u.type_id)?;
        let first = &u.extra.object_slots[0];
        if first.len() < 10 {
            return None;
        }
        let computer = self.players.get(u.owner as usize).is_some_and(|p| p.controller == crate::types::map_control::COMPUTER && p.slot_state == crate::types::slot_state::PLAYING);
        let ai = computer || u.owner == crate::types::PLAYER_NEUTRAL_AGGRESSIVE;
        let mobile = !u.is_building && o.real("spd") > 0.0;
        let critter = u.owner as usize == crate::types::PLAYER_NEUTRAL_PASSIVE as usize && u.abilities.iter().any(|(a, _)| a == codes::ability::id::WANDER);
        let mut hi = 0;
        if u.owner == self.local_player {
            hi |= 0x8;
        }
        if mobile && !critter {
            hi |= 0x2;
        }
        if mobile && ai {
            hi |= 0x1;
        }
        let word = (hi << 4 | (u.owner as u32 & 0xf)) << 16;
        let way = |limit: u16| (w3_formats::w3z::Array { words: [0, 0, 128, 0], entries: Vec::new() }, u32::MAX, 0, limit);
        // Chopping (read: Forestwalk's peasants chopping): its tree's cell last, 0x100000
        // more, 0x12000012, its ways empty, and the tree's first footprint region.
        if let (Some(r), Some(p)) = (u.extra.gold_run.as_ref().filter(|r| r.lumber && matches!(r.phase, crate::goldrun::Phase::Chopping(_))), self.pathing.as_ref()) {
            if let Some(crate::types::Obj::Destructable(d)) = self.handles.get(r.mine) {
                let empty = |limit: u16| (super::path_array(Vec::new()), u32::MAX, 0, limit);
                let region = d.object_slots.get(3).map_or(NONE, |&s| rf(self.tagged(s)));
                return Some((
                    first[9].0,
                    w3_formats::w3z::First::Path {
                        reals: [-128000.0, -128000.0, -128000.0, -128000.0, ((d.x - p.origin[0]) / 32.0).floor(), ((d.y - p.origin[1]) / 32.0).floor()],
                        words: [word | 0x10_0000, 0, 0, u32::MAX, u32::MAX, 0],
                        real: o.real("collision") / 32.0,
                        word: 0x1200_0012,
                        ways: [empty(700), empty(400)],
                        refs: [NONE, NONE, rf(first[3]), region],
                    },
                ));
            }
        }
        // A walk to a point (read: Forestwalk's peasants sent off by script): its local
        // goal thrice, 0x2000000 in its first word, its collision, a local way from its
        // local goal to where it stands and its long way; its fine region.
        // Provisional: the unit it yields to and the ways' counters (none written).
        if let (Some(p), &crate::types::Action::Move { x, y }) = (self.pathing.as_ref(), &u.action) {
            if self.saved_order(h).is_some_and(|o| o.walk.is_some()) && u.extra.gold_run.is_none() {
                let cells = |(x, y): (f32, f32)| [(x - p.origin[0]) / 32.0, (y - p.origin[1]) / 32.0];
                let here = cells((u.x, u.y));
                let [gx, gy] = cells((x, y));
                let local = if u.path.len() > 1 { cells(u.path[u.path.len() - 1]) } else { [gx, gy] };
                let set = |entries: Vec<[f32; 2]>, limit: u16| (super::path_array(entries), 0, 0, limit);
                let corners: Vec<[f32; 2]> = match u.extra.coarse.as_ref() {
                    Some(c) => c.points.iter().map(|&(x, y)| [x, y]).collect(),
                    None => vec![[gx / 2.0, gy / 2.0], [here[0] / 2.0, here[1] / 2.0]],
                };
                return Some((
                    first[9].0,
                    w3_formats::w3z::First::Path {
                        reals: [local[0], local[1], local[0], local[1], local[0], local[1]],
                        words: [word | 0x0200_0000, 0, 0, u32::MAX, u32::MAX, 0],
                        real: o.real("collision") / 32.0,
                        word: 0x0200_0002,
                        ways: [set(vec![local, here], 700), set(corners, 400)],
                        refs: [NONE, NONE, rf(first[3]), NONE],
                    },
                ));
            }
        }
        // A worker on a gold run (read: Forestwalk's peasants at 12 seconds): walking,
        // its walk's goal thrice, 0x12000000 in its first word, its collision, 0x10000010,
        // its local way from its end to where it stands and its long way in half cells,
        // and the target's region; in its mine its last goal, 0x100000 more, 0x12000012,
        // its ways empty.
        if let (Some(r), Some(p)) = (u.extra.gold_run.as_ref().filter(|r| !r.lumber || matches!(r.phase, crate::goldrun::Phase::ToMine | crate::goldrun::Phase::ToHall(_))), self.pathing.as_ref()) {
            let target = match r.phase {
                crate::goldrun::Phase::ToHall(hall) => hall,
                _ => r.mine,
            };
            let rt = self.run_target(target);
            let region = rt.map_or(NONE, |t| t.3);
            let goal = rt.map(|t| (t.0, t.1));
            let cells = |(x, y): (f32, f32)| [(x - p.origin[0]) / 32.0, (y - p.origin[1]) / 32.0];
            let collision = o.real("collision") / 32.0;
            let set = |entries: Vec<[f32; 2]>, limit: u16| (super::path_array(entries), 0, 0, limit);
            let empty = |limit: u16| (super::path_array(Vec::new()), u32::MAX, 0, limit);
            let walking = self.saved_order(h).is_some_and(|o| o.walk.is_some());
            if let (true, Some((gx, gy))) = (walking, goal) {
                let here = cells((u.x, u.y));
                let end = u.path.last().map_or([gx, gy], |&q| cells(q));
                return Some((
                    first[9].0,
                    w3_formats::w3z::First::Path {
                        reals: [gx, gy, gx, gy, gx, gy],
                        words: [word | 0x1200_0000, 0, 0, u32::MAX, u32::MAX, 0],
                        real: collision,
                        word: 0x1000_0010,
                        ways: [set(vec![end, here], 700), set(vec![[gx / 2.0, gy / 2.0], [here[0] / 2.0, here[1] / 2.0]], 400)],
                        refs: [NONE, NONE, rf(first[3]), region],
                    },
                ));
            }
            if let (crate::goldrun::Phase::InMine(_), Some((gx, gy))) = (r.phase, goal) {
                let _ = cells;
                return Some((
                    first[9].0,
                    w3_formats::w3z::First::Path {
                        reals: [-128000.0, -128000.0, -128000.0, -128000.0, gx, gy],
                        words: [word | 0x10_0000, 0, 0, u32::MAX, u32::MAX, 0],
                        real: collision,
                        word: 0x1200_0012,
                        ways: [empty(700), empty(400)],
                        refs: [NONE, NONE, rf(first[3]), region],
                    },
                ));
            }
        }
        Some((
            first[9].0,
            w3_formats::w3z::First::Path {
                reals: [-128000.0; 6],
                words: [word, 0, 0, u32::MAX, u32::MAX, 0],
                real: 0.0,
                // (A unit's 0x2000002, a structure's 0: read, every one of the quiet test
                // map's.)
                word: if u.is_building { 0 } else { 0x0200_0002 },
                ways: [way(if critter { 32 } else { 700 }), way(400)],
                refs: [NONE, NONE, rf(first[3]), NONE],
            },
        ))
    }
}

impl World {
    /// A unit's in-range listeners with their abilities (`` `pil ``): its abilities'
    /// kept with it, its alarm's, Ensnare's, its inventory's, its spells', and a
    /// flyer's detection's.
    pub(crate) fn unit_pils(&self, h: u32) -> Vec<(u32, [u8; 4])> {
        let Some(u) = self.unit(h) else { return Vec::new() };
        let code_of = |s: u32| self.tags.live.get(&s).map(|e| e.0);
        let mut pils: Vec<(u32, [u8; 4])> = Vec::new();
        let rest = &u.extra.object_slots[2];
        // (Not a kept slot another object has taken since: read, a flyer's making's,
        // let go as it slept.)
        for (k, &(s, b)) in rest.iter().enumerate() {
            if self.tagged(s).1 == b && code_of(s) == Some(*codes::class::IN_RANGE_LISTENER) {
                if let Some(c) = rest[..k].iter().rev().filter(|&&(x, b)| self.tagged(x).1 == b).filter_map(|&(x, _)| code_of(x)).find(|c| c[0] == b'A') {
                    pils.push((s, c));
                }
            }
        }
        pils.extend(u.extra.alarm_pil.map(|s| (s, *codes::ability::id::ALARM)));
        pils.extend(u.extra.ensnare_pil.map(|p| (p.1, *codes::ability::id::ENSNARE)));
        pils.extend(u.extra.item_pil.map(|p| (p.0, *codes::ability::id::INVENTORY)));
        pils.extend(u.extra.spell_pils.iter().copied());
        // (A flyer's detection's, made again as it woke: read, Forestwalk's whelps' and
        // dragons' at the first dawn, which the original's sleep lets go and crashed
        // without.)
        pils.extend(u.extra.flyer_pil.map(|s| (s, *codes::ability::id::BURROW_DETECTION_FLYERS)));
        pils.retain(|&(s, _)| code_of(s) == Some(*codes::class::IN_RANGE_LISTENER));
        pils
    }

    /// A unit's listeners' chain (`` `rtl `` and `` `pil ``), by birth, the newest its
    /// head: its kept ones, its spells', those on its mana, its in-range ones, and its
    /// held items' abilities'.
    pub(crate) fn unit_listener_chain(&self, h: u32) -> Vec<(u32, u32)> {
        let Some(u) = self.unit(h) else { return Vec::new() };
        let code_of = |s: u32| self.tags.live.get(&s).map(|e| e.0);
        let pils = self.unit_pils(h);
        let is = |s: u32, c: &[u8; 4]| code_of(s) == Some(*c);
        // (Not a kept slot another object has taken since.)
        let mut chain: Vec<(u32, u32)> = u.extra.object_slots[1].iter().chain(&u.extra.object_slots[2]).filter(|&&(s, b)| self.tagged(s).1 == b).map(|&(s, _)| s).chain(u.extra.spell_timers.iter().copied()).chain(u.extra.mana_listeners.iter().map(|l| l.1)).chain(pils.iter().map(|p| p.0)).filter(|&s| is(s, codes::class::THRESHOLD_LISTENER) || is(s, codes::class::IN_RANGE_LISTENER)).map(|s| self.tagged(s)).collect();
        chain.extend(self.held_item_objects(h).into_iter().filter(|x| &x.1 == codes::class::THRESHOLD_LISTENER).map(|x| x.0));
        chain.sort_by_key(|x| x.1);
        chain.dedup();
        chain
    }

    /// A unit's in-range listeners (`` `pil ``, CPoPoInterfLis's `FUN_6f15f100`; read against
    /// the quiet test map's): each its check's period (an aura's, Ensnare's, and an
    /// inventory's half a second, a shop's 2, the alarm's 3) and its handler, linked on
    /// its unit's listeners' chain and on its position, a periodic timing from its making
    /// with its property count; its range in cells (an aura's area, a shop's DataA, the
    /// alarm's range, Ensnare's range and DataB, an inventory's 180); and the units in
    /// range, their positions in the order a search of the map finds them.
    pub fn unit_pil_records(&self, h: u32) -> Vec<(u32, w3_formats::w3z::First)> {
        use w3_formats::w3z::{First, Link, ObjectBase};
        let Some(u) = self.unit(h) else { return Vec::new() };
        let first = &u.extra.object_slots[0];
        if first.len() < 10 {
            return Vec::new();
        }
        let me = first[0];
        let pils = self.unit_pils(h);
        let chain = self.unit_listener_chain(h);
        // Its position's: the same, by birth.
        let mut on_pos: Vec<(u32, u32)> = pils.iter().map(|p| self.tagged(p.0)).collect();
        on_pos.sort_by_key(|x| x.1);
        on_pos.dedup();
        let ability = |c: &[u8; 4]| u.abilities.iter().map(|(a, _)| *a).find(|a| self.objects.abilities.get(*a).is_some_and(|o| crate::objects::id_from_str(o.str("code").trim()) == *c)).and_then(|a| self.objects.abilities.get(a));
        let mut out = Vec::new();
        for (s, c) in pils {
            let t = self.tagged(s);
            let (period, range) = match &c {
                codes::ability::id::NEUTRAL_BUILDING => (2.0, ability(&c).map_or(0.0, |o| o.level_real("DataA", 1))),
                codes::ability::id::ALARM => (3.0, self.objects.abilities.get(*codes::ability::id::ALARM).map_or(0.0, |o| o.level_real("Rng", 1))),
                codes::ability::id::ENSNARE => (0.5, ability(&c).map_or(0.0, |o| o.level_real("Rng", 1) + o.level_real("DataB", 1))),
                codes::ability::id::INVENTORY => (0.5, 180.0),
                codes::ability::id::BURROW_DETECTION_FLYERS => (0.5, self.objects.abilities.get(*codes::ability::id::BURROW_DETECTION_FLYERS).map_or(900.0, |o| o.level_real("Rng", 1))),
                // (Its range provisional: the ability's cast range, +0x50 not yet read.)
                codes::ability::id::ARCH_MAGE_BLIZZARD | codes::ability::id::PIT_LORD_RAIN_OF_FIRE => (0.5, ability(&c).map_or(0.0, |o| o.level_real("Rng", 1))),
                _ => (0.5, ability(&c).map_or(0.0, |o| o.level_real("Area", 1))),
            };
            let made = self.made_abs(s).map_or(0.0, f32::from_bits);
            let mut due = crate::efloat::add(made.to_bits(), (period as f32).to_bits());
            // (One due since the tick began still pending: the property module runs after
            // the agent timers; read, the auras' due at 5.0 pending where the script's
            // timer due then has fired.)
            while f32::from_bits(due) < self.time - 0.0205 {
                due = crate::efloat::add(due, (period as f32).to_bits());
            }
            let count = self.tags.prop_counts.get(&s).copied().unwrap_or(0);
            let k = chain.iter().position(|&x| x == t);
            let newer = k.and_then(|k| chain.get(k + 1)).map_or(NONE, |&x| rf(x));
            let older = k.filter(|&k| k > 0).map_or(NONE, |k| rf(chain[k - 1]));
            // (Chained on its position too, newest first: read, the hero test map's
            // and every-unit test map's units with two.)
            let k = on_pos.iter().position(|&x| x == t);
            let pos_newer = k.and_then(|k| on_pos.get(k + 1)).map_or(Link(NONE, 0), |&x| Link(rf(x), 1));
            let pos_older = k.filter(|&k| k > 0).map_or(Link(NONE, 0), |k| Link(rf(on_pos[k - 1]), 1));
            // The units in range, positions by the map's search.
            // (In range with its collision: read, a Necropolis a ghoul's inventory's.)
            let near: Vec<u32> = self.unit_order.iter().copied().filter(|&v| v != h && self.unit(v).is_some_and(|w| !w.dead && !w.hidden && (w.x - u.x).hypot(w.y - u.y) - self.objects.units.get(w.type_id).map_or(0.0, |o| o.real("collision")) <= range)).collect();
            let near = crate::natives::units::spatial_order(self, near);
            // (A detector's entries marked 2: read, the whelps'.)
            let mark = if c == *codes::ability::id::BURROW_DETECTION_FLYERS { 2 } else { 0 };
            let entries: Vec<[u32; 3]> = near.iter().filter_map(|&v| self.unit(v).and_then(|w| w.extra.object_slots[0].get(1).copied())).map(|p| [p.0, p.1, mark]).collect();
            let object = ObjectBase {
                real: period as f32,
                words: [pil_handler(&c), 0x2019_0400],
                links: [(Link(newer, 0), Link(older, 0), rf(t), rf(me)), (pos_newer, pos_older, rf(t), rf(first[1]))],
                timings: [Timing(Some((f32::from_bits(due), period as f32, count, 0, 1))), Timing(None)],
            };
            out.push((s, First::Interference { object, real: range / 32.0, word: 0, array: super::in_range_array(entries) }));
        }
        out
    }
}

impl World {
    /// The accelerator map's four grids as saved (`FUN_6f163120`; NIpse's
    /// CAcceleratorMap, `crate::accel`): each cell a word of two bits a layer (ground,
    /// then the 0x80, 0x40, and 0x04 layers, from bit 14 down), open 0, refused 1, mixed
    /// 2; a cell refuses a layer by its static byte (the map's flags with 0x01, and 0x10
    /// where 0x02) or a destructable's footprint (its types 0xc2, 0x10, 0x08, provisional
    /// for other destructables than trees).
    pub fn saved_grids(&self) -> Vec<Vec<u16>> {
        let Some(pyramid) = self.accel_pyramid() else { return Vec::new() };
        pyramid
            .levels
            .iter()
            .map(|l| {
                l.cells
                    .iter()
                    .map(|c| c.iter().enumerate().fold(0u16, |acc, (k, s)| acc | (match s { crate::accel::State::Open => 0, crate::accel::State::Blocked => 1, crate::accel::State::Mixed => 2 } << (14 - 2 * k))))
                    .collect()
            })
            .collect()
    }

    /// The accelerator pyramid as the map stands (`crate::accel`): a cell refuses a
    /// layer by its static byte or a destructable's or structure's footprint.
    pub fn accel_pyramid(&self) -> Option<crate::accel::Pyramid> {
        self.accel_pyramid_with(None)
    }

    /// The pyramid a walk's coarse way is planned over: every structure in it but the
    /// walk's target (`except`; provisional: read, the altar's acolyte's way from beside
    /// its haunted mine going south about it, as a mine in the pyramid gives, the
    /// acolytes' ways to the mine reaching its middle, as one open there gives).
    pub(crate) fn accel_pyramid_with(&self, except: Option<u32>) -> Option<crate::accel::Pyramid> {
        self.accel_pyramid_lifting(except, &[])
    }

    /// The same with the destructables `lifted` out of it, as a search lifts its ends'
    /// (`FUN_6f059590`'s `FUN_6f15d360` about its `FUN_6f1627e0`).
    pub(crate) fn accel_pyramid_lifting(&self, except: Option<u32>, lifted: &[u32]) -> Option<crate::accel::Pyramid> {
        let p = self.pathing.as_ref()?;
        let (w, h) = (p.width as usize, p.height as usize);
        let mut regions = vec![0u8; w * h];
        for &dh in &self.destructable_order {
            let Some(crate::types::Obj::Destructable(d)) = self.handles.get(dh) else { continue };
            if d.object_slots.len() == 6 && !lifted.contains(&dh) {
                for &(x, y) in &d.pathing_cells {
                    if x >= 0 && y >= 0 && (x as usize) < w && (y as usize) < h {
                        regions[y as usize * w + x as usize] |= 0xda;
                    }
                }
            }
        }
        // Structures' footprints the same (their pathing textures' blocked cells; read:
        // types 0xc2 and 0x10 over them, 0x08 over where none may build).
        for &uh in &self.unit_order {
            let Some(u) = self.unit(uh) else { continue };
            if !u.is_building || u.hidden || u.extra.footprint_slots.len() != 3 {
                continue;
            }
            // (Gold mines, haunted or entangled too, are not in it: read, Forestwalk's
            // undead computer's acolyte's coarse way to its haunted mine reaching the
            // mine's middle, (175, 258), as only an open cell there gives.)
            if except.map_or(self.is_gold_mine(uh), |e| e == uh) {
                continue;
            }
            for &(x, y) in &u.pathing_cells {
                if x >= 0 && y >= 0 && (x as usize) < w && (y as usize) < h {
                    regions[y as usize * w + x as usize] |= 0xd2;
                }
            }
        }
        let masks = [(0x06u8, 0x06u8), (0x80, 0x80), (0x40, 0x40), (0x04, 0x04)];
        Some(crate::accel::Pyramid::build(w, h, |x, y, layer| {
            let k = y as usize * w + x as usize;
            let f = p.flags[k];
            let stat = f | 0x01 | if f & 0x02 != 0 { 0x10 } else { 0 };
            stat & masks[layer].0 != 0 || regions[k] & masks[layer].1 != 0
        }))
    }
}

impl World {
    /// The pathing map's cells as saved (the second proximity map, `FUN_6f14d620`): each
    /// cell's flags (its static byte, 0x20 where blighted, in its top byte) and the static
    /// regions over it (the destructables' and structures' footprints; not the units'
    /// regions nor the places' of no type), the newest first.
    pub fn saved_pathing_cells(&self) -> Vec<(u32, u32, u32, Vec<Ref>)> {
        let Some(p) = self.pathing.as_ref() else { return Vec::new() };
        let (w, h) = (p.width as usize, p.height as usize);
        let mut over: Vec<Vec<(u32, u32)>> = vec![Vec::new(); w * h];
        let mut add = |x: i64, y: i64, t: (u32, u32)| {
            if x >= 0 && y >= 0 && (x as usize) < w && (y as usize) < h {
                over[y as usize * w + x as usize].push(t);
            }
        };
        for &dh in &self.destructable_order {
            let Some(crate::types::Obj::Destructable(d)) = self.handles.get(dh) else { continue };
            if d.object_slots.len() < 3 {
                continue;
            }
            if d.object_slots.len() == 6 {
                for k in 3..6 {
                    for &(x, y) in &d.pathing_cells {
                        add(x as i64, y as i64, self.tagged(d.object_slots[k]));
                    }
                }
            }
        }
        for &uh in &self.unit_order {
            let Some(u) = self.unit(uh) else { continue };
            if u.is_building && !u.hidden && u.extra.footprint_slots.len() == 3 {
                for (k, &s) in u.extra.footprint_slots.iter().enumerate() {
                    let cells = if k < 2 { &u.pathing_cells } else { &u.build_cells };
                    for &(x, y) in cells {
                        add(x as i64, y as i64, self.tagged(s));
                    }
                }
            }
        }
        let mut out = Vec::with_capacity(w * h);
        for y in 0..h {
            for x in 0..w {
                let k = y * w + x;
                let f = p.flags[k];
                let flags = f | 0x01 | if f & 0x02 != 0 { 0x10 } else { 0 };
                let mut refs = over[k].clone();
                refs.sort_by_key(|t| std::cmp::Reverse(t.1));
                out.push((x as u32, y as u32, (flags as u32) << 24, refs.into_iter().map(rf).collect()));
            }
        }
        out
    }
}

impl World {
    /// The regions section as saved (`FUN_6f15c750`, `w3_formats::w3z::PathingMaps`):
    /// the two proximity maps' and four grids' references (`^pma`, `^ama`), the coarse
    /// map of 8-cell blocks (a cell and a half's border each side: none of its cells
    /// written), the pathing map (`saved_pathing_cells`), each with three words (its
    /// search mark, the running game's, `marks`; 0x11223344, 0xb8a008f0), the
    /// accelerator's grids (`saved_grids`), and the `l^sy` and `l^ac` references.
    pub fn saved_pathing_maps(&self, marks: [u32; 2]) -> Option<w3_formats::w3z::PathingMaps> {
        use w3_formats::w3z::{Grid, MapFrame, PathingMaps, ProximityMap};
        let p = self.pathing.as_ref()?;
        let (w, h) = (p.width, p.height);
        let frame = |w: u32, h: u32, scale: f32| MapFrame { width: w, height: h, bounds: [0.0, 0.0, h as f32, w as f32], cells: [0, 0, h, w], scale: [scale, 1.0 / scale] };
        let of = |code: &[u8; 4]| {
            let mut v: Vec<(u32, u32)> = self.tags.live.iter().filter(|(_, (c, _))| c == code).map(|(&s, (_, b))| (s, *b)).collect();
            v.sort();
            v
        };
        let (pma, ama, sy, ac) = (of(codes::class::PATHING_MAP), of(codes::class::ACCELERATOR_GRID), of(codes::class::LONG_RANGE_SYSTEM), of(codes::class::LONG_RANGE_ACCELERATOR));
        if pma.len() != 2 || ama.len() != 4 || sy.is_empty() || ac.is_empty() {
            return None;
        }
        let words = |mark: u32| [mark, 0x1122_3344, 0xb8a0_08f0];
        let grids: Vec<Vec<u16>> = self.saved_grids();
        let levels = crate::accel::Pyramid::build(w as usize, h as usize, |_, _, _| false).levels;
        let mut gs: Vec<Grid> = Vec::new();
        for (k, l) in levels.iter().enumerate() {
            gs.push(Grid { frame: frame(l.width as u32, l.height as u32, (2u32 << k) as f32), cells: grids.get(k).cloned().unwrap_or_default() });
        }
        Some(PathingMaps {
            refs: [rf(pma[0]), rf(pma[1]), rf(ama[0]), rf(ama[1]), rf(ama[2]), rf(ama[3])],
            proximity: [
                ProximityMap { frame: frame(w / 8 + 3, h / 8 + 3, 8.0), words: words(marks[0]), cells: Vec::new() },
                ProximityMap { frame: frame(w, h, 1.0), words: words(marks[1]), cells: self.saved_pathing_cells() },
            ],
            grids: gs.try_into().ok()?,
            tail: [rf(sy[0]), rf(ac[0])],
        })
    }
}
