//! A unit's own record (`+w3u`'s second, CUnit's `FUN_6f696490`), read against the quiet
//! test map's 71 units at 5 seconds: its agent flags, its handlers (who listens to it,
//! and for what), its widget, its triggers' events, and its 109 fields.

use w3_formats::w3z::{Field, Handlers, Ref, Second, Sprite, Timing, Widget};

use super::{rf, NONE};
use crate::world::World;

/// The events (less 0xd0000) an agent of a kind listens to on the unit it serves, as
/// it registers when made (read: the quiet test map's units' registries, and traced as
/// made at `FUN_6f0725b0`).  Provisional for kinds not read there: none.
fn listened(code: &[u8; 4]) -> &'static [u16] {
    match code {
        b"Amov" => &[0x144, 0x161, 0x166, 0x16b, 0x16c, 0x16d, 0x16e, 0x16f, 0x170, 0x171, 0x172, 0x173, 0x174, 0x175, 0x176, 0x177, 0x178, 0x17f, 0x18e, 0x193, 0x29d],
        b"Aatk" => &[0x144, 0x148, 0x149, 0x14a, 0x14b, 0x14c, 0x14d, 0x14e, 0x14f, 0x161, 0x163, 0x165, 0x166, 0x168, 0x169, 0x16a, 0x1a2, 0x1a5, 0x2a3, 0x2a4],
        b"ACsp" => &[0x1a2, 0x1d5, 0x1d6],
        b"AHbu" | b"AObu" | b"AEbu" => &[0x144, 0x161, 0x179, 0x17a, 0x1a2, 0x29c, 0x2a5, 0x2a6],
        b"AUbu" => &[0x144, 0x161, 0x162, 0x179, 0x17a, 0x1a2, 0x29c, 0x2a5, 0x2a6],
        b"Ahar" => &[0x144, 0x150, 0x151, 0x152, 0x161, 0x166, 0x167, 0x182, 0x183, 0x184, 0x185, 0x186, 0x187, 0x191, 0x1c5],
        b"Ahrl" => &[0x144, 0x150, 0x151, 0x152, 0x161, 0x166, 0x167, 0x184, 0x185, 0x186, 0x187, 0x1c5],
        b"Aaha" => &[0x144, 0x166, 0x182, 0x183],
        b"Amil" => &[0x144, 0x188],
        // Spells cast on their own (autocast).
        b"Arep" | b"Aadm" | b"Anhe" | b"Ainf" | b"Arst" | b"Ahea" | b"Aren" => &[0x144, 0x148, 0x14c, 0x15f, 0x160, 0x161, 0x1a2, 0x2a3, 0x2a4],
        // Spells cast at a target, and one cast at none.
        b"ACtb" | b"AOsh" | b"Aens" | b"Acan" | b"Alam" | b"Auns" => &[0x144, 0x15f, 0x160, 0x161],
        b"ACtc" => &[0x144, 0x15f, 0x160],
        b"AInv" => &[0x144, 0x17c, 0x17d, 0x17e, 0x1a0, 0x1a2, 0x1a7, 0x299],
        b"Ablr" => &[0x1a0, 0x1a2, 0x1a7],
        b"Awan" => &[0x148, 0x14c, 0x177, 0x178, 0x1a2],
        b"Aalr" => &[0x1a2],
        b"Afih" | b"Afiu" | b"Afio" | b"Afin" | b"Abgm" => &[0x1a0],
        b"Avul" => &[0x1a1, 0x1a7],
        b"Aarm" | b"AHad" | b"AHab" | b"AUau" | b"AOae" | b"AEar" | b"AUav" | b"AEah" | b"Aakb" | b"AOac" | b"Aoar" => &[0x1a0, 0x1a1, 0x1a2, 0x1a7],
        b"Asel" => &[0x1a2],
        b"Aneu" => &[0x1a5],
        b"Aque" => &[0x1a0, 0x1a1, 0x1a2],
        b"Aupg" => &[0x1a1, 0x1a2],
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
        b"Aens" => 0xd_01bf,
        b"Aneu" | b"AInv" => 0xd_01aa,
        b"Aalr" => 0xd_01c8,
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
        let Ok(data) = self.chain.read(model) else { return Vec::new() };
        let Ok(m) = w3_formats::mdx::Model::parse(&data) else { return Vec::new() };
        m.sequences
            .iter()
            .enumerate()
            .filter(|(_, s)| {
                let words: Vec<&str> = s.name.split(|c: char| c.is_whitespace() || c == '-').filter(|w| !w.is_empty() && !w.chars().all(|c| c.is_ascii_digit())).collect();
                words.len() == 1 && words[0].eq_ignore_ascii_case("stand")
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
        let invulnerable = u.abilities.iter().any(|(a, _)| a == b"Avul");
        let computer = self.players.get(u.owner as usize).is_some_and(|p| p.controller == crate::types::map_control::COMPUTER && p.slot_state == crate::types::slot_state::PLAYING);
        let creep = u.owner == crate::types::PLAYER_NEUTRAL_AGGRESSIVE;
        let types: Vec<String> = o.list("type").iter().map(|t| t.trim().to_ascii_lowercase()).collect();
        let is = |t: &str| types.iter().any(|x| x == t);

        // The agent's flags (+0x20).
        let mut own = 0x600 | if hidden { 0x5 } else { 0x6 };
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
                Some("Ahar") => own |= 0x4_8000,
                Some("Ahrl") => own |= 0x4_0000,
                Some("Aaha") => own |= 0x1_0000,
                _ => {}
            }
        }

        // Who listens to it, each with its stamp: as made, in the order made.
        let abilities: Vec<(u32, u32, [u8; 4])> = u.extra.object_slots[1].iter().chain(&u.extra.object_slots[2]).filter_map(|&(s, b)| code_of(s).filter(|c| c[0] == b'A' && c != b"AI-U").map(|c| (s, b, c))).collect();
        let mut agents: Vec<(Ref, (u32, u32), Vec<u16>)> = vec![(rf(me), (me.1, 0), SELF_EVENTS.to_vec())];
        for &(s, b, c) in &abilities {
            let mut events = listened(&c).to_vec();
            // (A computer's structure's alarm listens for its owner's AI too, read: the
            // undead's Necropolis and Haunted Gold Mine.)
            if &c == b"Aalr" && computer {
                events.push(0x1a5);
            }
            // (A haunted mine's listens as it starts to serve, after its AIs.)
            if !events.is_empty() && &c != b"Abgm" {
                agents.push((Ref(s, b), (b, 0), events));
            }
        }
        let made = abilities.iter().map(|a| a.1).max().unwrap_or(me.1);
        if let Some(a) = abilities.iter().find(|a| &a.2 == b"Abgm") {
            agents.push((Ref(a.0, a.1), (made, 9), listened(b"Abgm").to_vec()));
        }
        if abilities.iter().any(|a| &a.2 == b"Aque") {
            own |= 0x20_0000;
        }
        // A structure's own again after its abilities.
        if building {
            agents.push((rf(me), (made, 1), vec![0x15e]));
        }
        // Its rally point (a second table's agent), with its abilities.
        if let Some(&(r, _)) = rallies.iter().find(|(_, owner)| *owner == rf(me)) {
            let at = abilities.iter().find(|a| &a.2 == b"Aupg").map_or(made, |a| a.1);
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
        let mine = u.abilities.iter().any(|(a, _)| matches!(a, b"Agld" | b"Abgm" | b"Aegm"));
        let nearest_site = || self.tags.sites.iter().min_by(|a, b| (a.0 - u.x).hypot(a.1 - u.y).total_cmp(&(b.0 - u.x).hypot(b.1 - u.y))).map(|s| s.2);
        if mine {
            let site = u.extra.site_ai.or_else(|| if u.abilities.iter().any(|(a, _)| a == b"Abgm") { nearest_site() } else { None });
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
        for b in u.buffs.iter().filter(|b| b.slot != 0) {
            let t = self.tagged(b.slot);
            if !buffs.contains(&t) {
                buffs.push(t);
            }
        }
        for t in buffs {
            // (Devotion Aura's on another listens to it not for 0x1a2: read, the ogres'.)
            let events = if code_of(t.0) == Some(*b"BHad") && !u.extra.object_slots[2].contains(&t) { vec![0x1a1, 0x1a7] } else { BUFF_EVENTS.to_vec() };
            agents.push((rf(t), (t.1, 0), events));
        }
        for &v in &self.unit_order {
            if v == h {
                continue;
            }
            for b in self.unit(v).into_iter().flat_map(|w| w.buffs.iter()).filter(|b| b.slot != 0 && b.caster == h) {
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
        agents.sort_by_key(|a| a.1);
        let mut events = vec![Vec::new(); 0x400];
        for (r, _, es) in &agents {
            for &e in es {
                events[e as usize].push((*r, 0xd_0000 + e as u32));
            }
        }
        let mut fixed: [Vec<(Ref, u32)>; 5] = Default::default();
        if !hidden {
            fixed[0].push((rf(self.game_object()), 0x8_0267));
        }
        if let Some(a) = abilities.iter().find(|a| &a.2 == b"Amov") {
            fixed[3].push((Ref(a.0, a.1), 0xd_0199));
            fixed[4].push((Ref(a.0, a.1), 0xd_019a));
        }
        // Its listeners' observers, keyed by the listener: a hash of four buckets by
        // slot, each in the order made.
        let ability_ref = |c: &[u8; 4]| abilities.iter().find(|a| &a.2 == c).map(|a| Ref(a.0, a.1));
        let mut keyed: Vec<((u32, u32), (Ref, u32))> = Vec::new();
        if let Some(&d) = u.extra.object_slots[1].iter().find(|&&(s, _)| code_of(s) == Some(*b"`rtl")) {
            keyed.push((d, (rf(me), 0xd_019f)));
        }
        for &(s, c) in &u.extra.spell_owners {
            if code_of(s) != Some(*b"`rtl") {
                continue;
            }
            let rising = u.extra.mana_listeners.iter().any(|&(t, x)| x == s && t.is_finite());
            if let Some(a) = ability_ref(&c) {
                keyed.push((self.tagged(s), (a, if rising { 0xd_01b6 } else { 0xd_01b7 })));
            }
        }
        let mut pils: Vec<(u32, [u8; 4])> = Vec::new();
        let rest = &u.extra.object_slots[2];
        for (k, &(s, _)) in rest.iter().enumerate() {
            if code_of(s) == Some(*b"`pil") {
                if let Some(c) = rest[..k].iter().rev().filter_map(|&(x, _)| code_of(x)).find(|c| c[0] == b'A') {
                    pils.push((s, c));
                }
            }
        }
        pils.extend(u.extra.alarm_pil.map(|s| (s, *b"Aalr")));
        pils.extend(u.extra.ensnare_pil.map(|p| (p.1, *b"Aens")));
        pils.extend(u.extra.item_pil.map(|p| (p.0, *b"AInv")));
        for (s, c) in pils {
            if code_of(s) == Some(*b"`pil") {
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
        let model = format!("{}.mdx", o.str("file"));
        let stands = self.stand_sequences(&model);
        let sprite = Sprite {
            real: f32::from_bits(0x0344_0000 | if building { 0x0008_0008 } else { 0 }),
            short: stands.first().copied().unwrap_or(0) as u16,
            byte: 255,
            word: tint,
            reals: [o.real("modelScale"), 1.0],
            words: [4, 0],
            last_real: f32::from_bits(150),
            last_byte: 1,
            flags: [1, 1, 1],
            entries: vec![(0.0, 4, stands.clone()), (0.0, 4, stands)],
            model,
        };
        let footprint: Vec<Ref> = u.extra.footprint_slots.iter().map(|&s| rf(self.tagged(s))).collect();
        let widget = Widget {
            sprite: Some(sprite),
            owner: u.owner as u32,
            word: u32::from_be_bytes(u.type_id),
            shorts: if building && u.owner as usize >= crate::types::PLAYER_NEUTRAL_AGGRESSIVE as usize { [4095, 0x8001] } else { [0, 0x8001] },
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
        let rcm: Vec<(u32, u32)> = u.extra.object_slots[1].iter().copied().filter(|&(s, _)| code_of(s) == Some(*b"`rcm")).collect();
        let death = u.extra.object_slots[1].iter().copied().find(|&(s, _)| code_of(s) == Some(*b"`rtl"));
        // The newest of its abilities and buffs (its abilities' chain's head, the AI's
        // unit list aside).
        let newest = abilities.iter().map(|a| (a.0, a.1)).chain(u.buffs.iter().filter(|b| b.slot != 0).map(|b| self.tagged(b.slot))).max_by_key(|a| a.1);
        let ai_unit = u.extra.ai_slots.iter().copied().find(|&(s, _)| code_of(s) == Some(*b"AI-U"));
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
        let mut flags = 0x1;
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
        if u.abilities.iter().any(|(a, _)| matches!(a, b"Agld" | b"Abgm" | b"Aegm")) {
            flags |= 0x2_0000;
        }
        let passive = u.owner as usize == crate::types::PLAYER_NEUTRAL_AGGRESSIVE as usize + 3;
        if passive && is("neutral") {
            flags |= 0x4_0000;
        }
        if passive && u.abilities.iter().any(|(a, _)| a == b"Awan") {
            flags |= 0x8_0000;
        }
        for (a, _) in &u.abilities {
            let Some(ab) = self.objects.abilities.get(*a) else { continue };
            if ab.str("code").trim() == "Artn" {
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
        let mut fields = vec![Field::Timer(NONE, 0, Timing(None)), w(u32::MAX), w(u32::MAX), w(0), w(u32::MAX), w(u32::MAX), w(0), w(0)];
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
        fields.extend(none3());
        fields.push(Field::Timer(NONE, 0, Timing(None)));
        fields.extend(none3());
        fields.extend(none3());
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
        fields.extend((52..=66).map(|_| w(0)));
        fields.extend([w(!building as u32), w(0)]);
        fields.extend([Field::Real(0.0), Field::Real(0.0), Field::Real(0.0)]);
        fields.extend([w(kinds), w(targets), w(defense), w(u32::MAX), w(u32::MAX)]);
        fields.extend([Field::Ref(NONE), Field::Ref(NONE), Field::Ref(NONE)]);
        fields.extend([Field::Real(o.real("impactZ")), Field::Real(o.real("launchX")), Field::Real(o.real("launchY")), Field::Real(o.real("launchZ"))]);
        fields.extend([Field::Byte(0), Field::Byte(0), w(0), w(0), w(0), w(0), Field::Ref(NONE), Field::Byte(0)]);
        fields.extend([w(tint), w(1), w(4), w(0), w(0), w(0), w(0), w(0)]);
        // (A spell immune unit's two words (+0x15c, +0x160: read, the granite golems' and a
        // red dragon's), its being built on another (isBuildOn, +0x60), and a flag word
        // (+0x280: 0x10 for all read, 0x8 for a type with elevation points).)
        let immune = u.abilities.iter().any(|(a, _)| self.objects.abilities.get(*a).is_some_and(|ab| ab.str("code").trim() == "Amim"));
        let built_on = if o.int("isBuildOn") != 0 { 0x100 } else { 0 };
        let elevated = if o.int("elevPts") > 0 { 8 } else { 0 };
        fields.extend([w(speed.to_bits()), w(speed.to_bits()), w(1f32.to_bits()), w(0), w(immune as u32), w(immune as u32), w(built_on), w(0), w(0x10 | elevated)]);
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
            if let Some(c) = code_of(s).filter(|c| (c[0] == b'A' && c != b"AI-U") || c[0] == b'B') {
                add((s, b), c, &mut chain);
            }
        }
        for b in u.buffs.iter().filter(|b| b.slot != 0) {
            let t = self.tagged(b.slot);
            if let Some(c) = code_of(b.slot) {
                add(t, c, &mut chain);
            }
        }
        for &(s, b) in &u.extra.ai_slots {
            if code_of(s) == Some(*b"uref") && self.tagged(s).1 == b {
                add((s, b), *b"uref", &mut chain);
            }
        }
        chain.sort_by_key(|x| x.0 .1);
        // Its rally point (the second table's agent) right after its rally ability.
        let me = u.extra.object_slots[0].first().copied().unwrap_or_default();
        if let (Some(&(r, _)), Some(k)) = (rallies.iter().find(|(_, owner)| *owner == rf(me)), chain.iter().position(|x| &x.1 == b"ARal")) {
            chain.insert(k + 1, ((r.0, r.1), *b"rlyp"));
        }
        chain
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
            if &code == b"rlyp" {
                continue;
            }
            let props: Vec<(u32, u32)> = match slots.iter().position(|&x| x == t) {
                Some(i) => slots[i + 1..].iter().copied().take_while(|&(s, b)| self.tagged(s).1 == b && self.tags.live.get(&s).is_some_and(|e| &e.0 == b"`rel")).collect(),
                None => Vec::new(),
            };
            let mut rec = super::agent_first(t, &code, &props);
            if let w3_formats::w3z::First::Agent { object, .. } = &mut rec {
                let newer = chain.get(k + 1).map_or(NONE, |c| rf(c.0));
                let older = if k > 0 { rf(chain[k - 1].0) } else { NONE };
                object.links[0] = (w3_formats::w3z::Link(newer, 0), w3_formats::w3z::Link(older, 0), rf(t), rf(me));
            }
            out.push((t.0, rec));
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
        b"Aalr" => 0x0031_0006,
        b"Avul" | b"Awan" | b"Agld" | b"Amic" | b"Amil" | b"Amim" | b"Andt" | b"Aneu" | b"Artn" => 0xfdd1_f00e,
        b"AEar" | b"AHad" => 0x0030_7006,
        b"ACtb" => 0x0401_8006,
        b"ACtc" => 0x0000_8002,
        b"AOsh" => 0x0001_000a,
        b"Aadm" => 0x0030_0056,
        b"Aarm" => 0x0430_0006,
        b"Acan" => 0xffd1_f006,
        b"Aens" => 0x0081_8006,
        b"Ahar" | b"Ahrl" => 0x0300_0040,
        b"Ainf" => 0x0000_f006,
        b"Alam" | b"Auns" => 0x0000_2008,
        b"Anhe" => 0x4430_f006,
        b"Arep" | b"Arst" => 0x4b30_640e,
        b"Aven" => 0x0400_0006,
        b"Aaha" | b"Abgm" | b"Abli" => 0xfdd1_f00e,
        _ => 0,
    }
}

impl World {
    /// The second record of an ability on a unit's chain, for the kinds read so far
    /// (read against the quiet test map's): CAbility's part (`FUN_6f42e830`: its flags,
    /// the ability made before it, five words, its targets, its code, a word, a timer,
    /// and a word), then its kind's.  None for kinds not read.
    pub fn ability_own_record(&self, h: u32, t: (u32, u32), code: &[u8; 4], older: Option<(u32, u32)>) -> Option<Second> {
        let u = self.unit(h)?;
        let o = self.objects.units.get(u.type_id)?;
        let computer = self.players.get(u.owner as usize).is_some_and(|p| p.controller == crate::types::map_control::COMPUTER && p.slot_state == crate::types::slot_state::PLAYING);
        let ai = computer || u.owner == crate::types::PLAYER_NEUTRAL_AGGRESSIVE;
        let w = Field::Word;
        let timer = || Field::Timer(NONE, 0, Timing(None));
        // A periodic agent timer it started (by its function's name): its next time due
        // after now, its period, its count, and the event it sends.
        let periodic = |name: &str, period: f32, event: u32| {
            let started = self.tags.timer_objects.get(&t.0).and_then(|v| v.iter().find(|x| x.1 == name)).map(|x| (x.0, x.2));
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
        let base_flags: u32 = match code {
            b"Avul" => 0x8000_0010,
            b"ACsp" => 0x400,
            _ => 0x10,
        };
        let flags = base_flags | if ai { 0x2 } else { 0 };
        let prev = older.map_or(NONE, rf);
        let mut own = vec![w(flags), w(prev.0), w(prev.1), w(0), w(0), w(0), w(0), w(0), w(ability_targets(code)), w(u32::from_be_bytes(*code)), w(0), timer(), w(0)];
        let tail = match code {
            // CAbilityMove (`FUN_6f5fec80`): two references, its speed and 1.0, words,
            // two reals, references; its two timers.
            b"Amov" => {
                own.extend([w(u32::MAX), w(u32::MAX), w(0), Field::Real(o.real("spd")), Field::Real(1.0), w(0), w(0), w(u32::MAX), w(u32::MAX), w(0), Field::Real(0.0), Field::Real(0.0), w(u32::MAX), w(u32::MAX), w(0), w(0)]);
                vec![timer(), timer()]
            }
            // CAbilityInvulnerable: a word; an empty list and a word.
            b"Avul" => {
                own.push(w(0));
                vec![Field::List(Vec::new()), w(0)]
            }
            // Creep sleep (`FUN_6f5224c0`, `FUN_6f5230a0`): 0.1; its sleep's timer, its
            // check every 2 seconds (event 0xd01bf), and a list.
            b"ACsp" => {
                own.push(Field::Real(0.1));
                // (The sleep's timer, as it fell asleep, else its check's: read, the
                // count of the night's creeps' second.)
                let sleep = periodic("517be0", 2.0, 0xd_01bf);
                vec![timer(), if matches!(sleep, Field::Timer(r, ..) if r != NONE) { sleep } else { periodic("524e00", 2.0, 0xd_01bf) }, Field::List(Vec::new())]
            }
            // Fire (CAbilityOnFire): a reference, and its look (event 0xd01c1) every 1.5
            // seconds and a two-thousandth of its maximum life, at most 3
            // (`FUN_6f41baf0`: read, a town hall's 2.25, a haunted mine's 1.975).
            b"Afih" | b"Afiu" | b"Afio" | b"Afin" => {
                use crate::efloat::{add, div, from_int, mul};
                let period = f32::from_bits(add(mul(u.max_life.to_bits(), div(from_int(1), from_int(2000))), div(from_int(3), from_int(2)))).clamp(1.5, 3.0);
                vec![w(u32::MAX), w(u32::MAX), w(0), periodic("41baf0", period, 0xd_01c1)]
            }
            // CAbilityAlarm (`FUN_6f42d040`): its timer and its in-range listener.
            b"Aalr" => vec![timer(), Field::Ref(u.extra.alarm_pil.map_or(NONE, |s| rf(self.tagged(s))))],
            _ => return None,
        };
        let handlers = Handlers { keyed: Vec::new(), fixed: Default::default(), events: vec![Vec::new(); 0x400] };
        Some(Second::Agent { own, handlers, tail })
    }
}
