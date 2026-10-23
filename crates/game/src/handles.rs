//! The handle table: every object a script can reference.

use std::collections::{BTreeSet, HashMap};

/// Handle ids start here, as GetHandleId shows in the original.
pub const HANDLE_BASE: u32 = 0x10_0000;

pub struct Handles<T> {
    objects: HashMap<u32, T>,
    free: BTreeSet<u32>,
    next: u32,
}

impl<T> Default for Handles<T> {
    fn default() -> Self {
        Handles { objects: HashMap::new(), free: BTreeSet::new(), next: HANDLE_BASE }
    }
}

impl<T> Handles<T> {
    pub fn insert(&mut self, v: T) -> u32 {
        let id = match self.free.pop_first() {
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

    pub fn remove(&mut self, id: u32) -> Option<T> {
        let v = self.objects.remove(&id);
        if v.is_some() {
            self.free.insert(id);
        }
        v
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

    pub fn ids(&self) -> Vec<u32> {
        let mut v: Vec<u32> = self.objects.keys().copied().collect();
        v.sort_unstable();
        v
    }
}
