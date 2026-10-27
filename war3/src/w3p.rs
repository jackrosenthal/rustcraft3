//! The original's profile progress file, `Save\ProfileN\Campaigns.w3p`: an INI with a
//! section per data set, `[Campaign]` for Reign of Chaos and `[Campaign1]` for The
//! Frozen Throne, each holding a value per campaign (named as its CampaignList entry), the
//! player's name, the menu's race (ShowScreen), TutorialCleared, and Difficulty; and a
//! section `[NOTE TO HACKERS ...]` with `Magic` and `Magic1`, the CRC-32 of each
//! section's campaign values as little-endian 64-bit words (FUN_6f228be0,
//! FUN_6f96ab10).  A campaign's value is a 64-bit mask (FUN_6f247950): mission n at bit
//! n, bit 63 the campaign available, 62 its opening cinematic, 61 its ending, 59 and 60
//! the menu's own; signed, in decimal.  Verified against the reference install's files.

use w3_game::persist::{CampaignState, Persist};

const NOTE: &str = "NOTE TO HACKERS -- Changing this file may be hazardous to your progress";

/// The file's sections in order, each with its values in order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct W3p {
    pub sections: Vec<(String, Vec<(String, String)>)>,
}

/// A data set's section and its Magic value's name.
fn names(version: u32) -> (&'static str, &'static str) {
    if version == 0 { ("Campaign", "Magic") } else { ("Campaign1", "Magic1") }
}

/// A campaign's progress as its value.
pub fn value(c: &CampaignState) -> i64 {
    let mut v = c.other_bits.unwrap_or(1 << 59) & (3 << 59);
    for (i, open) in c.missions.iter().enumerate().take(59) {
        if *open {
            v |= 1 << i;
        }
    }
    v |= (c.available as u64) << 63 | (c.opening_cinematic as u64) << 62 | (c.ending_cinematic as u64) << 61;
    v as i64
}

/// A campaign's progress from its value.
pub fn state(v: i64) -> CampaignState {
    let v = v as u64;
    let last = (0..59).rev().find(|i| v >> i & 1 != 0).map_or(0, |i| i + 1);
    CampaignState { other_bits: Some(v & (3 << 59)), available: v >> 63 & 1 != 0, missions: (0..last).map(|i| v >> i & 1 != 0).collect(), opening_cinematic: v >> 62 & 1 != 0, ending_cinematic: v >> 61 & 1 != 0 }
}

impl W3p {
    pub fn parse(text: &str) -> W3p {
        let mut w = W3p::default();
        for line in text.lines().map(str::trim) {
            if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                w.sections.push((name.to_string(), Vec::new()));
            } else if let (Some((k, v)), Some(sec)) = (line.split_once('='), w.sections.last_mut()) {
                sec.1.push((k.to_string(), v.to_string()));
            }
        }
        w
    }

    pub fn to_text(&self) -> String {
        let mut out = String::new();
        for (name, values) in &self.sections {
            out += &format!("[{name}]\r\n");
            for (k, v) in values {
                out += &format!("{k}={v}\r\n");
            }
            out += "\r\n";
        }
        out
    }

    fn section_mut(&mut self, name: &str) -> &mut Vec<(String, String)> {
        if let Some(i) = self.sections.iter().position(|s| s.0 == name) {
            return &mut self.sections[i].1;
        }
        self.sections.push((name.to_string(), Vec::new()));
        &mut self.sections.last_mut().unwrap().1
    }

    fn get(&self, section: &str, key: &str) -> Option<&str> {
        self.sections.iter().find(|s| s.0 == section)?.1.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
    }

    fn set(&mut self, section: &str, key: &str, value: String) {
        let sec = self.section_mut(section);
        match sec.iter_mut().find(|(k, _)| k == key) {
            Some(e) => e.1 = value,
            None => sec.push((key.to_string(), value)),
        }
    }

    /// Puts a data set's progress in its section, with its campaigns (their places in
    /// CampaignList, which may have empty entries, and names) in order, and its Magic.
    pub fn put(&mut self, version: u32, campaigns: &[(usize, String)], player: &str, p: &Persist) {
        let (section, magic) = names(version);
        let mut bytes = Vec::new();
        let mut sec = Vec::new();
        for (i, name) in campaigns {
            let v = p.campaigns.get(*i).map_or_else(|| value(&CampaignState::default()), value);
            bytes.extend(v.to_le_bytes());
            sec.push((name.clone(), v.to_string()));
        }
        sec.push(("PlayerName".into(), player.to_string()));
        sec.push(("ShowScreen".into(), p.menu_race.to_string()));
        sec.push(("TutorialCleared".into(), (p.tutorial_cleared as i32).to_string()));
        sec.push(("Difficulty".into(), p.difficulty.to_string()));
        *self.section_mut(section) = sec;
        let mut h = crc32fast::Hasher::new();
        h.update(&bytes);
        self.set(NOTE, magic, (h.finalize() as i32).to_string());
        // The note's section stays after the first campaign section, as the original
        // writes it.
        if let Some(i) = self.sections.iter().position(|s| s.0 == NOTE) {
            let note = self.sections.remove(i);
            let at = self.sections.iter().position(|s| s.0.starts_with("Campaign")).map_or(self.sections.len(), |k| k + 1);
            self.sections.insert(at, note);
        }
    }

    /// Reads a data set's progress into `p`, its campaigns named in order; none if the
    /// section is missing or its Magic does not match.
    pub fn take(&self, version: u32, campaigns: &[(usize, String)], p: &mut Persist) -> bool {
        let (section, magic) = names(version);
        let vals: Vec<i64> = campaigns.iter().map(|(_, n)| self.get(section, n).and_then(|v| v.parse().ok()).unwrap_or(0)).collect();
        let bytes: Vec<u8> = vals.iter().flat_map(|v| v.to_le_bytes()).collect();
        let mut h = crc32fast::Hasher::new();
        h.update(&bytes);
        if self.get(NOTE, magic).and_then(|m| m.parse::<i32>().ok()) != Some(h.finalize() as i32) {
            return false;
        }
        if !self.sections.iter().any(|s| s.0 == section) {
            return false;
        }
        p.campaigns.clear();
        for ((i, _), v) in campaigns.iter().zip(vals) {
            *p.campaign(*i) = state(v);
        }
        let num = |k: &str| self.get(section, k).and_then(|v| v.parse::<i32>().ok());
        p.menu_race = num("ShowScreen").unwrap_or(0);
        p.tutorial_cleared = num("TutorialCleared").unwrap_or(0) != 0;
        p.difficulty = num("Difficulty").unwrap_or(1);
        true
    }

    /// Whether a data set's section holds progress (more than a profile's name).
    pub fn has_progress(&self, version: u32) -> bool {
        let (section, _) = names(version);
        self.sections.iter().any(|s| s.0 == section && s.1.iter().any(|(k, _)| k != "PlayerName"))
    }

    pub fn player_name(&self) -> Option<&str> {
        self.get("Campaign1", "PlayerName").or_else(|| self.get("Campaign", "PlayerName"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ORIGINAL: &str = "[Campaign1]\r\nNightElf=-3458764513820540927\r\nHuman=576460752303423488\r\nUndead=576460752303423488\r\nOrc=-4035225266123964415\r\nPlayerName=Test\r\nShowScreen=0\r\nTutorialCleared=0\r\nDifficulty=1\r\n\r\n[NOTE TO HACKERS -- Changing this file may be hazardous to your progress]\r\nMagic1=-1575126343\r\nMagic=464222035\r\n\r\n[Campaign]\r\nTutorial=-4035225266123964415\r\nHuman=-4035225266123964415\r\nUndead=576460752303423488\r\nOrc=576460752303423488\r\nNightElf=576460752303423488\r\nPlayerName=Test\r\nShowScreen=0\r\nTutorialCleared=0\r\nDifficulty=1\r\n\r\n";

    /// A file the original wrote reads, and writes back byte for byte.
    #[test]
    fn profiles_read_and_write_as_the_original() {
        let w = W3p::parse(ORIGINAL);
        let tft: Vec<(usize, String)> = [(4, "NightElf"), (5, "Human"), (6, "Undead"), (8, "Orc")].map(|(i, n)| (i, n.to_string())).to_vec();
        let roc: Vec<(usize, String)> = ["Tutorial", "Human", "Undead", "Orc", "NightElf"].iter().enumerate().map(|(i, n)| (i, n.to_string())).collect();
        let (mut p1, mut p0) = (Persist::default(), Persist::default());
        assert!(w.take(1, &tft, &mut p1));
        assert!(w.take(0, &roc, &mut p0));
        assert!(p1.campaigns[4].available && p1.campaigns[4].missions == vec![true]);
        let mut out = W3p::parse(ORIGINAL);
        out.put(1, &tft, "Test", &p1);
        out.put(0, &roc, "Test", &p0);
        assert_eq!(out.to_text(), ORIGINAL);
    }
}
