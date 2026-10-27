//! The original engine's tags: every object it makes (a game object, an ability, a
//! script handle, and the engine's own regions, positions, and properties) takes the
//! next tag from its object manager (Tempest's, `DAT_6fd6860c`).  A tag names an object
//! in the network's actions (as two words), and the tags given so far go into the
//! game's checksum, so this game counts the same tags at the same moments
//! (`docs/reference/bootybay_tags.txt` is the original's sequence for a map's start).

use serde::{Deserialize, Serialize};

/// A type code as the original keeps it at +0x10 of each tagged object (`+w3u` a unit,
/// `^pmr` a region, `Amov` an ability, ...).
pub type Code = [u8; 4];

/// While set, a new world records its tags' codes (the `tags` example sets it).
pub static RECORD: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Tags {
    /// Tags given so far (the next presence tag, while none has been given back).
    pub next: u32,
    /// The objects' codes in the order tagged, while recording (for comparing with the
    /// original's).
    #[serde(skip)]
    pub log: Option<Vec<Code>>,
}

impl Tags {
    pub fn new() -> Tags {
        Tags { next: 0, log: RECORD.load(std::sync::atomic::Ordering::Relaxed).then(Vec::new) }
    }

    /// The next tag, for an object of type `code`.
    pub fn take(&mut self, code: &Code) -> u32 {
        let t = self.next;
        self.next += 1;
        if let Some(l) = self.log.as_mut() {
            l.push(*code);
        }
        t
    }

    /// Tags for objects of the codes given, in turn.
    pub fn take_all(&mut self, codes: &[&Code]) {
        for c in codes {
            self.take(c);
        }
    }

    /// `n` objects of one code.
    pub fn take_n(&mut self, code: &Code, n: usize) {
        for _ in 0..n {
            self.take(code);
        }
    }
}
