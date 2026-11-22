//! Communism mode (rustcraft3's own, a lobby's flag `COMMUNISM`): the human players'
//! gold is one pool, which each of them holds and any of them spends.  Each player's
//! gold stays its own state, so everything that pays or charges a player (orders,
//! bounties, scripts) works as ever; the pool takes in each player's change since it
//! last stood and gives every player the sum (`World::pool_gold`), after every command
//! carried out (so a purchase sees what the others spent before it) and every step of
//! the game (so a script's grants to each player all count).  It runs in the
//! simulation, so every computer of a game, and its replay, pools alike.

use crate::types::{map_control, player_state, slot_state};
use crate::world::World;

/// The lobby's settings flag for the mode: above the original's (whose highest,
/// referees, is 0x400000) and within the 24 bits its settings word carries.
pub const COMMUNISM: u32 = 0x800000;

impl World {
    /// The pool takes in each member's change since it last stood, and every member
    /// holds the sum; the first time, the pool is what the members hold together.
    /// Members are the human players playing, those who leave holding what the pool
    /// was as they left.
    pub fn pool_gold(&mut self) {
        if !self.shared_gold {
            return;
        }
        let members: Vec<usize> = (0..self.players.len().min(crate::types::PLAYER_COUNT))
            .filter(|&p| self.players[p].controller == map_control::USER && self.players[p].slot_state == slot_state::PLAYING)
            .collect();
        let gold = |w: &World, p: usize| w.players[p].get_state(player_state::RESOURCE_GOLD);
        let pool = match self.gold_pool {
            None => members.iter().map(|&p| gold(self, p)).sum(),
            Some(was) => was + members.iter().map(|&p| gold(self, p) - was).sum::<i32>(),
        };
        let pool = pool.max(0);
        for &p in &members {
            self.players[p].state.insert(player_state::RESOURCE_GOLD, pool);
        }
        self.gold_pool = Some(pool);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn world(humans: &[usize]) -> World {
        let mut w = World::new(Arc::new(w3_mpq::Chain::new()), crate::objects::ObjectDb::default());
        for &p in humans {
            w.players[p].controller = map_control::USER;
            w.players[p].slot_state = slot_state::PLAYING;
        }
        w.shared_gold = true;
        w
    }

    fn set(w: &mut World, p: usize, g: i32) {
        w.players[p].state.insert(player_state::RESOURCE_GOLD, g);
    }

    fn gold(w: &World, p: usize) -> i32 {
        w.players[p].get_state(player_state::RESOURCE_GOLD)
    }

    #[test]
    fn the_humans_gold_is_one_pool_anyone_spends() {
        let mut w = world(&[0, 1, 2]);
        for (p, g) in [(0, 50), (1, 50), (2, 50), (11, 999)] {
            set(&mut w, p, g);
        }
        w.pool_gold();
        assert_eq!([gold(&w, 0), gold(&w, 1), gold(&w, 2)], [150, 150, 150]);
        // A computer's (or the creeps') gold is its own.
        assert_eq!(gold(&w, 11), 999);
        // One spends 100; another is paid 30 by a bounty in the same step.
        set(&mut w, 0, 50);
        set(&mut w, 2, 180);
        w.pool_gold();
        assert_eq!([gold(&w, 0), gold(&w, 1), gold(&w, 2)], [80, 80, 80]);
        // A script's grant to each of them all counts.
        for p in 0..3 {
            set(&mut w, p, 80 + 60);
        }
        w.pool_gold();
        assert_eq!(gold(&w, 1), 260);
        // One who leaves keeps what was; the pool goes on among the rest.
        w.players[2].slot_state = slot_state::LEFT;
        set(&mut w, 0, 200);
        w.pool_gold();
        assert_eq!([gold(&w, 0), gold(&w, 1), gold(&w, 2)], [200, 200, 260]);
    }

    #[test]
    fn without_the_mode_gold_is_each_ones_own() {
        let mut w = world(&[0, 1]);
        w.shared_gold = false;
        set(&mut w, 0, 10);
        set(&mut w, 1, 20);
        w.pool_gold();
        assert_eq!([gold(&w, 0), gold(&w, 1)], [10, 20]);
    }
}
