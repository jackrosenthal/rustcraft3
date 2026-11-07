//! Start locations as the game begins (FUN_6f1e9fd0), by the map's placement
//! (SetGamePlacement): MAP_PLACEMENT_RANDOM (0) places each team in turn on the first
//! of the free start locations shuffled anew (FUN_6f1e9e60, FUN_6f1e5060); FIXED (1) and USE_MAP_SETTINGS (2) keep those the map set;
//! TEAMS_TOGETHER (3), with the setup's Teams Together (0x40), places each team, the
//! largest first, around a random free location, its other members on free locations
//! the placed ones rank (SetStartLocPrio; FUN_6f1ea2f0, FUN_6f1e5140), and otherwise
//! falls back to random placement.

use crate::types::{map_control, slot_state};
use crate::world::World;

const PRIO_LOW: i32 = 0;
const PRIO_HIGH: i32 = 1;

impl World {
    /// The players who take a start location: users and computers in the game, but those
    /// whose location the map forced (ForcePlayerStartLocation), which keep it.
    fn placed_players(&self) -> Vec<u8> {
        (0..crate::types::PLAYER_NEUTRAL_AGGRESSIVE)
            .filter(|&p| {
                let pl = &self.players[p as usize];
                pl.slot_state == slot_state::PLAYING && matches!(pl.controller, map_control::USER | map_control::COMPUTER) && pl.get_state(crate::types::player_state::OBSERVER) == 0 && !pl.start_forced
            })
            .collect()
    }

    /// The start locations the map forced for players: taken before any is placed
    /// (read: Mafarazzo TD, each of its 12 players forced by its slots' setup, no
    /// placement's draws before the original's first turn).
    fn forced_locations(&self) -> Vec<i32> {
        self.players.iter().take(crate::types::PLAYER_NEUTRAL_AGGRESSIVE as usize).filter(|p| p.start_forced).map(|p| p.start_location).collect()
    }

    /// The list shuffled as `FUN_6f1d1460` does: from the second entry on, each swapped
    /// with one at or before it (read; measured: Lost Temple's two players, each a team,
    /// placed at the original's start locations, where walking back from the last entry
    /// placed them elsewhere).
    fn shuffled<T>(&mut self, mut v: Vec<T>) -> Vec<T> {
        for i in 1..v.len() {
            let j = self.rng.int(0, i as i32) as usize;
            v.swap(i, j);
        }
        v
    }

    pub(crate) fn place_start_locations(&mut self) {
        let players = self.placed_players();
        let count = self.start_locations.len() as i32;
        if count == 0 || players.is_empty() {
            return;
        }
        match self.placement {
            0 => self.place_randomly(&players),
            3 if self.map_flags & 0x40 != 0 => {
                if !self.place_teams_together(&players) {
                    self.place_randomly(&players);
                }
            }
            3 => self.place_randomly(&players),
            _ => {}
        }
    }

    /// The players by team, the teams in the order their first players come.
    fn teams_of(&self, players: &[u8]) -> Vec<(i32, Vec<u8>)> {
        let mut teams: Vec<(i32, Vec<u8>)> = Vec::new();
        for &p in players {
            let t = self.players[p as usize].team;
            match teams.iter_mut().find(|g| g.0 == t) {
                Some(g) => g.1.push(p),
                None => teams.push((t, vec![p])),
            }
        }
        teams
    }

    /// Each team in turn takes the first of the free start locations, shuffled for it,
    /// its members in order; one with too few left ends the placing.
    fn place_randomly(&mut self, players: &[u8]) {
        let mut taken: Vec<i32> = self.forced_locations();
        for (_, members) in self.teams_of(players) {
            let free = self.shuffled((0..self.start_locations.len() as i32).filter(|l| !taken.contains(l)).collect());
            if free.len() < members.len() {
                break;
            }
            for (&p, &l) in members.iter().zip(&free) {
                self.players[p as usize].start_location = l;
                taken.push(l);
            }
        }
    }

    /// A start location's priorities as the map set them (SetStartLocPrioCount and
    /// SetStartLocPrio): each slot's location and priority, a slot never set location 0
    /// at the low priority.
    fn prio_entries(&self, from: i32) -> Vec<(i32, i32)> {
        let Some(s) = self.start_locations.get(from as usize) else { return Vec::new() };
        (0..s.prio_count.max(0)).map(|k| s.prios.get(&k).copied().unwrap_or((0, PRIO_LOW))).collect()
    }

    /// Each team in turn, the one with the most players not yet placed first (the first
    /// such), around one of the free start locations shuffled for it (`FUN_6f1ea2f0`):
    /// the first around which `team_around` finds a location for each of them, else, the
    /// original's fallback, the first around which it finds any, those found taken and
    /// the rest left for a later turn of the loop; no location left ends it as failed.
    fn place_teams_together(&mut self, players: &[u8]) -> bool {
        let teams = self.teams_of(players);
        let mut taken: Vec<i32> = self.forced_locations();
        let mut placed: Vec<(u8, i32)> = Vec::new();
        loop {
            let left: Vec<Vec<u8>> = teams.iter().map(|t| t.1.iter().copied().filter(|p| !placed.iter().any(|x| x.0 == *p)).collect()).collect();
            let Some(members) = left.iter().fold(None::<&Vec<u8>>, |best, m| if m.len() > best.map_or(0, |b| b.len()) { Some(m) } else { best }) else { break };
            let anchors = self.shuffled((0..self.start_locations.len() as i32).filter(|l| !taken.contains(l)).collect());
            let mut found = None;
            for strict in [true, false] {
                for &anchor in &anchors {
                    found = self.team_around(members, anchor, strict, &taken);
                    if found.is_some() {
                        break;
                    }
                }
                if found.is_some() {
                    break;
                }
            }
            let Some(found) = found else { return false };
            for (p, l) in found {
                placed.push((p, l));
                taken.push(l);
            }
        }
        for (p, l) in placed {
            self.players[p as usize].start_location = l;
        }
        true
    }

    /// A team's players placed around `anchor` (`FUN_6f1e5140`): from it, the free
    /// locations each location found ranks (SetStartLocPrio's high, and any other as
    /// low), breadth first, each round's high ones then its low ones, each shuffled,
    /// until there is one for each player; strictly none unless there is, else as many
    /// as there are.  The players, shuffled, take them in order.
    fn team_around(&mut self, members: &[u8], anchor: i32, strict: bool, taken: &[i32]) -> Option<Vec<(u8, i32)>> {
        let n = self.start_locations.len();
        let free = |l: i32| (l as usize) < n && !taken.contains(&l);
        let (mut queue, mut chosen, mut high, mut low) = (vec![anchor], Vec::new(), Vec::new(), Vec::new());
        while chosen.len() < members.len() {
            if queue.is_empty() {
                let h = self.shuffled(std::mem::take(&mut high));
                let l = self.shuffled(std::mem::take(&mut low));
                if h.is_empty() && l.is_empty() {
                    break;
                }
                queue.extend(h.into_iter().chain(l).filter(|x| !chosen.contains(x)));
            } else {
                let x = queue.remove(0);
                chosen.push(x);
                for (loc, prio) in self.prio_entries(x) {
                    if chosen.contains(&loc) || queue.contains(&loc) || !free(loc) {
                        continue;
                    }
                    let list = if prio == PRIO_HIGH { &mut high } else { &mut low };
                    if !list.contains(&loc) {
                        list.push(loc);
                    }
                }
            }
        }
        if strict && chosen.len() < members.len() {
            return None;
        }
        let members = self.shuffled(members.to_vec());
        Some(members.into_iter().zip(chosen).collect())
    }
}
