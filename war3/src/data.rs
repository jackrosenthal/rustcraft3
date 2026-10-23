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

pub struct GameData {
    pub dir: PathBuf,
    pub version: Version,
    pub chain: Arc<Chain>,
    war3x: Option<Arc<Archive>>,
    war3x_local: Option<Arc<Archive>>,
    patch: Option<Arc<Archive>>,
    pub misc: Profile,
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
        let mut data = GameData { dir: dir.to_path_buf(), version, chain, war3x, war3x_local, patch, misc };
        data.select_data_set(DataSet::Melee);
        Ok(data)
    }

    /// Mounts the rule-set folders for a game (FUN_6f03e070).
    pub fn select_data_set(&mut self, set: DataSet) {
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
