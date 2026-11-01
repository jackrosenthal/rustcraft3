//! The handle table: every object a script can reference.

use crate::collections::HashMap;

/// Handle ids start here, as GetHandleId shows in the original.
pub const HANDLE_BASE: u32 = 0x10_0000;

/// Ids of objects no script has been given start here, apart from the handles: the
/// original gives such an object no handle (read: a save's hidden gold mine under a
/// haunted mine has none, nor the art the engine shows, as the haunted mine's circles
/// and the creeps' auras, and the handles after them follow on without a gap).
pub const UNSEEN_BASE: u32 = 0x7000_0000;

#[derive(serde::Serialize, serde::Deserialize)]
pub struct Handles<T> {
    objects: HashMap<u32, T>,
    /// Freed ids, the last freed taken first (measured: locations removed in the
    /// order a, c, b give their ids back as b, c, a).
    free: Vec<u32>,
    /// Ids of removed objects that scripts may still hold, in the order removed; each
    /// is freed once a sweep finds no script value referring to it, as the original
    /// frees a handle only when its references are gone.
    #[serde(default)]
    pending: Vec<u32>,
    next: u32,
    #[serde(default = "unseen_base")]
    next_unseen: u32,
    /// The handles given to unseen objects as scripts first had them: each handle's
    /// object's id, and each such object's handle.
    #[serde(default)]
    aliases: HashMap<u32, u32>,
    #[serde(default)]
    handle_of: HashMap<u32, u32>,
    /// When each handle was last given, in order (the original keeps its handles'
    /// rows so, a reused handle's after those given before it: read, Forestwalk's test
    /// map's save at 2.5 seconds, the melee timer's first force in the crypt's event
    /// data's freed handle 0x100f61 listed after the crypt's 0x100f62).
    #[serde(default)]
    given: HashMap<u32, u64>,
    #[serde(default)]
    given_count: u64,
}

fn unseen_base() -> u32 {
    UNSEEN_BASE
}

impl<T> Default for Handles<T> {
    fn default() -> Self {
        Handles { objects: HashMap::default(), free: Vec::new(), pending: Vec::new(), next: HANDLE_BASE, next_unseen: UNSEEN_BASE, aliases: HashMap::default(), handle_of: HashMap::default(), given: HashMap::default(), given_count: 0 }
    }
}

impl<T> Handles<T> {
    pub fn insert(&mut self, v: T) -> u32 {
        let id = self.take_id();
        self.objects.insert(id, v);
        id
    }

    /// The next handle: the last freed, or a new one.
    fn take_id(&mut self) -> u32 {
        let id = match self.free.pop() {
            Some(id) => id,
            None => {
                let id = self.next;
                self.next += 1;
                id
            }
        };
        self.mark_given(id);
        id
    }

    fn mark_given(&mut self, h: u32) {
        self.given.insert(h, self.given_count);
        self.given_count += 1;
    }

    /// Where a handle stands among those given, the earlier first.
    pub fn given_order(&self, h: u32) -> (u64, u32) {
        (self.given.get(&h).copied().unwrap_or(0), h)
    }

    /// The handle scripts know an object by: its id, or for an unseen object the
    /// handle it is given as a script first has it.
    pub fn handle(&mut self, id: u32) -> u32 {
        if id < UNSEEN_BASE {
            return id;
        }
        if let Some(&h) = self.handle_of.get(&id) {
            return h;
        }
        let h = self.take_id();
        self.aliases.insert(h, id);
        self.handle_of.insert(id, h);
        h
    }

    /// Gives an unseen object a handle chosen for it (as a loaded game's), taking the
    /// handle out of those free or awaiting a sweep.
    pub fn alias(&mut self, h: u32, id: u32) {
        self.free.retain(|&f| f != h);
        self.pending.retain(|&f| f != h);
        if h >= self.next {
            // The ids between stay unused, as the original's table has them free.
            self.free.extend(self.next..h);
            self.next = h + 1;
        }
        self.aliases.insert(h, id);
        self.handle_of.insert(id, h);
        self.mark_given(h);
    }

    /// The object a script's handle names (an unseen object's under its own id).
    pub fn resolve(&self, h: u32) -> u32 {
        self.aliases.get(&h).copied().unwrap_or(h)
    }

    /// Whether a script's handle stands for an unseen object.
    pub fn is_alias(&self, h: u32) -> bool {
        self.aliases.contains_key(&h)
    }

    /// The handle an object was given, if scripts have had it.
    pub fn known_handle(&self, id: u32) -> Option<u32> {
        if id < UNSEEN_BASE { Some(id) } else { self.handle_of.get(&id).copied() }
    }

    /// An object no script has been given, with an id apart from the handles'.
    pub fn insert_unseen(&mut self, v: T) -> u32 {
        let id = self.next_unseen;
        self.next_unseen += 1;
        self.objects.insert(id, v);
        id
    }

    /// Skips ids, as handles the original makes before ours would take them.
    pub fn reserve(&mut self, n: u32) {
        self.next += n;
    }

    pub fn remove(&mut self, id: u32) -> Option<T> {
        let v = self.objects.remove(&id);
        // An unseen object's handle, if it was given one, is what scripts hold.
        let id = if id < UNSEEN_BASE {
            id
        } else {
            match self.handle_of.remove(&id) {
                Some(h) => {
                    self.aliases.remove(&h);
                    h
                }
                None => return v,
            }
        };
        if v.is_some() && !self.pending.contains(&id) {
            self.pending.push(id);
        }
        v
    }

    /// Removes an object no script can hold, its handle free at once (the original's
    /// count of its references down to none, `FUN_6f283f40`, `FUN_6f28a300`).
    pub fn release(&mut self, id: u32) -> Option<T> {
        let v = self.objects.remove(&id);
        if v.is_some() {
            self.free.push(id);
        }
        v
    }

    /// Whether any removed id awaits a sweep.
    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    /// Frees the removed ids no script value refers to.
    pub fn sweep(&mut self, referenced: &crate::collections::HashSet<u32>) {
        let (gone, kept): (Vec<u32>, Vec<u32>) = self.pending.iter().copied().partition(|id| !referenced.contains(id));
        self.pending = kept;
        self.free.extend(gone);
    }

    pub fn get(&self, id: u32) -> Option<&T> {
        self.objects.get(&id)
    }

    pub fn get_mut(&mut self, id: u32) -> Option<&mut T> {
        self.objects.get_mut(&id)
    }

    pub fn contains(&self, id: u32) -> bool {
        self.objects.contains_key(&id)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&u32, &T)> {
        self.objects.iter()
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (&u32, &mut T)> {
        self.objects.iter_mut()
    }

    /// Ids of removed objects that scripts may still hold.
    pub fn pending(&self) -> &[u32] {
        &self.pending
    }

    pub fn ids(&self) -> Vec<u32> {
        let mut v: Vec<u32> = self.objects.keys().copied().collect();
        v.sort_unstable();
        v
    }
}
