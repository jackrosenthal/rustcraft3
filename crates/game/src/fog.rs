//! Fog of war: each player's explored and visible cells on a 128-unit grid (the cell size
//! game.dll's CFogOfWarMap uses), from unit sight, shared vision, and fog modifiers.

use crate::types::*;
use crate::world::World;

/// The fog grid cell size (game.dll initializes it to 128.0 next to its half, 64.0).
pub const FOG_CELL: f32 = 128.0;

pub const FOG_MASKED: i32 = 1;
pub const FOG_FOGGED: i32 = 2;
pub const FOG_VISIBLE: i32 = 4;

/// The shared-vision alliance flag (ALLIANCE_SHARED_VISION).
pub(crate) const ALLIANCE_SHARED_VISION: usize = 5;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum FogShape {
    Rect(Rect),
    Circle(f32, f32, f32),
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct FogModifier {
    pub player: u8,
    pub state: i32,
    pub shape: FogShape,
    pub shared: bool,
    pub after_units: bool,
    pub active: bool,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct FogMap {
    pub width: usize,
    pub height: usize,
    pub origin: (f32, f32),
    /// Per player: cells ever seen.
    pub explored: Vec<Vec<bool>>,
    /// Per player: cells seen now.
    pub visible: Vec<Vec<bool>>,
    /// Cliff level per cell, for line of sight.
    pub level: Vec<u8>,
    pub timer: f32,
}

impl FogMap {
    pub fn new(bounds: Rect, levels: impl Fn(f32, f32) -> u8) -> FogMap {
        let width = ((bounds.max_x - bounds.min_x) / FOG_CELL).ceil().max(1.0) as usize;
        let height = ((bounds.max_y - bounds.min_y) / FOG_CELL).ceil().max(1.0) as usize;
        let origin = (bounds.min_x, bounds.min_y);
        let mut level = vec![0; width * height];
        for y in 0..height {
            for x in 0..width {
                level[y * width + x] = levels(origin.0 + (x as f32 + 0.5) * FOG_CELL, origin.1 + (y as f32 + 0.5) * FOG_CELL);
            }
        }
        FogMap {
            width,
            height,
            origin,
            explored: vec![vec![false; width * height]; PLAYER_COUNT],
            visible: vec![vec![false; width * height]; PLAYER_COUNT],
            level,
            timer: 0.0,
        }
    }

    pub fn cell(&self, x: f32, y: f32) -> Option<usize> {
        let cx = ((x - self.origin.0) / FOG_CELL).floor();
        let cy = ((y - self.origin.1) / FOG_CELL).floor();
        if cx < 0.0 || cy < 0.0 || cx as usize >= self.width || cy as usize >= self.height {
            return None;
        }
        Some(cy as usize * self.width + cx as usize)
    }

    /// Whether an occluded cell lies strictly between two cells (along the line between
    /// their centers, sampled every half cell).
    fn blocked(&self, from: usize, to: usize, occluded: &[bool]) -> bool {
        let (fx, fy) = ((from % self.width) as f32, (from / self.width) as f32);
        let (tx, ty) = ((to % self.width) as f32, (to / self.width) as f32);
        let steps = ((tx - fx).abs().max((ty - fy).abs()) * 2.0) as usize;
        for k in 1..steps {
            let t = k as f32 / steps as f32;
            let (cx, cy) = ((fx + (tx - fx) * t).round() as usize, (fy + (ty - fy) * t).round() as usize);
            let c = cy * self.width + cx;
            if c != from && c != to && occluded.get(c).copied().unwrap_or(false) {
                return true;
            }
        }
        false
    }

    /// Cells whose centers lie within `r` of (x, y).
    fn circle(&self, x: f32, y: f32, r: f32, mut f: impl FnMut(usize, usize)) {
        let x0 = (((x - r) - self.origin.0) / FOG_CELL).floor().max(0.0) as usize;
        let y0 = (((y - r) - self.origin.1) / FOG_CELL).floor().max(0.0) as usize;
        let x1 = ((((x + r) - self.origin.0) / FOG_CELL).ceil().max(0.0) as usize).min(self.width);
        let y1 = ((((y + r) - self.origin.1) / FOG_CELL).ceil().max(0.0) as usize).min(self.height);
        for cy in y0..y1 {
            for cx in x0..x1 {
                let px = self.origin.0 + (cx as f32 + 0.5) * FOG_CELL;
                let py = self.origin.1 + (cy as f32 + 0.5) * FOG_CELL;
                if (px - x).hypot(py - y) <= r {
                    f(cx, cy);
                }
            }
        }
    }

    fn rect(&self, r: &Rect, mut f: impl FnMut(usize)) {
        let x0 = ((r.min_x - self.origin.0) / FOG_CELL).floor().max(0.0) as usize;
        let y0 = ((r.min_y - self.origin.1) / FOG_CELL).floor().max(0.0) as usize;
        let x1 = (((r.max_x - self.origin.0) / FOG_CELL).ceil().max(0.0) as usize).min(self.width);
        let y1 = (((r.max_y - self.origin.1) / FOG_CELL).ceil().max(0.0) as usize).min(self.height);
        for cy in y0..y1 {
            for cx in x0..x1 {
                f(cy * self.width + cx);
            }
        }
    }

    /// Applies a fog state to a shape for one player once.
    pub fn set_state(&mut self, player: u8, state: i32, shape: &FogShape) {
        let p = player as usize % PLAYER_COUNT;
        let mut cells = Vec::new();
        match shape {
            FogShape::Rect(r) => self.rect(r, |c| cells.push(c)),
            FogShape::Circle(x, y, r) => {
                let w = self.width;
                self.circle(*x, *y, *r, |cx, cy| cells.push(cy * w + cx));
            }
        }
        for c in cells {
            match state {
                FOG_MASKED => {
                    self.explored[p][c] = false;
                    self.visible[p][c] = false;
                }
                FOG_FOGGED => {
                    self.explored[p][c] = true;
                    self.visible[p][c] = false;
                }
                FOG_VISIBLE => {
                    self.explored[p][c] = true;
                    self.visible[p][c] = true;
                }
                _ => {}
            }
        }
    }
}

impl World {
    /// Recomputes every player's vision.  Vision changes a few times a second.
    pub fn update_fog(&mut self, dt: f32) {
        let Some(fog) = self.fog.as_mut() else { return };
        fog.timer -= dt;
        if fog.timer > 0.0 {
            return;
        }
        fog.timer = 0.1;
        let day = (6.0..18.0).contains(&self.time_of_day);
        // Each unit's sight: position, radius, cliff level, and whether it sees over cliffs.
        let mut sources: Vec<(u8, f32, f32, f32, u8, bool, u16)> = Vec::new();
        for &h in &self.unit_order {
            let Some(u) = self.unit(h) else { continue };
            if u.dead || u.hidden && u.inside == 0 {
                continue;
            }
            let Some(o) = self.objects.units.get(u.type_id) else { continue };
            let r = if day || self.ultravision(u) { o.real("sight") } else { o.real("nsight") };
            if r <= 0.0 {
                continue;
            }
            let flying = o.str("movetp").eq_ignore_ascii_case("fly");
            let fog = self.fog.as_ref().unwrap();
            let lvl = fog.cell(u.x, u.y).map_or(0, |c| fog.level[c]);
            let crippled = self.crippled.get(&u.owner).copied().unwrap_or(0);
            sources.push((u.owner, u.x, u.y, r, lvl, flying, u.extra.shared_vision | crippled));
        }
        // Sentinel owls see from their trees while the trees stand.
        for s in &self.sentinels {
            if self.is_tree_pub(s.tree) {
                if let Some((x, y)) = self.widget_pos(s.tree) {
                    sources.push((s.owner, x, y, s.radius, u8::MAX, true, 0));
                }
            }
        }
        let modifiers: Vec<FogModifier> = self.handles.iter().filter_map(|(_, o)| match o {
            Obj::FogModifier(m) if m.active => Some(m.clone()),
            _ => None,
        }).collect();
        // Who shares vision with whom: player p sees what q sees when q grants p shared vision.
        let mut viewers: Vec<Vec<usize>> = vec![Vec::new(); PLAYER_COUNT];
        for q in 0..PLAYER_COUNT {
            for p in 0..PLAYER_COUNT {
                if p == q || self.players.get(q).is_some_and(|pl| pl.alliances[p][ALLIANCE_SHARED_VISION]) {
                    viewers[q].push(p);
                }
            }
        }
        // Standing destructables with an occlusion height (trees, line of sight
        // blockers) hide what lies beyond them from the ground.
        let (fw, fh) = { let f = self.fog.as_ref().unwrap(); (f.width, f.height) };
        let mut occluded = vec![false; fw * fh];
        for (_, o) in self.handles.iter() {
            let Obj::Destructable(d) = o else { continue };
            if d.life <= 0.0 {
                continue;
            }
            if self.objects.destructables.get(d.type_id).is_some_and(|od| od.real("occH") > 0.0) {
                if let Some(c) = self.fog.as_ref().unwrap().cell(d.x, d.y) {
                    occluded[c] = true;
                }
            }
        }
        let fog = self.fog.as_mut().unwrap();
        for v in fog.visible.iter_mut() {
            v.fill(false);
        }
        let w = fog.width;
        for (owner, x, y, r, lvl, flying, shared) in sources {
            let mut cells = Vec::new();
            fog.circle(x, y, r, |cx, cy| cells.push(cy * w + cx));
            let from = fog.cell(x, y);
            for c in cells {
                // Ground units do not see onto higher cliff levels, nor past occluders.
                if !flying && fog.level[c] > lvl {
                    continue;
                }
                if !flying && from.is_some_and(|f| fog.blocked(f, c, &occluded)) {
                    continue;
                }
                for &p in &viewers[owner as usize % PLAYER_COUNT] {
                    fog.visible[p][c] = true;
                    fog.explored[p][c] = true;
                }
                // UnitShareVision: players this unit shares its sight with.
                for p in (0..PLAYER_COUNT).filter(|&p| shared & (1 << p) != 0) {
                    fog.visible[p][c] = true;
                    fog.explored[p][c] = true;
                }
            }
        }
        for m in modifiers {
            let mut cells = Vec::new();
            match &m.shape {
                FogShape::Rect(r) => fog.rect(r, |c| cells.push(c)),
                FogShape::Circle(x, y, r) => fog.circle(*x, *y, *r, |cx, cy| cells.push(cy * w + cx)),
            }
            let targets: Vec<usize> = if m.shared { viewers[m.player as usize % PLAYER_COUNT].clone() } else { vec![m.player as usize % PLAYER_COUNT] };
            for p in targets {
                for &c in &cells {
                    match m.state {
                        FOG_VISIBLE => {
                            fog.visible[p][c] = true;
                            fog.explored[p][c] = true;
                        }
                        FOG_FOGGED if m.after_units || !fog.visible[p][c] => {
                            fog.visible[p][c] = false;
                            fog.explored[p][c] = true;
                        }
                        FOG_MASKED if m.after_units || !fog.visible[p][c] => {
                            fog.visible[p][c] = false;
                            fog.explored[p][c] = false;
                        }
                        _ => {}
                    }
                }
            }
        }
        // Observers see the whole map.
        let observers: Vec<usize> = (0..PLAYER_COUNT).filter(|&p| self.players.get(p).is_some_and(|pl| pl.get_state(crate::types::player_state::OBSERVER) != 0)).collect();
        if let Some(fog) = self.fog.as_mut() {
            for p in observers {
                fog.visible[p].iter_mut().for_each(|v| *v = true);
                fog.explored[p].iter_mut().for_each(|v| *v = true);
            }
        }
    }

    /// FOG_OF_WAR_VISIBLE, _FOGGED, or _MASKED at a point for a player, honoring
    /// FogEnable and FogMaskEnable.
    pub fn fog_state(&self, x: f32, y: f32, player: u8) -> i32 {
        if !self.fog_enabled {
            return FOG_VISIBLE;
        }
        let Some(fog) = self.fog.as_ref() else { return FOG_VISIBLE };
        let Some(c) = fog.cell(x, y) else { return FOG_MASKED };
        let p = player as usize % PLAYER_COUNT;
        if fog.visible[p][c] {
            FOG_VISIBLE
        } else if fog.explored[p][c] || !self.fog_mask_enabled {
            FOG_FOGGED
        } else {
            FOG_MASKED
        }
    }

    pub fn unit_visible_to(&self, h: u32, player: u8) -> bool {
        let Some(u) = self.unit(h) else { return false };
        if u.owner == player || self.players.get(u.owner as usize).is_some_and(|o| o.alliances[player as usize % PLAYER_COUNT][ALLIANCE_SHARED_VISION]) {
            return true;
        }
        // Observers see every unit.
        if self.players.get(player as usize).is_some_and(|p| p.get_state(crate::types::player_state::OBSERVER) != 0) {
            return true;
        }
        // Invisible units are hidden from others without a detector near.
        if self.is_invisible(h) && !self.detected_by(h, player) {
            return false;
        }
        self.fog_state(u.x, u.y, player) == FOG_VISIBLE
    }

    /// The local player's fog as bytes for rendering: 255 visible, 0 masked, and the
    /// fogged level in between.
    pub fn fog_image(&self, player: u8, fogged: u8) -> Option<(usize, usize, Vec<u8>)> {
        let fog = self.fog.as_ref()?;
        let p = player as usize % PLAYER_COUNT;
        // The world's boundary, outside the playable area, stays black
        // (EnableWorldFogBoundary).
        let pl = self.playable;
        let boundary = |c: usize| {
            let (x, y) = (fog.origin.0 + ((c % fog.width) as f32 + 0.5) * FOG_CELL, fog.origin.1 + ((c / fog.width) as f32 + 0.5) * FOG_CELL);
            self.fog_boundary && pl.max_x > pl.min_x && (x < pl.min_x || x > pl.max_x || y < pl.min_y || y > pl.max_y)
        };
        let img = (0..fog.width * fog.height)
            .map(|c| {
                if boundary(c) {
                    0
                } else if !self.fog_enabled || fog.visible[p][c] {
                    255
                } else if fog.explored[p][c] || !self.fog_mask_enabled {
                    fogged
                } else {
                    0
                }
            })
            .collect();
        Some((fog.width, fog.height, img))
    }
}
