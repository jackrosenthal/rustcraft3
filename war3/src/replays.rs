//! Where replays are kept: the last game's as Replay\LastReplay.w3g, and with the
//! autosaveReplay option a copy as Replay\Autosaved\Custom\Replay_<date>.w3g for games
//! that are not campaign missions (FUN_6f2cfe50).

use std::path::{Path, PathBuf};

use w3_game::replay::Replay;

pub fn last_replay() -> Option<PathBuf> {
    Some(crate::data::replay_dir()?.join("LastReplay.w3g"))
}

fn write(path: &Path, bytes: &[u8]) {
    if let Some(d) = path.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    if let Err(e) = std::fs::write(path, bytes) {
        log::warn!("writing {}: {e}", path.display());
    }
}

/// Keeps a finished game's replay.
pub fn keep(replay: &Replay) {
    let bytes = match replay.to_bytes() {
        Ok(b) => b,
        Err(e) => return log::warn!("replay: {e}"),
    };
    if let Some(p) = last_replay() {
        write(&p, &bytes);
    }
    if crate::options::Options::load().autosave_replay && !replay.campaign {
        let t = timestamp();
        if let Some(d) = crate::data::replay_dir() {
            write(&d.join("Autosaved").join("Custom").join(format!("Replay_{t}.w3g")), &bytes);
        }
    }
}

/// The time as "%04u_%02u_%02u_%02u%02u" (in UTC: the standard library has no time zones).
fn timestamp() -> String {
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs()) as i64;
    let (days, rem) = (t.div_euclid(86400), t.rem_euclid(86400));
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}_{m:02}_{d:02}_{:02}{:02}", rem / 3600, rem % 3600 / 60)
}

/// The days since 1970-01-01 as a year, month, and day (Howard Hinnant's algorithm).
pub(crate) fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}
