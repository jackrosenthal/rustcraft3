//! Preload files (Preload, PreloadGenStart, PreloadGenEnd, Preloader), as game.dll
//! keeps them: in CustomMapData under the player's folder, named with .pld unless they
//! end in .pld, .txt, .sav, or .mgv.  PreloadGenEnd writes a PreloadFiles function
//! calling Preload for each file preloaded since PreloadGenClear and PreloadEnd with the
//! seconds since PreloadGenStart (FUN_6f230170); Preloader compiles such a file after
//! common.j and runs its PreloadFiles (FUN_6f20fbb0).

use std::path::PathBuf;

use crate::world::World;

#[derive(Debug, Default)]
pub struct Preload {
    pub dir: Option<PathBuf>,
    pub files: Vec<String>,
    pub started: Option<std::time::Instant>,
}

impl Clone for Preload {
    fn clone(&self) -> Preload {
        Preload { dir: self.dir.clone(), files: self.files.clone(), started: self.started }
    }
}

impl Preload {
    /// The file a name stands for, within the folder.
    fn path(&self, name: &str) -> Option<PathBuf> {
        let name = name.replace('/', "\\");
        if name.is_empty() || name.contains("..") || name.contains(':') || name.starts_with('\\') {
            return None;
        }
        let lower = name.to_ascii_lowercase();
        let name = if [".pld", ".txt", ".sav", ".mgv"].iter().any(|e| lower.ends_with(e)) { name } else { format!("{name}.pld") };
        let mut p = self.dir.clone()?;
        for part in name.split('\\').filter(|s| !s.is_empty()) {
            p.push(part);
        }
        Some(p)
    }
}

impl World {
    pub fn write_preload_file(&mut self, name: &str) {
        let Some(path) = self.preload.path(name) else { return };
        let mut out = String::from("function PreloadFiles takes nothing returns nothing\n\r\n");
        for f in &self.preload.files {
            out.push_str(&format!("\tcall Preload( \"{}\" )\r\n", f.replace('\\', "\\\\")));
        }
        let secs = self.preload.started.map_or(0.0, |t| t.elapsed().as_secs_f64());
        out.push_str(&format!("\tcall PreloadEnd( {secs:.1} )\r\n"));
        out.push_str("\nendfunction\n\n\r\n");
        if let Some(d) = path.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        if let Err(e) = std::fs::write(&path, out) {
            log::warn!("{}: {e}", path.display());
        }
    }

    pub fn run_preload_file(&mut self, name: &str) {
        let Some(path) = self.preload.path(name) else { return };
        let Ok(src) = std::fs::read(&path) else { return };
        let src = String::from_utf8_lossy(&src).into_owned();
        let common = self.chain.read_string("scripts\\common.j").unwrap_or_default();
        let module = match w3_jass::compile([("common.j", &*common), (name, &*src)]) {
            Ok(m) => m,
            Err(e) => {
                log::warn!("{}: {e}", path.display());
                return;
            }
        };
        let mut vm = w3_jass::Vm::new(module);
        if let Err(e) = vm.init_globals(self) {
            log::warn!("{}: {e}", path.display());
            return;
        }
        if let Some(f) = vm.module.function("PreloadFiles") {
            vm.call(self, f, &[]);
        }
    }
}
