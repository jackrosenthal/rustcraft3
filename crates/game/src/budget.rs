//! The engine's search budgets (NIpse's, read from game.dll): for each player (a path's
//! class, +0x88 bits 16 to 19, the owner as `CUnit::vf107` and `FUN_6f698ce0` give it to
//! `FUN_6f168a80`), four records of 0x1c bytes at `DAT_6fd53a90` + player * 0x70, made
//! by the static initializers at 0x6f003d20 (`FUN_6f1656d0`): a pop limit they serve,
//! a period, and a cap.  Each step (`FUN_6f167310` from `FUN_6f15aa80`) a record's
//! countdown runs down, and at 0 its use is cleared and the countdown set to its period
//! again (`FUN_6f167fa0`), so it is renewed every period + 1 steps.  A search asks its
//! record first (`FUN_6f168310`): over the cap, it is refused and queued; under it,
//! allowed while none waits, else only the first in the queue is, the rest queued
//! behind it (`FUN_6f165ea0`, `FUN_6f1686a0`).  Its pops are then added to the use.

use std::collections::VecDeque;

/// The records: a path's coarse plan over the accelerator by its pop limit (+0x86:
/// over 400, `Order`, 5000, as a move's cluster path, `FUN_6f169840`; `Order2000` for a
/// path flagged 0x4000000; 400 or under, `Coarse`, as a unit's own path's), and its
/// local search (`Local`, `DAT_6fd53ae4`, `FUN_6f166e90`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Order = 0,
    Order2000 = 1,
    Coarse = 2,
    Local = 3,
}

/// Each record's period and cap (0x6f003d20: 5000 3 800, 2000 2 300, 400 2 900, 700 1
/// 1100).
const TABLE: [(u64, u32); 4] = [(3, 800), (2, 300), (2, 900), (1, 1100)];

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
struct Record {
    window: i64,
    spent: u32,
    queue: VecDeque<u32>,
}

/// The budgets of each player.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Budgets {
    players: Vec<[Record; 4]>,
}

impl Budgets {
    /// A record at step `now` (the engine's step count), renewed every period + 1 steps
    /// from step 1 (read: the local budget renewed at odd steps, a Booty Bay sheep's
    /// search refused till then; provisional for the others: all taken as renewed
    /// together first, as the initializers leave their countdowns at 0).
    fn record(&mut self, player: u8, kind: Kind, now: u64) -> &mut Record {
        let p = player as usize;
        if self.players.len() <= p {
            self.players.resize_with(p + 1, Default::default);
        }
        let (period, _) = TABLE[kind as usize];
        let window = (now as i64 - 1).div_euclid(period as i64 + 1);
        let r = &mut self.players[p][kind as usize];
        if r.window != window {
            r.window = window;
            r.spent = 0;
        }
        r
    }

    /// Whether `unit`'s search may run now (`FUN_6f168310`), queuing it when not.  Those
    /// at the queue's head no longer `waiting` (gone, or walking no more) are dropped
    /// first, as the engine drops a path from it as its walk ends (`FUN_6f168800`).
    pub fn allow(&mut self, player: u8, kind: Kind, now: u64, unit: u32, waiting: impl Fn(u32) -> bool) -> bool {
        let cap = TABLE[kind as usize].1;
        let r = self.record(player, kind, now);
        while r.queue.front().is_some_and(|&u| u != unit && !waiting(u)) {
            r.queue.pop_front();
        }
        let queued = r.queue.contains(&unit);
        if cap < r.spent {
            if !queued {
                r.queue.push_back(unit);
            }
            return false;
        }
        if r.queue.is_empty() {
            return true;
        }
        if r.queue.front() == Some(&unit) {
            r.queue.pop_front();
            return true;
        }
        if !queued {
            r.queue.push_back(unit);
        }
        false
    }

    /// Adds a search's pops to its record; the use after.
    pub fn spend(&mut self, player: u8, kind: Kind, now: u64, pops: u32) -> u32 {
        let r = self.record(player, kind, now);
        r.spent += pops;
        r.spent
    }

    /// A unit's searches no longer wait (its walk over or begun anew: `FUN_6f168800`,
    /// `FUN_6f1686a0`).
    pub fn leave(&mut self, unit: u32) {
        for p in &mut self.players {
            for r in p.iter_mut() {
                r.queue.retain(|&u| u != unit);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Mafarazzo TD's 180 creeps, ordered at once, their local searches 21 to 35 pops:
    /// 41 search in the first step, the rest queued, served in their order as the budget
    /// is renewed every other step.
    #[test]
    fn queues_past_the_cap() {
        let mut b = Budgets::default();
        let mut served = Vec::new();
        for now in [3u64, 4, 5] {
            for u in 0..60 {
                if b.allow(11, Kind::Local, now, u, |_| true) {
                    b.spend(11, Kind::Local, now, 30);
                    served.push((now, u));
                }
            }
        }
        // 37 in the first window (the 37th takes it past 1100), none at its second step,
        // then the 23 queued, from the queue's head, those not yet queued behind them.
        assert_eq!(served.iter().filter(|s| s.0 == 3).count(), 37);
        assert_eq!(served.iter().filter(|s| s.0 == 4).count(), 0);
        assert_eq!(served.iter().filter(|s| s.0 == 5).map(|s| s.1).collect::<Vec<_>>(), (37..60).collect::<Vec<_>>());
    }
}
