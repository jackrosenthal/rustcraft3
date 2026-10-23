//! A computer player's guard posts (CGuardPost): AddGuardPost names a unit type to
//! keep at a point.  A new unit of that type that is not a Hero takes the first empty
//! post of its type whose replacements (SetReplacementCount) are not used up, leaves
//! its captain, and attack-moves there (FUN_6f9c9d60, FUN_6f9bfc90).  FillGuardPosts
//! asks for a unit for every such empty post (FUN_6f9c5ef0); ReturnGuardPosts sends
//! idle guards more than 50 from their posts back (FUN_6f9ca090, FUN_6f9c0b00).

use crate::objects::Id;
use crate::types::*;
use crate::world::World;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct GuardPost {
    pub id: Id,
    pub x: f32,
    pub y: f32,
    /// The unit keeping the post.
    pub unit: u32,
    /// Units assigned to the post so far.
    pub filled: i32,
}

impl World {
    fn post_empty(&self, post: &GuardPost) -> bool {
        self.unit(post.unit).is_none_or(|u| u.dead)
    }

    /// AddGuardPost (FUN_6f9c2ee0): a post, unless an empty one of that type is
    /// already at that point.
    pub(crate) fn add_guard_post(&mut self, p: u8, id: Id, x: f32, y: f32) {
        let exists = self.ai.get(&p).is_some_and(|s| s.posts.iter().any(|g| g.id == id && g.x == x && g.y == y && self.post_empty(g)));
        if !exists {
            if let Some(s) = self.ai.get_mut(&p) {
                s.posts.push(GuardPost { id, x, y, unit: 0, filled: 0 });
            }
        }
    }

    /// A computer player's new unit takes an empty guard post of its type.
    pub(crate) fn ai_new_unit(&mut self, h: u32) {
        let Some(u) = self.unit(h) else { return };
        let (p, type_id) = (u.owner, u.type_id);
        if u.is_building || u.is_hero && u.extra.illusion.is_none() || self.objects.units.get(type_id).is_some_and(|o| o.list("type").iter().any(|t| t.trim().eq_ignore_ascii_case("peon"))) {
            return;
        }
        let Some(st) = self.ai.get(&p) else { return };
        let limit = st.replacements;
        let Some(i) = st.posts.iter().position(|g| g.id == type_id && self.post_empty(g) && g.filled <= limit) else { return };
        let Some(st) = self.ai.get_mut(&p) else { return };
        let post = &mut st.posts[i];
        post.unit = h;
        post.filled += 1;
        let (x, y) = (post.x, post.y);
        st.attack.units.retain(|&k| k != h);
        st.defense.units.retain(|&k| k != h);
        let attack = crate::natives::basic::order_id("attack");
        self.order_unit(h, attack, x, y, 0);
    }

    /// FillGuardPosts: a unit asked for each empty post whose replacements remain.
    pub(crate) fn fill_guard_posts(&mut self, p: u8) {
        let Some(st) = self.ai.get(&p) else { return };
        let limit = st.replacements;
        let wanted: Vec<Id> = st.posts.iter().filter(|g| self.post_empty(g) && g.filled <= limit).map(|g| g.id).collect();
        for id in wanted {
            self.ai_produce(p, id, -1);
        }
    }

    /// ReturnGuardPosts: idle guards more than 50 from their posts attack-move back.
    pub(crate) fn return_guard_posts(&mut self, p: u8) {
        let Some(st) = self.ai.get(&p) else { return };
        let away: Vec<(u32, f32, f32)> = st
            .posts
            .iter()
            .filter_map(|g| self.unit(g.unit).filter(|u| !u.dead && matches!(u.action, Action::Idle) && (u.x - g.x).hypot(u.y - g.y) > 50.0).map(|_| (g.unit, g.x, g.y)))
            .collect();
        let attack = crate::natives::basic::order_id("attack");
        for (h, x, y) in away {
            self.order_unit(h, attack, x, y, 0);
        }
    }

    /// Units keeping a guard post, which the captains leave alone.
    pub(crate) fn guards(&self, p: u8) -> Vec<u32> {
        self.ai.get(&p).map(|s| s.posts.iter().filter(|g| !self.post_empty(g)).map(|g| g.unit).collect()).unwrap_or_default()
    }
}

impl World {
    /// RemoveGuardPosition (FUN_6f9be910, FUN_6f9c9b30): a computer player's unit
    /// leaves its captain and stops, and its guard post is never filled again.
    pub fn remove_guard_position(&mut self, h: u32) {
        let Some(p) = self.unit(h).map(|u| u.owner) else { return };
        let Some(st) = self.ai.get_mut(&p) else { return };
        st.attack.units.retain(|&k| k != h);
        st.defense.units.retain(|&k| k != h);
        for g in st.posts.iter_mut().filter(|g| g.unit == h) {
            g.unit = 0;
            g.filled = 99999;
        }
        self.order_unit(h, crate::natives::basic::order_id("stop"), 0.0, 0.0, 0);
    }

    /// RecycleGuardPosition (FUN_6f9bde20): as RemoveGuardPosition, then the unit is
    /// taken in afresh and may keep another post.
    pub fn recycle_guard_position(&mut self, h: u32) {
        self.remove_guard_position(h);
        self.ai_new_unit(h);
    }

    /// RemoveAllGuardPositions (FUN_6f9c9a60): the computer player's guards leave
    /// their captains and every post is gone.
    pub fn remove_all_guard_positions(&mut self, p: u8) {
        let Some(st) = self.ai.get_mut(&p) else { return };
        let guards: Vec<u32> = st.posts.iter().map(|g| g.unit).filter(|&u| u != 0).collect();
        st.attack.units.retain(|k| !guards.contains(k));
        st.defense.units.retain(|k| !guards.contains(k));
        st.posts.clear();
    }
}
