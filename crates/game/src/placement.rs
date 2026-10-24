//! Start locations as the game begins (FUN_6f1e9fd0), by the map's placement
//! (SetGamePlacement): MAP_PLACEMENT_RANDOM (0) gives each player a free start
//! location in random order; FIXED (1) and USE_MAP_SETTINGS (2) keep those the map set;
//! TEAMS_TOGETHER (3), with the setup's Teams Together (0x40), places each team, the
//! largest first, around a random free location, its other members on free locations
//! the placed ones rank near (SetStartLocPrio: high, then low; FUN_6f1ea2f0), and
//! otherwise falls back to random placement.

use crate::types::{map_control, slot_state};
use crate::world::World;

const PRIO_LOW: i32 = 0;
const PRIO_HIGH: i32 = 1;

impl World {
    /// The players who take a start location: users and computers in the game.
    fn placed_players(&self) -> Vec<u8> {
        (0..crate::types::PLAYER_NEUTRAL_AGGRESSIVE)
            .filter(|&p| {
                let pl = &self.players[p as usize];
                pl.slot_state == slot_state::PLAYING && matches!(pl.controller, map_control::USER | map_control::COMPUTER) && pl.get_state(crate::types::player_state::OBSERVER) == 0
            })
            .collect()
    }

    fn shuffled(&mut self, mut v: Vec<i32>) -> Vec<i32> {
        for i in (1..v.len()).rev() {
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

    fn place_randomly(&mut self, players: &[u8]) {
        let free = self.shuffled((0..self.start_locations.len() as i32).collect());
        for (&p, loc) in players.iter().zip(free) {
            self.players[p as usize].start_location = loc;
        }
    }

    /// The priority one start location gives another: high, low, or none.
    fn prio(&self, from: i32, to: i32) -> Option<i32> {
        self.start_locations.get(from as usize)?.prios.values().find(|&&(loc, _)| loc == to).map(|&(_, p)| p).filter(|&p| p == PRIO_HIGH || p == PRIO_LOW)
    }

    fn place_teams_together(&mut self, players: &[u8]) -> bool {
        let mut teams: Vec<(i32, Vec<u8>)> = Vec::new();
        for &p in players {
            let t = self.players[p as usize].team;
            match teams.iter_mut().find(|g| g.0 == t) {
                Some(g) => g.1.push(p),
                None => teams.push((t, vec![p])),
            }
        }
        // The largest teams first.
        teams.sort_by(|a, b| b.1.len().cmp(&a.1.len()));
        let mut taken: Vec<i32> = Vec::new();
        let mut placed: Vec<(u8, i32)> = Vec::new();
        for (_, members) in teams {
            let anchors = self.shuffled((0..self.start_locations.len() as i32).filter(|l| !taken.contains(l)).collect());
            let mut done = None;
            for anchor in anchors {
                let mut mine = vec![anchor];
                let mut ok = true;
                for _ in 1..members.len() {
                    // The free location the placed members rank nearest: high, then low.
                    let free: Vec<i32> = (0..self.start_locations.len() as i32).filter(|l| !taken.contains(l) && !mine.contains(l)).collect();
                    let best = [PRIO_HIGH, PRIO_LOW].iter().find_map(|&want| free.iter().copied().find(|&l| mine.iter().any(|&m| self.prio(m, l) == Some(want))));
                    match best {
                        Some(l) => mine.push(l),
                        None => {
                            ok = false;
                            break;
                        }
                    }
                }
                if ok {
                    done = Some(mine);
                    break;
                }
            }
            let Some(mine) = done else { return false };
            for (&p, &l) in members.iter().zip(&mine) {
                placed.push((p, l));
                taken.push(l);
            }
        }
        for (p, l) in placed {
            self.players[p as usize].start_location = l;
        }
        true
    }
}
