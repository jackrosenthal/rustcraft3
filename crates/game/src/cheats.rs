//! Single-player cheat codes, typed as chat (game.dll's console commands, CONSOLE.CPP).
//! Each is known only by a 64-bit hash of its word, kept in Units\Telemetry.txt under
//! [Telemetry] a to v (FUN_6f379bc0 registers them); a typed word is hashed with
//! Storm's SStrHash64 (ordinal 580) and matched.  The switches toggle bits of the cheat
//! flags (CCheatData) and announce CHEATENABLED or CHEATDISABLED; the rest act at once.

use w3_jass::Vm;

use crate::types::player_state;
use crate::world::{Request, World};

/// Cheat flag bits (CCheatData +4), as the game reads them.
pub mod flag {
    /// thedudeabides: abilities have no cooldown.
    pub const COOLDOWN: u32 = 0x1;
    /// whosyourdaddy: users' units take no damage from computers' and deal them 100 times
    /// as much (CUnit vf72).
    pub const GOD: u32 = 0x2;
    /// warpten: building, training, and research go 16 times as fast (FUN_6f66bd60).
    pub const FAST_BUILD: u32 = 0x4;
    /// iocainepowder: corpses decay 16 times as fast (FUN_6f69c000).
    pub const FAST_DEATH: u32 = 0x8;
    /// pointbreak: food is not limited (FUN_6f266680).
    pub const FOOD: u32 = 0x10;
    /// thereisnospoon: spells cost no mana.
    pub const MANA: u32 = 0x20;
    /// strengthandhonor and itvexesme (IsNoDefeatCheat, IsNoVictoryCheat).
    pub const NO_DEFEAT: u32 = 0x40;
    pub const NO_VICTORY: u32 = 0x80;
    /// whoisjohngalt: a hero skill may be learned below its hero level (FUN_6f528d20).
    pub const HERO_LEVEL: u32 = 0x100;
    /// iseedeadpeople: the whole map is seen.
    pub const SHOW_MAP: u32 = 0x200;
    /// synergy: the tech tree's requirements are met.
    pub const TECH_TREE: u32 = 0x400;
    /// daylightsavings: the day stops.
    pub const TIME_STOPPED: u32 = 0x800;
}

/// The speed factor of the fast cheats (0x6fd3c784).
pub const FAST: f32 = 16.0;

/// Storm's SStrHash64 table (0x15043f38).
const TABLE: [u64; 16] = [
    0x486e26eedcaa16b3, 0xe1918eef202dafdb, 0x341c7dc71c365303, 0x40ef2d3765fd5e49, 0xd6057177904ece93, 0x1c38024f98fd323b, 0xe3061ae7a39b0fa1, 0x9797f25fe4444563,
    0xcd2ec20c8dc1b898, 0x31759633799a306d, 0x8c2063852e6e9627, 0x79237d9973922c66, 0x8728628d28628824, 0x8f1f7e9625887795, 0x296e3281389c0d60, 0x6f4893ca61636542,
];

/// Storm's SStrHash64 (ordinal 580) with its default seed: letters folded to upper case
/// and slashes to backslashes.
pub fn hash64(s: &str) -> u64 {
    let (mut seed, mut state): (u64, u64) = (0x7fed7fed7fed7fed, 0xeeeeeeeeeeeeeeee);
    for &b in s.as_bytes() {
        let mut c = b as u64;
        if b.is_ascii_lowercase() {
            c -= 0x20;
        }
        if c == 0x2f {
            c = 0x5c;
        }
        seed = TABLE[(c >> 4) as usize].wrapping_add(TABLE[(c & 15) as usize]) ^ state.wrapping_add(seed);
        state = state.wrapping_mul(33).wrapping_add(c).wrapping_add(seed).wrapping_add(3);
    }
    if seed == 0 { 1 } else { seed }
}

impl World {
    pub fn cheat(&self, bit: u32) -> bool {
        self.cheat_flags & bit != 0
    }

    /// Which [Telemetry] entry a word's hash is.
    fn cheat_key(&self, word: &str) -> Option<char> {
        let text = self.chain.read_string("Units\\Telemetry.txt").ok()?;
        let h = hash64(word);
        let p = w3_formats::profile::Profile::parse(&text);
        let keys = p.sections.get("telemetry")?;
        keys.iter().find(|(_, v)| v.trim().parse::<i64>().ok().map(|n| n as u64) == Some(h)).and_then(|(k, _)| k.chars().next())
    }

    /// The cheat announcement (FUN_6f379aa0).
    fn announce(&mut self, on: bool) {
        let text = self.localized(if on { "CHEATENABLED" } else { "CHEATDISABLED" });
        self.requests.push(Request::DisplayText { players: 1 << self.local_player, x: 0.0, y: 0.0, text, duration: -1.0 });
    }

    fn toggle(&mut self, bit: u32) -> bool {
        self.cheat_flags ^= bit;
        let on = self.cheat_flags & bit != 0;
        self.announce(on);
        on
    }

    /// The quantity cheats' player: the one named (0 to 15), else the one typing.
    fn cheat_player(&self, arg: Option<&str>, player: u8) -> Option<u8> {
        match arg.and_then(|a| a.parse::<i64>().ok()) {
            None | Some(-1) => Some(player),
            Some(n) if (0..16).contains(&n) => Some(n as u8),
            _ => None,
        }
    }

    fn add_resource(&mut self, player: u8, state: i32, amount: i32) {
        if let Some(p) = self.players.get_mut(player as usize) {
            *p.state.entry(state).or_insert(0) += amount;
        }
    }

    /// A chat line from `player` as a cheat; returns whether it was one.
    pub fn console_command(&mut self, vm: &mut Vm, player: u8, text: &str) -> Option<crate::types::EventCtx> {
        let mut words = text.split_whitespace();
        let word = words.next()?;
        let key = self.cheat_key(word)?;
        let args: Vec<&str> = words.collect();
        let _ = vm;
        let amount = |a: Option<&&str>| a.and_then(|v| v.parse::<i32>().ok()).unwrap_or(500);
        match key {
            'a' => {
                self.toggle(flag::FAST_BUILD);
            }
            'b' => {
                self.toggle(flag::FAST_DEATH);
            }
            'c' => {
                self.toggle(flag::GOD);
            }
            'g' => {
                self.toggle(flag::FOOD);
            }
            'h' => {
                self.toggle(flag::MANA);
            }
            'i' => {
                if self.toggle(flag::COOLDOWN) {
                    for &u in &self.unit_order.clone() {
                        if let Some(u) = self.unit_mut(u) {
                            u.cooldowns.clear();
                        }
                    }
                }
            }
            'j' => {
                self.toggle(flag::NO_DEFEAT);
            }
            'k' => {
                self.toggle(flag::NO_VICTORY);
            }
            'l' => {
                self.toggle(flag::HERO_LEVEL);
            }
            'm' => {
                let on = self.toggle(flag::SHOW_MAP);
                if on {
                    self.cheat_fog = Some((self.fog_enabled, self.fog_mask_enabled));
                    self.fog_enabled = false;
                    self.fog_mask_enabled = false;
                } else if let Some((f, m)) = self.cheat_fog.take() {
                    self.fog_enabled = f;
                    self.fog_mask_enabled = m;
                }
            }
            'n' => {
                self.toggle(flag::TECH_TREE);
            }
            // keysersoze, leafittome, greedisgood [amount] [player].
            'd' | 'e' | 'f' => {
                let Some(p) = self.cheat_player(args.get(1).copied(), player) else { return Some(Default::default()) };
                let n = amount(args.first());
                if key != 'e' {
                    self.add_resource(p, player_state::RESOURCE_GOLD, n);
                }
                if key != 'd' {
                    self.add_resource(p, player_state::RESOURCE_LUMBER, n);
                }
                self.announce(true);
            }
            // riseandshine and lightsout: [Misc] Dawn and Dusk; daylightsavings [hour]
            // sets the hour, or stops and starts the day.
            'o' | 'p' => {
                self.time_of_day = self.misc_f32(if key == 'o' { "Dawn" } else { "Dusk" }, if key == 'o' { 6.0 } else { 18.0 });
                self.announce(true);
            }
            'q' => match args.first().and_then(|v| v.parse::<f32>().ok()) {
                Some(h) => {
                    self.time_of_day = h;
                    self.announce(true);
                }
                None => {
                    let on = self.toggle(flag::TIME_STOPPED);
                    self.time_of_day_suspended = on;
                }
            },
            // sharpandshiny: every upgrade researched.
            'r' => {
                let ids: Vec<_> = self.objects.upgrades.objects.keys().copied().collect();
                for id in ids {
                    let max = self.objects.upgrades.get(id).map_or(1, |o| o.int("maxlevel").max(1));
                    if let Some(pl) = self.players.get_mut(player as usize) {
                        pl.tech_researched.insert(id, max);
                    }
                }
                self.research_changed(player);
                self.announce(true);
            }
            // tenthleveltaurenchieftain: Power of the Horde.
            'v' => self.requests.push(Request::PlayMusic("Sound\\Music\\mp3Music\\PH1.mp3".into())),
            // somebodysetupusthebomb, allyourbasearebelongtous: the player's defeat or
            // victory event (0x8020d, 0x8020e).
            't' | 'u' => {
                self.announce(true);
                let event = if key == 't' { crate::event_ids::EVENT_PLAYER_DEFEAT } else { crate::event_ids::EVENT_PLAYER_VICTORY };
                return Some(crate::types::EventCtx { player, has_player: true, event_id: event, ..Default::default() });
            }
            // motherland [race] [mission]: that campaign's mission, with those before it
            // opened (FUN_6f379c60).
            's' => {
                let race = args.first().copied().unwrap_or("human");
                let n = args.get(1).and_then(|v| v.parse::<usize>().ok()).unwrap_or(1).max(1) - 1;
                let file = if self.version >= 1 { "UI\\CampaignStrings_exp.txt" } else { "UI\\CampaignStrings.txt" };
                let p = w3_formats::profile::Profile::parse(&self.chain.read_string(file).unwrap_or_default());
                let index = p.get_list("Index", "CampaignList").iter().position(|c| c.trim().trim_matches('"').eq_ignore_ascii_case(race));
                let map = p.get_list(race, &format!("Mission{n}")).get(2).map(|m| m.trim().trim_matches('"').to_string());
                if let (Some(c), Some(map)) = (index, map) {
                    for m in 0..=n {
                        self.persist.set_mission(c, m, true);
                    }
                    self.announce(true);
                    self.requests.push(Request::ChangeLevel { map, score: false });
                }
            }
            _ => return None,
        }
        Some(Default::default())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn hash_matches_storm() {
        // [Telemetry] f in Units\Telemetry.txt.
        assert_eq!(super::hash64("greedisgood") as i64, 2354477290916240923);
    }
}
