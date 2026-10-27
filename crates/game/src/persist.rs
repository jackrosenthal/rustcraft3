//! What a player's profile keeps between games: campaign progress (which campaigns and
//! missions are open, and the cinematics seen), the campaign difficulty, and the game
//! caches maps save (SaveGameCache) to carry heroes and choices into later missions.

use crate::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::types::{GameCache, StoredUnit};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CampaignState {
    /// The original's other flags for it (bits 59 and 60 of its progress value, which
    /// its campaign menu keeps; bit 59 is set as a profile is made).
    #[serde(default)]
    pub other_bits: Option<u64>,
    pub available: bool,
    /// Open missions by index.
    pub missions: Vec<bool>,
    pub opening_cinematic: bool,
    pub ending_cinematic: bool,
}

/// One saved game cache, in a form JSON can hold.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SavedCache {
    /// Its name as InitGameCache gave it.
    #[serde(default)]
    pub name: String,
    pub ints: Vec<(String, String, i32)>,
    pub reals: Vec<(String, String, f32)>,
    pub bools: Vec<(String, String, bool)>,
    pub strings: Vec<(String, String, String)>,
    pub units: Vec<(String, String, StoredUnit)>,
}

impl SavedCache {
    pub fn from_cache(c: &GameCache) -> SavedCache {
        SavedCache {
            name: c.name.clone(),
            ints: c.ints.iter().map(|((a, b), v)| (a.clone(), b.clone(), *v)).collect(),
            reals: c.reals.iter().map(|((a, b), v)| (a.clone(), b.clone(), *v)).collect(),
            bools: c.bools.iter().map(|((a, b), v)| (a.clone(), b.clone(), *v)).collect(),
            strings: c.strings.iter().map(|((a, b), v)| (a.clone(), b.clone(), v.clone())).collect(),
            units: c.units.iter().map(|((a, b), v)| (a.clone(), b.clone(), v.clone())).collect(),
        }
    }

    pub fn to_cache(&self, name: &str) -> GameCache {
        let key = |a: &String, b: &String| (a.clone(), b.clone());
        GameCache {
            name: name.to_string(),
            ints: self.ints.iter().map(|(a, b, v)| (key(a, b), *v)).collect(),
            reals: self.reals.iter().map(|(a, b, v)| (key(a, b), *v)).collect(),
            bools: self.bools.iter().map(|(a, b, v)| (key(a, b), *v)).collect(),
            strings: self.strings.iter().map(|(a, b, v)| (key(a, b), v.clone())).collect(),
            units: self.units.iter().map(|(a, b, v)| (key(a, b), v.clone())).collect(),
        }
    }

    /// As the original's file holds it (`w3_formats::w3v`): its missions by name, each
    /// one's entries by key (the original's own order, its hash tables', is not kept),
    /// and its counters as the original's sample has them (the missions' count, and 0
    /// for each mission's tables).
    pub fn to_w3v(&self) -> w3_formats::w3v::Cache {
        let mut names: Vec<&String> = self.ints.iter().map(|e| &e.0).chain(self.reals.iter().map(|e| &e.0)).chain(self.bools.iter().map(|e| &e.0)).chain(self.units.iter().map(|e| &e.0)).chain(self.strings.iter().map(|e| &e.0)).collect();
        names.sort();
        names.dedup();
        let missions: Vec<w3_formats::w3v::Mission> = names
            .into_iter()
            .map(|m| {
                fn pick<T: Clone, U>(v: &[(String, String, T)], m: &str, f: impl Fn(&T) -> U) -> Vec<(String, U)> {
                    let mut out: Vec<(String, U)> = v.iter().filter(|e| e.0 == m).map(|e| (e.1.clone(), f(&e.2))).collect();
                    out.sort_by(|a, b| a.0.cmp(&b.0));
                    out
                }
                w3_formats::w3v::Mission {
                    name: m.clone(),
                    counters: [0; 5],
                    ints: pick(&self.ints, m, |v| *v),
                    reals: pick(&self.reals, m, |v| *v),
                    bools: pick(&self.bools, m, |v| *v as u32),
                    units: pick(&self.units, m, |v| v.record.clone()).into_iter().filter(|e| !e.1.is_empty()).collect(),
                    strings: pick(&self.strings, m, |v| v.clone()),
                }
            })
            .collect();
        w3_formats::w3v::Cache { name: self.name.clone(), counter: missions.len() as u32, missions }
    }

    pub fn from_w3v(c: &w3_formats::w3v::Cache) -> SavedCache {
        let mut s = SavedCache { name: c.name.clone(), ..Default::default() };
        for m in &c.missions {
            let n = &m.name;
            s.ints.extend(m.ints.iter().map(|(k, v)| (n.clone(), k.clone(), *v)));
            s.reals.extend(m.reals.iter().map(|(k, v)| (n.clone(), k.clone(), *v)));
            s.bools.extend(m.bools.iter().map(|(k, v)| (n.clone(), k.clone(), *v != 0)));
            s.units.extend(m.units.iter().filter_map(|(k, v)| Some((n.clone(), k.clone(), crate::actions::stored_unit(v)?))));
            s.strings.extend(m.strings.iter().map(|(k, v)| (n.clone(), k.clone(), v.clone())));
        }
        s
    }
}

impl Persist {
    /// The saved caches as the original's `Campaigns.w3v` holds them.
    pub fn caches_w3v(&self) -> w3_formats::w3v::Caches {
        let mut names: Vec<&String> = self.caches.keys().collect();
        names.sort();
        let caches: Vec<_> = names
            .into_iter()
            .map(|n| {
                let mut c = self.caches[n].to_w3v();
                if c.name.is_empty() {
                    c.name = n.clone();
                }
                c
            })
            .collect();
        w3_formats::w3v::Caches { counter: caches.len() as u32, caches }
    }

    /// The saved caches from the original's `Campaigns.w3v`, in place of those kept.
    pub fn take_caches_w3v(&mut self, f: &w3_formats::w3v::Caches) {
        self.caches = f.caches.iter().map(|c| (c.name.to_ascii_lowercase(), SavedCache::from_w3v(c))).collect();
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Persist {
    /// By campaign number (the position in CampaignList).
    pub campaigns: Vec<CampaignState>,
    /// The campaign difficulty chosen on the campaign screen.
    pub difficulty: i32,
    /// Saved game caches by lowercase name.
    pub caches: HashMap<String, SavedCache>,
    /// Campaigns whose race the menu was last set to show (SetCampaignMenuRace).
    pub menu_race: i32,
    /// SetTutorialCleared.
    pub tutorial_cleared: bool,
    /// SetCustomCampaignButtonVisible, by button.
    pub custom_campaign_buttons: HashMap<i32, bool>,
}

impl Persist {
    pub fn campaign(&mut self, n: usize) -> &mut CampaignState {
        if self.campaigns.len() <= n {
            self.campaigns.resize(n + 1, CampaignState::default());
        }
        &mut self.campaigns[n]
    }

    pub fn mission_open(&self, campaign: usize, mission: usize) -> bool {
        self.campaigns.get(campaign).is_some_and(|c| c.missions.get(mission).copied().unwrap_or(false))
    }

    pub fn set_mission(&mut self, campaign: usize, mission: usize, open: bool) {
        let c = self.campaign(campaign);
        if c.missions.len() <= mission {
            c.missions.resize(mission + 1, false);
        }
        c.missions[mission] = open;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The original's saved cache (`docs/reference/cache_sample.w3v`) taken into a
    /// profile and written out again holds the same record: its missions and keys are
    /// in name order there, as this game writes them.
    #[test]
    fn the_originals_cache_comes_back_the_same() {
        let file = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/reference/cache_sample.w3v")).unwrap();
        let f = w3_formats::w3v::Caches::parse(&file).unwrap();
        let mut p = Persist::default();
        p.take_caches_w3v(&f);
        let hero = &p.caches["testcache.w3v"].units[0].2;
        assert_eq!((&hero.type_id, hero.level, hero.str_), (b"Hpal", 1, 22.0));
        let (_, data) = w3_formats::container::unpack(&file).unwrap();
        assert_eq!(p.caches_w3v().data(), data);
    }
}
