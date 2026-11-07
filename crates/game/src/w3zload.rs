//! What a saved game in the original's format holds beyond the simulation's objects,
//! put back as it loads (`Game::restore_original` calls `restore_presentation`): so far
//! the local player's camera.

use w3_formats::w3z::{First, SaveGame};

use crate::Game;

impl Game {
    /// The save's presentation state over the map's start's: the game camera's target
    /// (the camera's second table position `` `pos `` at slot 1092, its target in
    /// pathing cells, `World::camera_records`; read, the hero map's at 300 seconds,
    /// (119.965454, 225.1696), its eye at 1089 28.8 cells behind).  Provisional: the
    /// camera's fields (its distance, angles, and height) stay the map's start's.
    pub(crate) fn restore_presentation(&mut self, s: &SaveGame) -> anyhow::Result<()> {
        let Some(m) = s.objects.as_ref() else { return Ok(()) };
        let objects = m.objects()?;
        let target = objects[1].iter().find(|o| o.entry.tag == 0x8000_0000 | 1092 && &o.entry.kind.to_be_bytes() == crate::codes::class::POSITION);
        if let (Some(Ok(First::Pos { values, .. })), Some(origin)) = (target.map(|o| o.first()), self.world.pathing.as_ref().map(|p| p.origin)) {
            let (x, y) = (origin[0] + values[0] * crate::pathing::CELL, origin[1] + values[1] * crate::pathing::CELL);
            self.world.camera.pan_to(x, y, 0.0);
        }
        Ok(())
    }
}
