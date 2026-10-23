//! Command-line options.

use std::ffi::OsString;
use std::path::PathBuf;

use clap::Parser;

/// Command-line options.  The original's switches (from war3.exe's Storm argument table at
/// 0x45d090: -d3d, -opengl, -swtnl, -classic, -datadir, -loadfile, -gametype, -fullscreen,
/// -window) are accepted with either one or two leading dashes.
#[derive(Parser, Debug)]
#[command(name = "war3", version, about = "Warcraft III: Reign of Chaos and The Frozen Throne")]
pub struct Args {
    /// Play Reign of Chaos instead of The Frozen Throne.
    #[arg(long)]
    pub classic: bool,

    /// Directory holding the game's data archives (War3.mpq and friends).
    #[arg(long, value_name = "DIR")]
    pub datadir: Option<PathBuf>,

    /// Map, saved game, or replay to load at startup.
    #[arg(long, value_name = "FILE")]
    pub loadfile: Option<PathBuf>,

    /// Game type to use with --loadfile.
    #[arg(long, value_name = "TYPE")]
    pub gametype: Option<String>,

    /// Run in a window.
    #[arg(long, conflicts_with = "fullscreen")]
    pub window: bool,

    /// Run fullscreen at the display's native resolution (the default).
    #[arg(long)]
    pub fullscreen: bool,

    /// Render with OpenGL.
    #[arg(long, conflicts_with = "d3d")]
    pub opengl: bool,

    /// Render with Direct3D (Direct3D 12 on Windows).
    #[arg(long)]
    pub d3d: bool,

    /// Accepted for compatibility; vertex processing always runs on the GPU.
    #[arg(long)]
    pub swtnl: bool,

    /// Window width; defaults to the display's native width.
    #[arg(long, value_name = "PIXELS")]
    pub width: Option<u32>,

    /// Window height; defaults to the display's native height.
    #[arg(long, value_name = "PIXELS")]
    pub height: Option<u32>,

    /// Disable vertical sync.
    #[arg(long)]
    pub no_vsync: bool,
}

const LEGACY_SWITCHES: &[&str] = &[
    "classic", "datadir", "loadfile", "gametype", "window", "fullscreen", "opengl", "d3d", "swtnl",
];

/// Rewrites the original's single-dash switches (`-window`) to clap's form (`--window`).
pub fn normalize_args(args: impl IntoIterator<Item = OsString>) -> Vec<OsString> {
    args.into_iter()
        .map(|a| match a.to_str() {
            Some(s) if s.starts_with('-') && !s.starts_with("--") => {
                let name = &s[1..];
                if LEGACY_SWITCHES.iter().any(|w| w.eq_ignore_ascii_case(name)) {
                    OsString::from(format!("--{}", name.to_ascii_lowercase()))
                } else {
                    a
                }
            }
            _ => a,
        })
        .collect()
}

