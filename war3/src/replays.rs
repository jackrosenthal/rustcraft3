//! Where replays are kept: the last game's as Replay\LastReplay.w3g, and with the
//! autosaveReplay option a copy as Replay\Autosaved\Custom\Replay_<date>.w3g
//! (FUN_6f2cfe50); none of a campaign mission.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use w3_game::replay::Replay;

/// The games this program has begun, as the original's net layer counts them for its
/// replays' leave records: one more as each game begins, and again as the local
/// player's game is over (`Replay::games`).
static GAMES: AtomicU32 = AtomicU32::new(0);

/// A game begins: the games so far, this one among them.
pub fn game_begun() -> u32 {
    GAMES.fetch_add(1, Ordering::Relaxed) + 1
}

/// The local player's game is over (a defeat or a victory: read, the original's next
/// game counted 3 after a first game's victory).
pub fn game_over() {
    GAMES.fetch_add(1, Ordering::Relaxed);
}

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

/// A map as the original's replay record names and checks it: its path from the
/// game's folder (a campaign mission's, its name in the game's archives), its playable
/// size, and its checksums over the game's scripts (as a LAN game's description has
/// them).  A map file outside the game's folder goes by its name in Maps (provisional:
/// the original plays only maps in its folder).
pub fn map_stat(data: &crate::data::GameData, map: &Path) -> Option<w3_game::replay::MapStat> {
    let (bytes, path) = if map.is_file() {
        let canonical = || Some(std::fs::canonicalize(map).ok()?.strip_prefix(std::fs::canonicalize(&data.dir).ok()?).ok()?.to_path_buf());
        let path = match map.strip_prefix(&data.dir).ok().map(Path::to_path_buf).or_else(canonical) {
            Some(p) => p.to_string_lossy().replace('/', "\\"),
            None => format!("Maps\\{}", map.file_name()?.to_string_lossy()),
        };
        (std::fs::read(map).ok()?, path)
    } else {
        let name = map.to_string_lossy().replace('/', "\\");
        (data.chain.read(&name).ok()?, name)
    };
    let archive = w3_mpq::Archive::from_bytes(map, bytes.clone()).ok()?;
    let info = w3_formats::map::MapInfo::parse(&archive.read("war3map.w3i").ok()?).ok()?;
    let (common, blizzard) = (data.chain.read("Scripts\\common.j").unwrap_or_default(), data.chain.read("Scripts\\blizzard.j").unwrap_or_default());
    let checksums = w3_net::map::Checksums::compute(&bytes, &common, &blizzard, |n| archive.read(n).ok());
    Some(w3_game::replay::MapStat { path, width: info.playable_width as u16, height: info.playable_height as u16, xoro: checksums.xoro, sha1: checksums.sha1 })
}

/// Keeps a finished game's replay: none of a campaign mission, whose game the original
/// records nowhere (read: its Sentinels chapter one, quit through the menu, left
/// LastReplay.w3g as it was and made no TempReplay.w3g).
pub fn keep(replay: &Replay) {
    if replay.campaign {
        return;
    }
    let bytes = match replay.to_bytes() {
        Ok(b) => b,
        Err(e) => return log::warn!("replay: {e}"),
    };
    if let Some(p) = last_replay() {
        write(&p, &bytes);
    }
    if crate::options::Options::load().autosave_replay {
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
