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
    pub ints: Vec<(String, String, i32)>,
    pub reals: Vec<(String, String, f32)>,
    pub bools: Vec<(String, String, bool)>,
    pub strings: Vec<(String, String, String)>,
    pub units: Vec<(String, String, StoredUnit)>,
}

impl SavedCache {
    pub fn from_cache(c: &GameCache) -> SavedCache {
        SavedCache {
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
