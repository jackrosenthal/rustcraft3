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
    pub customkeys: bool,
    pub netgameport: u32,
    #[serde(rename = "autosaveReplay")]
    pub autosave_replay: bool,
    /// The chat font chosen from UI\MiscUI.txt's [ChatFonts].
    pub chatsupport: u32,
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
    /// The name typed on the Local Area Network screen (setting 0x41, empty at first).
    pub lanname: String,
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
            customkeys: false,
            netgameport: 6112,
            autosave_replay: false,
            chatsupport: 0,
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
            lanname: String::new(),
        }
    }
}

/// The registry key the original keeps its settings under (HKEY_CURRENT_USER).
const KEY: &str = "Software\\Blizzard Entertainment\\Warcraft III";

/// Each setting's key (below `KEY`) and value name, as the original's registry has them.
const PLACES: [(&str, &str); 25] = [
    ("Gameplay", "mousescroll"),
    ("Gameplay", "mousescrolldisable"),
    ("Gameplay", "keyscroll"),
    ("Gameplay", "tooltips"),
    ("Gameplay", "healthbars"),
    ("Gameplay", "formationtoggle"),
    ("Gameplay", "subgrouporder"),
    ("Gameplay", "customkeys"),
    ("Gameplay", "netgameport"),
    ("Gameplay", "autosaveReplay"),
    ("Misc", "chatsupport"),
    ("Sound", "sfx"),
    ("Sound", "sfxvolume"),
    ("Sound", "music"),
    ("Sound", "musicvolume"),
    ("Sound", "ambient"),
    ("Sound", "movement"),
    ("Sound", "unit"),
    ("Sound", "subtitles"),
    ("Sound", "environmental"),
    ("Sound", "positional"),
    ("Video", "gamma"),
    ("Video", "particles"),
    ("Video", "lights"),
    ("Video", "unitshadows"),
];

impl Options {
    /// The settings as the original keeps them: its registry keys and values, written as
    /// a registry file (REGEDIT4) under the user's data folder.
    fn path() -> Option<PathBuf> {
        Some(dirs::data_dir()?.join("war3-remaster").join("Warcraft III.reg"))
    }

    /// Before the registry file, the settings were JSON.
    fn old_path() -> Option<PathBuf> {
        Some(dirs::data_dir()?.join("war3-remaster").join("options.json"))
    }

    pub fn load() -> Options {
        if let Some(text) = Self::path().and_then(|p| std::fs::read_to_string(p).ok()) {
            return Self::from_reg(&text);
        }
        Self::old_path().and_then(|p| std::fs::read(p).ok()).and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }

    pub fn store(&self) {
        let Some(path) = Self::path() else { return };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Err(e) = std::fs::write(&path, self.to_reg()) {
            log::warn!("writing {}: {e}", path.display());
        }
    }

    /// The settings by name, as serde has them (numbers, flags, and strings).
    fn values(&self) -> serde_json::Map<String, serde_json::Value> {
        match serde_json::to_value(self) {
            Ok(serde_json::Value::Object(m)) => m,
            _ => Default::default(),
        }
    }

    /// The registry file: a key per section, flags and numbers as DWORDs, and the LAN
    /// screen's name as the String key's "userlocal".
    fn to_reg(&self) -> String {
        let v = self.values();
        let mut out = String::from("REGEDIT4\r\n");
        let mut sections: Vec<&str> = PLACES.iter().map(|p| p.0).collect();
        sections.dedup();
        sections.sort();
        sections.dedup();
        for sec in sections {
            out += &format!("\r\n[HKEY_CURRENT_USER\\{KEY}\\{sec}]\r\n");
            for (_, name) in PLACES.iter().filter(|p| p.0 == sec) {
                let n = match v.get(*name) {
                    Some(serde_json::Value::Bool(b)) => *b as u64,
                    Some(serde_json::Value::Number(n)) => n.as_u64().unwrap_or(0),
                    _ => continue,
                };
                out += &format!("\"{name}\"=dword:{n:08x}\r\n");
            }
        }
        out += &format!("\r\n[HKEY_CURRENT_USER\\{KEY}\\String]\r\n\"userlocal\"=\"{}\"\r\n", self.lanname.replace('\\', "\\\\").replace('"', "\\\""));
        out
    }

    /// Reads a registry file; settings it lacks keep their defaults.
    fn from_reg(text: &str) -> Options {
        let mut v = Options::default().values();
        let mut section = String::new();
        for line in text.lines().map(str::trim) {
            if let Some(key) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                section = key.rsplit('\\').next().unwrap_or_default().to_string();
                continue;
            }
            let Some((name, value)) = line.split_once('=') else { continue };
            let name = name.trim_matches('"');
            if section == "String" && name == "userlocal" {
                let s = value.strip_prefix('"').and_then(|v| v.strip_suffix('"')).unwrap_or(value).replace("\\\"", "\"").replace("\\\\", "\\");
                v.insert("lanname".into(), serde_json::Value::String(s));
                continue;
            }
            if !PLACES.iter().any(|p| p.0 == section && p.1 == name) {
                continue;
            }
            let Some(n) = value.strip_prefix("dword:").and_then(|h| u32::from_str_radix(h, 16).ok()) else { continue };
            let new = match v.get(name) {
                Some(serde_json::Value::Bool(_)) => serde_json::Value::Bool(n != 0),
                _ => serde_json::Value::from(n),
            };
            v.insert(name.into(), new);
        }
        serde_json::from_value(serde_json::Value::Object(v)).unwrap_or_default()
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
        tree.set_text("CustomKeysValue", &db.string(if o.customkeys { "ON" } else { "OFF" }));
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

/// The main menu's options (COptionsMenu, OptionsMenu.fdf): the Gameplay, Video, and
/// Sound panels, chosen by their buttons, with OK and Cancel.
pub mod menu {
    use w3_ui::{FrameDb, FrameTree};

    use super::Options;

    pub const PANELS: [&str; 3] = ["GameplayPanel", "VideoPanel", "SoundPanel"];

    /// The video menus whose quality the renderer does not vary: shown at High.
    const FIXED_HIGH: [&str; 4] = ["ModelDetailMenu", "AnimQualityMenu", "TextureQualityMenu", "SpellFilterMenu"];

    /// Puts the options in the controls (FUN_6f2b2c50 and its kin).  No panel shows and
    /// OK waits until a panel is chosen.
    pub fn fill(tree: &mut FrameTree, db: &FrameDb, o: &Options, resolution: (u32, u32)) {
        for p in PANELS {
            tree.set_visible(p, false);
        }
        tree.set_enabled("OKButton", false);
        tree.set_visible("OptionsConfirmDialog", false);
        // Chat support shows only with [ChatFonts] ChatFontEnabled set, listing its
        // ChatFontDisplayName<n>.
        let fonts = &db.misc_ui;
        let chat = fonts.get_str("ChatFonts", "ChatFontEnabled").is_some_and(|v| v.trim() != "0");
        for n in ["ChatSupportMenu", "ChatSupportInfoText", "ChatSupportLabel", "ChatSupportBackdrop"] {
            tree.set_visible(n, chat);
        }
        if chat {
            let count = fonts.get_str("ChatFonts", "ChatFontCount").and_then(|v| v.trim().parse().ok()).unwrap_or(0);
            let names: Vec<String> = (0..count).map(|k| fonts.get_str("ChatFonts", &format!("ChatFontDisplayName{k}")).unwrap_or_default()).collect();
            let sel = (o.chatsupport as usize).min(names.len().saturating_sub(1));
            tree.set_items("ChatSupportMenu", names, sel);
        }
        // Gameplay.
        tree.set_value("MouseScrollSlider", o.mousescroll as f32);
        tree.set_checked("MouseScrollDisableCheckBox", o.mousescrolldisable);
        tree.set_value("KeyScrollSlider", o.keyscroll as f32);
        tree.set_checked("TooltipsCheckBox", o.tooltips);
        tree.set_checked("HealthBarsCheckBox", o.healthbars);
        tree.set_checked("SubgroupCheckBox", o.subgrouporder);
        tree.set_checked("FormationToggleCheckBox", o.formationtoggle);
        tree.set_checked("CustomKeysCheckBox", o.customkeys);
        tree.set_checked("AutosaveReplayCheckBox", o.autosave_replay);
        tree.set_text("GamePortEditBox", &o.netgameport.to_string());
        // Video: the display's one resolution (FUN_6f2ab650's "%dx%dx%d").
        tree.set_value("GammaSlider", o.gamma as f32);
        tree.set_items("ResolutionMenu", vec![format!("{}x{}x{}", resolution.0, resolution.1, 32)], 0);
        tree.set_enabled("ResolutionMenu", false);
        for n in FIXED_HIGH {
            tree.set_selected(n, 2);
            tree.set_enabled(n, false);
        }
        tree.set_selected("ParticlesMenu", o.particles as usize);
        tree.set_selected("LightsMenu", o.lights as usize);
        tree.set_selected("ShadowsMenu", o.unitshadows as usize);
        tree.set_selected("OcclusionMenu", o.occlusion as usize);
        // Sound.  There is one sound system, so no provider to choose.
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
        tree.set_enabled("ProviderMenu", false);
        sync(tree);
    }

    /// Controls that follow others: the volume sliders their checkboxes (FUN_6f2b5e60,
    /// FUN_6f2b62c0), the mouse scroll slider its disable box.
    pub fn sync(tree: &mut FrameTree) {
        let sfx = tree.checked("SoundCheckBox");
        tree.set_enabled("SoundVolumeSlider", sfx);
        let music = tree.checked("MusicCheckBox");
        tree.set_enabled("MusicVolumeSlider", music);
        let mouse = !tree.checked("MouseScrollDisableCheckBox");
        tree.set_enabled("MouseScrollSlider", mouse);
    }

    /// Shows a panel; choosing one lets OK be pressed (COptionsMenu vf3 cases 0 to 2).
    pub fn show(tree: &mut FrameTree, panel: usize) {
        for (k, p) in PANELS.iter().enumerate() {
            tree.set_visible(p, k == panel);
        }
        tree.set_enabled("OKButton", true);
    }

    /// The options as the controls show them.
    pub fn read(tree: &FrameTree, o: &mut Options) {
        let v = |n: &str| tree.value(n).round().max(0.0) as u32;
        let sel = |n: &str| tree.get(n).map_or(0, |i| tree.frames[i].selected) as u32;
        o.mousescroll = v("MouseScrollSlider");
        o.mousescrolldisable = tree.checked("MouseScrollDisableCheckBox");
        o.keyscroll = v("KeyScrollSlider");
        o.tooltips = tree.checked("TooltipsCheckBox");
        o.healthbars = tree.checked("HealthBarsCheckBox");
        o.subgrouporder = tree.checked("SubgroupCheckBox");
        o.formationtoggle = tree.checked("FormationToggleCheckBox");
        o.customkeys = tree.checked("CustomKeysCheckBox");
        o.autosave_replay = tree.checked("AutosaveReplayCheckBox");
        if let Some(port) = tree.get("GamePortEditBox").and_then(|i| tree.frames[i].text.as_deref()?.trim().parse().ok()) {
            o.netgameport = port;
        }
        o.chatsupport = sel("ChatSupportMenu");
        o.gamma = v("GammaSlider");
        o.particles = sel("ParticlesMenu");
        o.lights = sel("LightsMenu");
        o.unitshadows = sel("ShadowsMenu") != 0;
        o.occlusion = sel("OcclusionMenu") != 0;
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
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Settings go to the registry file and back as they were.
    #[test]
    fn settings_keep_through_the_registry_file() {
        let o = Options { mousescroll: 70, healthbars: true, musicvolume: 12, chatsupport: 2, lanname: "Jack \"J\"".into(), ..Default::default() };
        let reg = o.to_reg();
        assert!(reg.contains("[HKEY_CURRENT_USER\\Software\\Blizzard Entertainment\\Warcraft III\\Gameplay]"));
        assert!(reg.contains("\"mousescroll\"=dword:00000046"));
        let back = Options::from_reg(&reg);
        assert_eq!(serde_json::to_value(&back).unwrap(), serde_json::to_value(&o).unwrap());
    }
}
