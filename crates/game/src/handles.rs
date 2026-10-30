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
}

fn unseen_base() -> u32 {
    UNSEEN_BASE
}

impl<T> Default for Handles<T> {
    fn default() -> Self {
        Handles { objects: HashMap::default(), free: Vec::new(), pending: Vec::new(), next: HANDLE_BASE, next_unseen: UNSEEN_BASE }
    }
}

impl<T> Handles<T> {
    pub fn insert(&mut self, v: T) -> u32 {
        let id = match self.free.pop() {
            Some(id) => id,
            None => {
                let id = self.next;
                self.next += 1;
                id
            }
        };
        self.objects.insert(id, v);
        id
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
        if v.is_some() && id < UNSEEN_BASE && !self.pending.contains(&id) {
            self.pending.push(id);
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
