//! The player's options: what the original keeps in the registry under Warcraft III's
//! Gameplay, Sound, and Video keys, with the defaults of game.dll's settings table
//! (0x6fa7efe8: name, key, type, default).  Kept on disk in the user's data directory.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Options {
    // Gameplay.
    pub mousescroll: u32,
    pub mousescrolldisable: bool,
    pub keyscroll: u32,
    pub tooltips: bool,
    pub healthbars: bool,
    pub formationtoggle: bool,
    pub subgrouporder: bool,
    // Sound.
    pub sfx: bool,
    pub sfxvolume: u32,
    pub music: bool,
    pub musicvolume: u32,
    pub ambient: bool,
    pub movement: bool,
    pub unit: bool,
    pub subtitles: bool,
    pub environmental: bool,
    pub positional: bool,
    // Video.  The original picks the quality settings from the hardware the first time
    // it runs; the renderer here always draws everything, at the highest settings.
    pub gamma: u32,
    pub particles: u32,
    pub lights: u32,
    pub unitshadows: bool,
    pub occlusion: bool,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            mousescroll: 50,
            mousescrolldisable: false,
            keyscroll: 50,
            tooltips: true,
            healthbars: false,
            formationtoggle: true,
            subgrouporder: false,
            sfx: true,
            sfxvolume: 100,
            music: true,
            musicvolume: 40,
            ambient: true,
            movement: true,
            unit: true,
            subtitles: false,
            environmental: true,
            positional: true,
            gamma: 30,
            particles: 2,
            lights: 2,
            unitshadows: true,
            occlusion: true,
        }
    }
}

impl Options {
    fn path() -> Option<PathBuf> {
        Some(dirs::data_dir()?.join("war3-remaster").join("options.json"))
    }

    pub fn load() -> Options {
        Self::path().and_then(|p| std::fs::read(p).ok()).and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }

    pub fn store(&self) {
        let Some(path) = Self::path() else { return };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        match serde_json::to_vec_pretty(self) {
            Ok(b) => {
                if let Err(e) = std::fs::write(&path, b) {
                    log::warn!("writing {}: {e}", path.display());
                }
            }
            Err(e) => log::warn!("options: {e}"),
        }
    }

    /// The scroll rate multipliers (keyboard, mouse): a setting of 0 to 100 is 0.5 to
    /// 1.5 times the camera's rate (game.dll FUN_6f363920 and FUN_6f363950).
    pub fn scroll_factors(&self) -> (f32, f32) {
        (self.keyscroll.min(100) as f32 / 100.0 + 0.5, self.mousescroll.min(100) as f32 / 100.0 + 0.5)
    }

    /// The sound effect and music volumes, 0 to 1.
    pub fn volumes(&self) -> (f32, f32) {
        let v = |on: bool, v: u32| if on { v.min(100) as f32 / 100.0 } else { 0.0 };
        (v(self.sfx, self.sfxvolume), v(self.music, self.musicvolume))
    }
}

/// The game menu's options panels (CEscMenuOptionsPanel): their pages, and the
/// controls' values to and from the options.
pub mod panel {
    use w3_ui::{FrameDb, FrameTree};

    use super::Options;

    pub const PAGES: [&str; 5] = ["OptionsPanel", "GameplayPanel", "VideoPanel", "SoundPanel", "NetworkPanel"];

    /// Shows one page; the OK and Cancel buttons go with every page but the first.
    pub fn show(tree: &mut FrameTree, page: &str) {
        for p in PAGES {
            tree.set_visible(p, p == page);
        }
        tree.set_visible("BottomButtonPanel", page != "OptionsPanel");
        // Network options are for network games (vf55_6f3b1ea0 hides the button).
        tree.set_visible("NetworkButton", false);
    }

    /// The game speed's name (FUN_6f3ce3e0).
    pub fn speed_name(db: &FrameDb, speed: usize) -> String {
        db.string(["SLOW", "NORMAL", "FAST"].get(speed).copied().unwrap_or("UNKNOWN"))
    }

    pub struct Context {
        pub speed: usize,
        /// The difficulty of a campaign mission (shown only then).
        pub difficulty: Option<i32>,
        pub resolution: (u32, u32),
    }

    pub fn fill(tree: &mut FrameTree, db: &FrameDb, o: &Options, c: &Context) {
        // Gameplay.
        tree.set_value("GameSpeedSlider", c.speed as f32);
        tree.set_text("GameSpeedValue", &speed_name(db, c.speed));
        tree.set_value("MouseScrollSlider", o.mousescroll as f32);
        tree.set_checked("MouseScrollDisable", o.mousescrolldisable);
        tree.set_value("KeyScrollSlider", o.keyscroll as f32);
        tree.set_checked("TooltipsCheckBox", o.tooltips);
        tree.set_checked("SubgroupCheckBox", o.subgrouporder);
        tree.set_checked("FormationToggleCheckBox", o.formationtoggle);
        tree.set_checked("HealthBarsCheckBox", o.healthbars);
        // Custom keys (CustomKeys.txt) are not read: always off.
        tree.set_text("CustomKeysValue", &db.string("OFF"));
        for n in ["DifficultyLabel", "DifficultyValue"] {
            tree.set_visible(n, c.difficulty.is_some());
        }
        if let Some(d) = c.difficulty {
            tree.set_text("DifficultyValue", &db.string(["EASY", "NORMAL", "HARD"].get(d as usize).copied().unwrap_or("UNKNOWN")));
        }
        // Video.
        tree.set_value("GammaSlider", o.gamma as f32);
        for (menu, v) in [("EscOptionsParticlesMenu", o.particles as usize), ("EscOptionsLightsMenu", o.lights as usize), ("EscOptionsShadowsMenu", o.unitshadows as usize), ("EscOptionsOcclusionMenu", o.occlusion as usize)] {
            tree.set_selected(menu, v);
        }
        tree.set_text("ResolutionValue", &format!("{}x{}x32", c.resolution.0, c.resolution.1));
        for n in ["ModelDetailValue", "AnimQualityValue", "TextureQualityValue", "SpellFilterValue"] {
            tree.set_text(n, &db.string("HIGH"));
        }
        // Sound.
        tree.set_checked("SoundCheckBox", o.sfx);
        tree.set_value("SoundVolumeSlider", o.sfxvolume as f32);
        tree.set_checked("MusicCheckBox", o.music);
        tree.set_value("MusicVolumeSlider", o.musicvolume as f32);
        tree.set_checked("AmbientCheckBox", o.ambient);
        tree.set_checked("MovementCheckBox", o.movement);
        tree.set_checked("UnitCheckBox", o.unit);
        tree.set_checked("SubtitlesCheckBox", o.subtitles);
        tree.set_checked("EnviroCheckBox", o.environmental);
        tree.set_checked("PositionalCheckBox", o.positional);
        sync(tree);
    }

    /// Controls that follow others: the volume sliders their checkboxes, the mouse
    /// scroll slider its disable box.
    pub fn sync(tree: &mut FrameTree) {
        let sfx = tree.checked("SoundCheckBox");
        tree.set_enabled("SoundVolumeSlider", sfx);
        let music = tree.checked("MusicCheckBox");
        tree.set_enabled("MusicVolumeSlider", music);
        let mouse = !tree.checked("MouseScrollDisable");
        tree.set_enabled("MouseScrollSlider", mouse);
    }

    /// The options as the page shows them, and the game speed chosen.
    pub fn read(tree: &FrameTree, page: &str, o: &mut Options) -> Option<usize> {
        let v = |n: &str| tree.value(n).round().max(0.0) as u32;
        match page {
            "GameplayPanel" => {
                o.mousescroll = v("MouseScrollSlider");
                o.mousescrolldisable = tree.checked("MouseScrollDisable");
                o.keyscroll = v("KeyScrollSlider");
                o.tooltips = tree.checked("TooltipsCheckBox");
                o.subgrouporder = tree.checked("SubgroupCheckBox");
                o.formationtoggle = tree.checked("FormationToggleCheckBox");
                o.healthbars = tree.checked("HealthBarsCheckBox");
                return Some(v("GameSpeedSlider") as usize);
            }
            "VideoPanel" => {
                o.gamma = v("GammaSlider");
                let sel = |n: &str| tree.get(n).map_or(0, |i| tree.frames[i].selected) as u32;
                o.particles = sel("EscOptionsParticlesMenu");
                o.lights = sel("EscOptionsLightsMenu");
                o.unitshadows = sel("EscOptionsShadowsMenu") != 0;
                o.occlusion = sel("EscOptionsOcclusionMenu") != 0;
            }
            "SoundPanel" => {
                o.sfx = tree.checked("SoundCheckBox");
                o.sfxvolume = v("SoundVolumeSlider");
                o.music = tree.checked("MusicCheckBox");
                o.musicvolume = v("MusicVolumeSlider");
                o.ambient = tree.checked("AmbientCheckBox");
                o.movement = tree.checked("MovementCheckBox");
                o.unit = tree.checked("UnitCheckBox");
                o.subtitles = tree.checked("SubtitlesCheckBox");
                o.environmental = tree.checked("EnviroCheckBox");
                o.positional = tree.checked("PositionalCheckBox");
            }
            _ => {}
        }
        None
    }
}
