//! Game data: the archive stack and the shared text and SLK tables loaded from it.
//!
//! The layering mirrors game.dll 1.27b (FUN_6f03df20, FUN_6f03e070, FUN_6f046800):
//! `War3.mpq` (priority 0) and `War3x.mpq` (1) are always mounted, then `War3xLocal.mpq`
//! (3) and `War3Patch.mpq` (8).  When a game starts, the `Melee_V<n>` or `Custom_V<n>`
//! folder of War3x, War3xLocal, and War3Patch is mounted above them at priorities 9-12,
//! where `n` is 0 for Reign of Chaos and 1 for The Frozen Throne.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, bail};
use w3_formats::profile::Profile;
use w3_formats::slk::Table;
use w3_mpq::{Archive, Chain};

/// The two game versions game.dll distinguishes (`DAT_6fcd52f0`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Version {
    ReignOfChaos = 0,
    FrozenThrone = 1,
}

/// Rule sets selected per game (`PTR_s_Melee_6fcd5050`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DataSet {
    Melee = 0,
    Custom = 1,
}

impl DataSet {
    fn folder(self) -> &'static str {
        match self {
            DataSet::Melee => "Melee",
            DataSet::Custom => "Custom",
        }
    }
}

const PRIORITY_DATA_SET: [i32; 4] = [9, 10, 11, 12];
/// Maps are mounted above everything else.
pub const PRIORITY_MAP: i32 = 100;
/// A custom campaign's archive: under its maps, over the game's data.
pub const PRIORITY_CAMPAIGN: i32 = 50;

pub struct GameData {
    pub dir: PathBuf,
    pub version: Version,
    pub chain: Arc<Chain>,
    war3x: Option<Arc<Archive>>,
    war3x_local: Option<Arc<Archive>>,
    patch: Option<Arc<Archive>>,
    pub misc: Profile,
    /// The player's profile (campaign progress and saved game caches), kept on disk
    /// per data version.
    pub persist: std::rc::Rc<std::cell::RefCell<w3_game::persist::Persist>>,
    /// Whether the next game is a campaign mission (it takes the profile's difficulty).
    pub campaign_game: std::cell::Cell<bool>,
    /// The game setup's advanced options for the next game (crate::advanced).
    pub setup_flags: std::cell::Cell<u32>,
}

/// The player profiles (the original keeps their names in the registry's userlocal
/// list): each one's campaign progress and saved games live in its own folder.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Profiles {
    pub entries: Vec<ProfileEntry>,
    /// The chosen profile, by position.
    pub current: Option<usize>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ProfileEntry {
    pub name: String,
    /// Its folder under the user's data directory.
    pub dir: String,
}

/// The longest profile name (NewProfileEditBox's limit, FUN_6f104350 with 15).
pub const PROFILE_NAME_MAX: usize = 15;

fn data_root() -> Option<PathBuf> {
    Some(dirs::data_dir()?.join("war3-remaster"))
}

impl Profiles {
    fn path() -> Option<PathBuf> {
        Some(data_root()?.join("profiles.json"))
    }

    /// The profiles; before any were kept, progress from earlier versions of this port
    /// (kept directly in the data directory) is the one profile, under `legacy_name`.
    pub fn load(legacy_name: &str) -> Profiles {
        if let Some(p) = Self::path().and_then(|p| std::fs::read(p).ok()).and_then(|b| serde_json::from_slice(&b).ok()) {
            return p;
        }
        let root = data_root();
        let legacy = root.as_ref().is_some_and(|r| ["profile_v0.json", "profile_v1.json", "Save"].iter().any(|f| r.join(f).exists()));
        if legacy {
            return Profiles { entries: vec![ProfileEntry { name: legacy_name.to_string(), dir: String::new() }], current: Some(0) };
        }
        Profiles::default()
    }

    pub fn store(&self) {
        let Some(path) = Self::path() else { return };
        if let Some(d) = path.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        if let Ok(b) = serde_json::to_vec_pretty(self) {
            if let Err(e) = std::fs::write(&path, b) {
                log::warn!("writing {}: {e}", path.display());
            }
        }
    }

    pub fn current(&self) -> Option<&ProfileEntry> {
        self.entries.get(self.current?)
    }

    pub fn find(&self, name: &str) -> Option<usize> {
        self.entries.iter().position(|e| e.name.eq_ignore_ascii_case(name))
    }

    /// Adds a profile in a folder of its own and chooses it.
    pub fn add(&mut self, name: &str) -> usize {
        let n = (1..).find(|n| !self.entries.iter().any(|e| e.dir == format!("Profiles\\Profile{n}"))).unwrap();
        self.entries.push(ProfileEntry { name: name.to_string(), dir: format!("Profiles\\Profile{n}") });
        self.current = Some(self.entries.len() - 1);
        self.entries.len() - 1
    }

    /// Removes a profile and its folder's progress.
    pub fn remove(&mut self, k: usize) {
        if k >= self.entries.len() {
            return;
        }
        let e = self.entries.remove(k);
        if !e.dir.is_empty() {
            if let Some(d) = data_root().map(|r| r.join(e.dir.replace('\\', "/"))) {
                let _ = std::fs::remove_dir_all(d);
            }
        }
        self.current = match self.current {
            Some(c) if c == k => None,
            Some(c) if c > k => Some(c - 1),
            c => c,
        };
    }
}

/// The current profile's folder.
fn profile_dir() -> Option<PathBuf> {
    let p = Profiles::load("");
    let dir = p.current()?.dir.clone();
    Some(data_root()?.join(dir.replace('\\', "/")))
}

/// Where the current profile's progress lives, one file per data version.
pub fn profile_path(version: Version) -> Option<PathBuf> {
    Some(profile_dir()?.join(format!("profile_v{}.json", version as u32)))
}

/// Where the current profile's saved games live, one folder per data version.
pub fn save_dir(version: Version) -> Option<PathBuf> {
    Some(profile_dir()?.join("Save").join(format!("v{}", version as u32)))
}

/// Where replays live (the original's Replay folder).
pub fn replay_dir() -> Option<PathBuf> {
    Some(data_root()?.join("Replay"))
}

/// Where network games are saved (the original's Save\Multiplayer).
/// Where screen captures go (the original's Screenshots).
pub fn screenshot_dir() -> Option<PathBuf> {
    Some(data_root()?.join("Screenshots"))
}

/// Where maps' preload files are written and read (the original's CustomMapData).
pub fn custom_map_data_dir() -> Option<PathBuf> {
    Some(data_root()?.join("CustomMapData"))
}

pub fn multiplayer_save_dir() -> Option<PathBuf> {
    Some(data_root()?.join("Save").join("Multiplayer"))
}

/// The current profile's progress (a fresh one at Normal difficulty when it has none).
pub fn load_persist(version: Version) -> w3_game::persist::Persist {
    profile_path(version)
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| w3_game::persist::Persist { difficulty: 1, ..Default::default() })
}

fn open_optional(dir: &Path, name: &str) -> anyhow::Result<Option<Arc<Archive>>> {
    // Archive names are matched case-insensitively, as on the original's file system.
    let Some(path) = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .find(|p| p.file_name().is_some_and(|f| f.to_string_lossy().eq_ignore_ascii_case(name)))
    else {
        return Ok(None);
    };
    let a = Archive::open(&path).with_context(|| format!("opening {}", path.display()))?;
    log::info!("mounted {}", path.display());
    Ok(Some(Arc::new(a)))
}

impl GameData {
    pub fn open(dir: &Path, classic: bool) -> anyhow::Result<GameData> {
        let chain = Arc::new(Chain::new());
        let Some(war3) = open_optional(dir, "War3.mpq")? else {
            bail!("War3.mpq not found in {}", dir.display());
        };
        chain.mount(war3, None, 0);
        let war3x = open_optional(dir, "War3X.mpq")?;
        if let Some(a) = &war3x {
            chain.mount(a.clone(), None, 1);
        }
        let war3x_local = open_optional(dir, "War3XLocal.mpq")?;
        if let Some(a) = &war3x_local {
            chain.mount(a.clone(), None, 3);
        }
        let patch = open_optional(dir, "War3Patch.mpq")?;
        if let Some(a) = &patch {
            chain.mount(a.clone(), None, 8);
        }
        // FUN_6f046800: the expansion is used when installed unless -classic is given.
        let version = if classic || war3x.is_none() { Version::ReignOfChaos } else { Version::FrozenThrone };
        let misc = Profile::parse(&chain.read_string("UI\\MiscData.txt").unwrap_or_default());
        let persist = load_persist(version);
        let data = GameData { dir: dir.to_path_buf(), version, chain, war3x, war3x_local, patch, misc, persist: std::rc::Rc::new(std::cell::RefCell::new(persist)), campaign_game: std::cell::Cell::new(false), setup_flags: std::cell::Cell::new(crate::advanced::DEFAULT_FLAGS) };
        data.select_data_set(DataSet::Melee);
        Ok(data)
    }

    /// Writes the profile to disk.
    pub fn save_profile(&self) {
        let Some(path) = profile_path(self.version) else { return };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        match serde_json::to_string_pretty(&*self.persist.borrow()) {
            Ok(s) => {
                if let Err(e) = std::fs::write(&path, s) {
                    log::warn!("saving profile {}: {e}", path.display());
                }
            }
            Err(e) => log::warn!("profile: {e}"),
        }
    }

    /// Mounts the rule-set folders for a game (FUN_6f03e070).
    pub fn select_data_set(&self, set: DataSet) {
        for p in PRIORITY_DATA_SET {
            self.chain.unmount(p);
        }
        let folder = format!("{}_V{}", set.folder(), self.version as u32);
        // With the expansion installed the folders come from War3x, War3xLocal, and
        // War3Patch (mounted twice, at 11 and 12, as the original does).
        let sources = [&self.war3x, &self.war3x_local, &self.patch, &self.patch];
        for (src, prio) in sources.into_iter().zip(PRIORITY_DATA_SET) {
            if let Some(a) = src {
                self.chain.mount(a.clone(), Some(&folder), prio);
            }
        }
    }

    pub fn slk(&self, path: &str) -> Table {
        match self.chain.read_string(path) {
            Ok(s) => Table::parse(&s),
            Err(e) => {
                log::warn!("{path}: {e}");
                Table::default()
            }
        }
    }

    pub fn profile(&self, paths: &[&str]) -> Profile {
        let mut p = Profile::default();
        for path in paths {
            match self.chain.read_string(path) {
                Ok(s) => p.merge(&s),
                Err(e) => log::warn!("{path}: {e}"),
            }
        }
        p
    }
}

/// Locates the game directory: the given one, else the current directory or a
/// `Warcraft III` folder beside the executable or the current directory.
pub fn find_data_dir(given: Option<&Path>) -> anyhow::Result<PathBuf> {
    if let Some(d) = given {
        return Ok(d.to_path_buf());
    }
    let mut candidates = vec![std::env::current_dir()?];
    if let Ok(exe) = std::env::current_exe() {
        if let Some(p) = exe.parent() {
            candidates.push(p.to_path_buf());
        }
    }
    let with_sub: Vec<PathBuf> = candidates.iter().map(|c| c.join("Warcraft III")).collect();
    candidates.extend(with_sub);
    for c in &candidates {
        if open_optional_exists(c, "War3.mpq") {
            return Ok(c.clone());
        }
    }
    bail!("could not find the Warcraft III data files; pass --datadir")
}

fn open_optional_exists(dir: &Path, name: &str) -> bool {
    std::fs::read_dir(dir).is_ok_and(|rd| {
        rd.filter_map(|e| e.ok())
            .any(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(name))
    })
}
