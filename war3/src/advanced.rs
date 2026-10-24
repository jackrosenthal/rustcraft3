//! The game setup's advanced options (CAdvancedOptionsPane, AdvancedOptionsPane.fdf) on
//! the Custom Game and local network create screens: the setup's flags, which become
//! the game's map flags (IsMapFlagSet).  As game.dll reads and writes them
//! (FUN_6f304e20): Lock Teams is 0x600 (alliance changes locked, trading among allies),
//! Teams Together 0x40, Full Shared Unit Control 0x10000, Random Races 0x40000, Random
//! Hero 0x20000; observers are 0x30 full, 0x20 on defeat, 0x400000 referees, or none;
//! visibility is 0x8 default, 0x1 hide terrain, 0x2 map explored, or 0x4 always
//! visible.  They start as 0x648 (FUN_6f303550).  The map info pane and the options
//! share their place, each panel's button showing the other.

use w3_ui::{FrameDb, FrameTree};

pub const DEFAULT_FLAGS: u32 = 0x648;

const CHECKS: [(&str, u32); 5] = [("LockTeamsCheckBox", 0x600), ("TeamsTogetherCheckBox", 0x40), ("AdvSharedControlCheckBox", 0x10000), ("RandomRacesCheckBox", 0x40000), ("RandomHeroCheckBox", 0x20000)];

/// The pane in AdvancedOptionsPaneContainer, showing `flags`; the Custom Game screen
/// keeps Random Races and Observers off (CSkirmish's FUN_6f2d2350 with 3 and 5).
pub fn attach(tree: &mut FrameTree, db: &FrameDb, flags: u32, skirmish: bool) {
    if let (Some(parent), true) = (tree.get("AdvancedOptionsPaneContainer"), db.templates.contains_key("AdvancedOptionsPane")) {
        if let Some(i) = tree.create(db, "AdvancedOptionsPane", parent) {
            tree.frames[i].all_points = true;
        }
    }
    for (name, bit) in CHECKS {
        tree.set_checked(name, flags & bit == bit);
    }
    let observers = match (flags & 0x10 != 0, flags & 0x20 != 0) {
        (true, true) => 0,
        (false, true) => 1,
        _ if flags & 0x400000 != 0 => 2,
        _ => 3,
    };
    tree.set_selected("ObserversMenu", observers);
    let visibility = if flags & 8 != 0 {
        0
    } else if flags & 1 != 0 {
        1
    } else if flags & 2 != 0 {
        2
    } else if flags & 4 != 0 {
        3
    } else {
        1
    };
    tree.set_selected("MapVisibilityMenu", visibility);
    if skirmish {
        for name in ["RandomRacesLabel", "RandomRacesCheckBox", "ObserversLabel", "ObserversMenu"] {
            tree.set_enabled(name, false);
        }
    }
    show(tree, false);
}

/// Shows the options (or the map info) in their shared place.
pub fn show(tree: &mut FrameTree, advanced: bool) {
    tree.set_visible("AdvancedOptionsPanel", advanced);
    tree.set_visible("MapInfoPanel", !advanced);
}

/// The flags the options show.
pub fn read(tree: &FrameTree) -> u32 {
    let mut flags = 0;
    for (name, bit) in CHECKS {
        if tree.checked(name) {
            flags |= bit;
        }
    }
    let selected = |n: &str| tree.get(n).map_or(0, |i| tree.frames[i].selected);
    flags |= [0x30, 0x20, 0x400000, 0][selected("ObserversMenu").min(3)];
    flags |= [8, 1, 2, 4][selected("MapVisibilityMenu").min(3)];
    flags
}

/// The game's options as text (AdvancedOptionsDisplay in AdvancedOptionsContainer) for
/// those in its chatroom or choosing it to join, unless they are the defaults
/// (FUN_6f304c80): Yes or No for each choice, then the observers and the visibility.
pub fn display(tree: &mut FrameTree, db: &FrameDb, flags: u32) {
    let Some(container) = tree.get("AdvancedOptionsContainer") else { return };
    if flags == DEFAULT_FLAGS {
        tree.set_visible("AdvancedOptionsContainer", false);
        return;
    }
    tree.set_visible("AdvancedOptionsContainer", true);
    if tree.get("AdvancedOptionsDisplay").is_none() && db.templates.contains_key("AdvancedOptionsDisplay") {
        if let Some(i) = tree.create(db, "AdvancedOptionsDisplay", container) {
            tree.frames[i].all_points = true;
        }
    }
    let yes_no = |on: bool| db.string(if on { "YES" } else { "NO" });
    for (name, bit) in CHECKS {
        let value = name.replace("CheckBox", "Value");
        tree.set_text(&value, &yes_no(flags & bit == bit));
    }
    let observers = match (flags & 0x10 != 0, flags & 0x20 != 0) {
        (true, true) => "FULL_OBSERVERS",
        (false, true) => "OBSERVERS_ON_DEFEAT",
        _ if flags & 0x400000 != 0 => "REFEREES",
        _ => "NO_OBSERVERS",
    };
    tree.set_text("ObserversValue", &db.string(observers));
    let visibility = if flags & 1 != 0 {
        "HIDE_TERRAIN"
    } else if flags & 4 != 0 {
        "ALWAYS_VISIBLE"
    } else if flags & 2 != 0 {
        "MAP_EXPLORED"
    } else {
        "DEFAULT"
    };
    tree.set_text("MapVisibilityValue", &db.string(visibility));
    tree.layout();
}
