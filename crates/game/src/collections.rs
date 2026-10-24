//! Hash maps and sets that iterate in the same order in every run (std's hasher with its
//! fixed keys, not a random seed), so that a game replays exactly.

use std::hash::{BuildHasherDefault, DefaultHasher};

pub type HashMap<K, V> = std::collections::HashMap<K, V, BuildHasherDefault<DefaultHasher>>;
pub type HashSet<K> = std::collections::HashSet<K, BuildHasherDefault<DefaultHasher>>;
