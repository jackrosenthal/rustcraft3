//! Communism mode (rustcraft3's own, a lobby's flag `COMMUNISM`): the human players'
//! gold is one pool and their lumber another, which each of them holds and any of them
//! spends.  Each player's resources stay their own state, so everything in the engine
//! that pays or charges a player (orders, bounties) works as ever; each pool takes in
//! each player's change since it last stood and gives every player the sum
//! (`World::pool_resources`), after every command carried out (so a purchase sees what
//! the others spent before it) and every step of the game.
//!
//! A script sees its player's share: GetPlayerState of a pooled resource reads the pool
//! divided among the members (`script_read`), and SetPlayerState changes the pool by
//! what it sets less that share (`script_write`).  An adjustment (Blizzard.j's reads,
//! then sets the read plus the change) so changes the pool by the change, and a setting
//! (Blizzard.j's SetPlayerStateBJ adjusts by the setting less the read) for each player
//! makes the pool the setting times the players: the map's 1000 lumber for each, a pool
//! of 1000 for each.  It runs in the simulation, so every computer of a game, and its
//! replay, pools alike.

use crate::types::{map_control, player_state, slot_state};
use crate::world::World;

/// The lobby's settings flag for the mode: above the original's (whose highest,
/// referees, is 0x400000) and within the 24 bits its settings word carries.
pub const COMMUNISM: u32 = 0x800000;

/// The resources pooled, in the order `World::resource_pools` keeps them.
pub const POOLED: [i32; 2] = [player_state::RESOURCE_GOLD, player_state::RESOURCE_LUMBER];

impl World {
    /// The players whose resources are pooled: the human players playing.
    fn pool_members(&self) -> Vec<u8> {
        (0..self.players.len().min(crate::types::PLAYER_COUNT) as u8).filter(|&p| self.players[p as usize].controller == map_control::USER && self.players[p as usize].slot_state == slot_state::PLAYING).collect()
    }

    /// A member's share of what it holds of a pooled resource (the pool, give or take
    /// its own change this step): an even part, the remainder a unit more to each of the
    /// first members, so the shares make up the whole.  None for anything not pooled.
    fn share(&self, player: u8, state: i32) -> Option<i32> {
        if !self.communism || !POOLED.contains(&state) {
            return None;
        }
        let members = self.pool_members();
        let rank = members.iter().position(|&p| p == player)? as i32;
        let (held, n) = (self.players[player as usize].get_state(state), members.len() as i32);
        Some(held / n + (rank < held % n) as i32)
    }

    /// What a script reads of a player's resource: its share of the pool.
    pub fn script_read(&self, player: u8, state: i32) -> i32 {
        self.share(player, state).unwrap_or_else(|| self.players[player as usize].get_state(state))
    }

    /// What a player holds once a script sets its resource to `value`: the pool changed
    /// by `value` less the player's share.
    pub fn script_write(&self, player: u8, state: i32, value: i32) -> i32 {
        let held = self.players[player as usize].get_state(state);
        self.share(player, state).map_or(value, |s| held + value - s)
    }

    /// Each pool takes in each member's change since it last stood, and every member
    /// holds the sum; the first time, a pool is what the members hold together.  Those
    /// who leave hold what the pool was as they left.
    pub fn pool_resources(&mut self) {
        if !self.communism {
            return;
        }
        let members = self.pool_members();
        let mut pools = [0; POOLED.len()];
        for (k, &state) in POOLED.iter().enumerate() {
            let held = |w: &World, p: u8| w.players[p as usize].get_state(state);
            let pool = match self.resource_pools {
                None => members.iter().map(|&p| held(self, p)).sum(),
                Some(was) => was[k] + members.iter().map(|&p| held(self, p) - was[k]).sum::<i32>(),
            };
            pools[k] = pool.clamp(0, 1_000_000);
            for &p in &members {
                self.players[p as usize].state.insert(state, pools[k]);
            }
        }
        self.resource_pools = Some(pools);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    const GOLD: i32 = player_state::RESOURCE_GOLD;
    const LUMBER: i32 = player_state::RESOURCE_LUMBER;

    fn world(humans: &[usize]) -> World {
        let mut w = World::new(Arc::new(w3_mpq::Chain::new()), crate::objects::ObjectDb::default());
        for &p in humans {
            w.players[p].controller = map_control::USER;
            w.players[p].slot_state = slot_state::PLAYING;
        }
        w.communism = true;
        w
    }

    fn put(w: &mut World, p: usize, s: i32, v: i32) {
        w.players[p].state.insert(s, v);
    }

    fn get(w: &World, p: usize, s: i32) -> i32 {
        w.players[p].get_state(s)
    }

    /// As SetPlayerState and GetPlayerState do it for a script.
    fn script_set(w: &mut World, p: usize, s: i32, v: i32) {
        let held = w.script_write(p as u8, s, v);
        put(w, p, s, held);
    }

    /// Blizzard.j's AdjustPlayerStateSimpleBJ, and SetPlayerStateBJ (an adjustment by
    /// the value less the read).
    fn adjust_bj(w: &mut World, p: usize, s: i32, delta: i32) {
        let v = w.script_read(p as u8, s) + delta;
        script_set(w, p, s, v);
    }

    fn set_bj(w: &mut World, p: usize, s: i32, value: i32) {
        let old = w.script_read(p as u8, s);
        adjust_bj(w, p, s, value - old);
    }

    #[test]
    fn the_humans_gold_and_lumber_are_pools_anyone_spends() {
        let mut w = world(&[0, 1, 2]);
        for (p, g) in [(0, 50), (1, 50), (2, 50), (11, 999)] {
            put(&mut w, p, GOLD, g);
            put(&mut w, p, LUMBER, g * 2);
        }
        w.pool_resources();
        assert_eq!([get(&w, 0, GOLD), get(&w, 1, GOLD), get(&w, 2, GOLD)], [150, 150, 150]);
        assert_eq!(get(&w, 1, LUMBER), 300);
        // A computer's (or the creeps') resources are its own, to scripts too.
        assert_eq!((get(&w, 11, GOLD), w.script_read(11, GOLD)), (999, 999));
        // One spends 100 gold and 40 lumber; another is paid 30 gold by a bounty.
        put(&mut w, 0, GOLD, 50);
        put(&mut w, 0, LUMBER, 260);
        put(&mut w, 2, GOLD, 180);
        w.pool_resources();
        assert_eq!([get(&w, 0, GOLD), get(&w, 1, GOLD), get(&w, 2, GOLD)], [80, 80, 80]);
        assert_eq!(get(&w, 2, LUMBER), 260);
        // A script sees a third, the shares making up the whole; its adjustment of each
        // of them all counts.
        assert_eq!([w.script_read(0, GOLD), w.script_read(1, GOLD), w.script_read(2, GOLD)], [27, 27, 26]);
        for p in 0..3 {
            adjust_bj(&mut w, p, GOLD, 60);
        }
        w.pool_resources();
        assert_eq!(get(&w, 1, GOLD), 260);
        // One who leaves keeps what was; the pool goes on among the rest.
        w.players[2].slot_state = slot_state::LEFT;
        put(&mut w, 0, GOLD, 200);
        w.pool_resources();
        assert_eq!([get(&w, 0, GOLD), get(&w, 1, GOLD), get(&w, 2, GOLD)], [200, 200, 260]);
    }

    #[test]
    fn a_resource_set_for_each_player_is_the_pool_times_the_players() {
        let mut w = world(&[0, 1, 2]);
        put(&mut w, 0, LUMBER, 70);
        w.pool_resources();
        // The map's level end: SetPlayerStateBJ of each player's lumber to 1000 (the
        // computers' too), and the native set outright alike.
        for p in [0, 1, 2, 11] {
            set_bj(&mut w, p, LUMBER, 1000);
        }
        w.pool_resources();
        assert_eq!([get(&w, 0, LUMBER), get(&w, 1, LUMBER), get(&w, 2, LUMBER), get(&w, 11, LUMBER)], [3000, 3000, 3000, 1000]);
        for p in 0..3 {
            script_set(&mut w, p, GOLD, 500);
        }
        w.pool_resources();
        assert_eq!(get(&w, 2, GOLD), 1500);
    }

    #[test]
    fn without_the_mode_resources_are_each_ones_own() {
        let mut w = world(&[0, 1]);
        w.communism = false;
        put(&mut w, 0, GOLD, 10);
        put(&mut w, 1, GOLD, 20);
        w.pool_resources();
        assert_eq!([get(&w, 0, GOLD), get(&w, 1, GOLD)], [10, 20]);
        assert_eq!((w.script_read(0, GOLD), w.script_write(0, GOLD, 5)), (10, 5));
    }
}
