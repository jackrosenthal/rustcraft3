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

/// A unit that struck an enemy which could not see it, shown to the enemy's owner for
/// a moment (FUN_6f67fc70): where it stood, how far around, and until when.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct FogFlash {
    pub unit: u32,
    pub players: u16,
    pub x: f32,
    pub y: f32,
    pub radius: f32,
    pub until: f32,
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
    /// Cliff level of each terrain corner, (width + 1) by (height + 1).
    pub corners: Vec<u8>,
    /// Each cell's height for line of sight: its tile's highest corner level times 128,
    /// raised by the occluders standing in it (CFogOfWarMap's height grid,
    /// FUN_6f24fff0, FUN_6f2517f0).
    pub sight_height: Vec<i16>,
    /// The occluders raising `sight_height`: handle, cell, the cliff level under it
    /// times 128, and the height it set.
    pub occluders: Vec<(u32, usize, i16, i16)>,
    pub timer: f32,
}

impl FogMap {
    /// `corner_level(x, y)`: the cliff level of terrain corner (x, y).
    pub fn new(bounds: Rect, corner_level: impl Fn(usize, usize) -> u8) -> FogMap {
        let width = ((bounds.max_x - bounds.min_x) / FOG_CELL).ceil().max(1.0) as usize;
        let height = ((bounds.max_y - bounds.min_y) / FOG_CELL).ceil().max(1.0) as usize;
        let origin = (bounds.min_x, bounds.min_y);
        let mut corners = vec![0; (width + 1) * (height + 1)];
        for y in 0..=height {
            for x in 0..=width {
                corners[y * (width + 1) + x] = corner_level(x, y);
            }
        }
        // A cell's height is the highest level among its sixteen pathing squares, each
        // taking the level of its nearest corner: the highest of its tile's corners.
        let mut sight_height = vec![0; width * height];
        for y in 0..height {
            for x in 0..width {
                let c = |dx: usize, dy: usize| corners[(y + dy) * (width + 1) + x + dx];
                sight_height[y * width + x] = c(0, 0).max(c(1, 0)).max(c(0, 1)).max(c(1, 1)) as i16 * 128;
            }
        }
        FogMap {
            width,
            height,
            origin,
            explored: vec![vec![false; width * height]; PLAYER_COUNT],
            visible: vec![vec![false; width * height]; PLAYER_COUNT],
            corners,
            sight_height,
            occluders: Vec::new(),
            timer: 0.0,
        }
    }

    /// The cliff level at a point: that of the corner nearest its pathing square
    /// (FUN_6f78bc90, FUN_6f750100).
    pub fn level_at(&self, x: f32, y: f32) -> u8 {
        let corner = |v: f32, o: f32, n: usize| ((((v - o) / 32.0).floor() as i32 + 2).div_euclid(4)).clamp(0, n as i32) as usize;
        let (cx, cy) = (corner(x, self.origin.0, self.width), corner(y, self.origin.1, self.height));
        self.corners.get(cy * (self.width + 1) + cx).copied().unwrap_or(0)
    }

    pub fn cell(&self, x: f32, y: f32) -> Option<usize> {
        let cx = ((x - self.origin.0) / FOG_CELL).floor();
        let cy = ((y - self.origin.1) / FOG_CELL).floor();
        if cx < 0.0 || cy < 0.0 || cx as usize >= self.width || cy as usize >= self.height {
            return None;
        }
        Some(cy as usize * self.width + cx as usize)
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
        // Sight goes between day's and night's over three seconds (measured with a
        // timer: a footman, 1400 by day and 800 by night, and a gryphon rider, 1600 and
        // 900, reach night's sight 2.75 to 3 seconds after midnight comes, losing fog
        // cells evenly on the way, and the footman regains day's likewise at dawn).
        let day = (6.0..18.0).contains(&self.time_of_day);
        let toward = if day { 0.0 } else { 1.0 };
        self.night_sight = if self.time <= dt {
            toward
        } else if self.night_sight < toward { (self.night_sight + dt / 3.0).min(toward) } else { (self.night_sight - dt / 3.0).max(toward) };
        let Some(fog) = self.fog.as_mut() else { return };
        fog.timer -= dt;
        if fog.timer > 0.0 {
            return;
        }
        fog.timer = 0.1;
        let night = self.night_sight;
        // Each unit's sight: position, radius, cliff level, and whether it sees over cliffs.
        let mut sources: Vec<(u8, f32, f32, f32, u8, bool, u16)> = Vec::new();
        for &h in &self.unit_order {
            let Some(u) = self.unit(h) else { continue };
            let Some(o) = self.objects.units.get(u.type_id) else { continue };
            // A dead unit sees on while its death plays (measured: a footman, death
            // 3.04, still shows its owner 1100 away 2.5 seconds after dying and not at
            // 3.0; a grunt, 1.75, at 1.6 and not at 1.9).
            let dying = u.dead && !u.extra.exploded && self.time - u.death_time < o.real("death");
            if (u.dead && !dying) || u.hidden && u.inside == 0 {
                continue;
            }
            let r = if self.ultravision(u) { o.real("sight") } else { o.real("sight") + (o.real("nsight") - o.real("sight")) * night };
            if r <= 0.0 {
                continue;
            }
            let flying = o.str("movetp").eq_ignore_ascii_case("fly");
            let fog = self.fog.as_ref().unwrap();
            let lvl = fog.level_at(u.x, u.y);
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
        // blockers) raise their cell to the cliff level under them times 128 plus that
        // height, keeping what is higher; one that falls or loses its height sets the
        // cell back to its own cliff level when it was the highest there, and
        // SetDestructableOccluderHeight sets the cell outright (FUN_6f2517f0).
        let mut standing: Vec<(u32, f32, bool)> = Vec::new();
        let mut occludes: crate::HashMap<crate::objects::Id, f32> = crate::HashMap::default();
        for &h in &self.destructable_order {
            let Some(Obj::Destructable(d)) = self.handles.get(h) else { continue };
            if d.life <= 0.0 {
                continue;
            }
            let occ = match d.occluder_height {
                Some(v) => v,
                None => *occludes.entry(d.type_id).or_insert_with(|| self.objects.destructables.get(d.type_id).map_or(0.0, |od| od.real("occH"))),
            };
            if occ > 0.0 {
                standing.push((h, occ, d.occluder_height.is_some()));
            }
        }
        {
            let fog = self.fog.as_mut().unwrap();
            let mut applied = std::mem::take(&mut fog.occluders);
            applied.retain(|&(h, c, base, value)| {
                let keep = standing.iter().any(|&(s, occ, _)| s == h && base + occ as i32 as i16 == value);
                if !keep && fog.sight_height[c] <= value {
                    fog.sight_height[c] = base;
                }
                keep
            });
            for &(h, occ, set) in &standing {
                if applied.iter().any(|&(a, ..)| a == h) {
                    continue;
                }
                let Some(Obj::Destructable(d)) = self.handles.get(h) else { continue };
                let Some(c) = fog.cell(d.x, d.y) else { continue };
                let base = fog.level_at(d.x, d.y) as i16 * 128;
                let value = base + occ as i32 as i16;
                fog.sight_height[c] = if set { value } else { fog.sight_height[c].max(value) };
                applied.push((h, c, base, value));
            }
            fog.occluders = applied;
        }
        let fog = self.fog.as_mut().unwrap();
        for v in fog.visible.iter_mut() {
            v.fill(false);
        }
        let w = fog.width;
        let mut flags: Vec<u8> = Vec::new();
        for (owner, x, y, r, lvl, flying, shared) in sources {
            let Some(from) = fog.cell(x, y) else { continue };
            let (ox, oy) = ((from % w) as i32, (from / w) as i32);
            // The sight in whole cells, rounded (measured: a peasant, 800 or 6.25 cells,
            // sees the cell 6 over and 2 up and not 6 and 3, nor 7 and 0; a footman,
            // 1400, sees footmen 1499 away east and not 1478 away northeast; a gryphon
            // rider, 1600, sees them 1720 away).
            let span = (r / FOG_CELL + 0.5).floor() as usize;
            let table = sight_table(span);
            // A ground viewer's eye is at its cliff level times 128 (FUN_6f252340).  The
            // cells about it are seen; each further cell is seen unless either of its
            // two cells toward the viewer is shadowed or stands above the eye, and is
            // shadowed itself on the same terms.  Cells off the map shadow nothing but
            // what lies past them, which is off the map too.
            let eye = lvl as i32 * 128;
            flags.clear();
            flags.resize(table.len(), 0);
            let mut cells = Vec::new();
            for (k, n) in table.iter().enumerate() {
                let (cx, cy) = (ox + n.dx as i32, oy + n.dy as i32);
                let inside = cx >= 0 && cy >= 0 && (cx as usize) < fog.width && (cy as usize) < fog.height;
                let inherited = if k < 9 { 0 } else { flags[n.a as usize] | flags[n.b as usize] };
                if inherited & 2 != 0 || !inside {
                    flags[k] = inherited | 2;
                    continue;
                }
                let c = cy as usize * w + cx as usize;
                if flying || inherited & 1 == 0 {
                    cells.push(c);
                }
                flags[k] = inherited | (eye < fog.sight_height[c] as i32) as u8;
            }
            for c in cells {
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
        // Attackers seen for striking from the fog.
        let now = self.time;
        self.fog_flashes.retain(|f| f.until > now);
        if let Some(fog) = self.fog.as_mut() {
            for f in &self.fog_flashes {
                let mut cells = Vec::new();
                fog.circle(f.x, f.y, f.radius, |cx, cy| cells.push(cy * w + cx));
                for p in (0..PLAYER_COUNT).filter(|&p| f.players & (1 << p) != 0) {
                    for &c in &cells {
                        fog.visible[p][c] = true;
                        fog.explored[p][c] = true;
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

    /// Shows an attacker to the owner of the enemy it struck, around where it stands, for
    /// FogFlashTime: at least FoggedAttackRevealRadius around it, or `radius` when more
    /// (FUN_6f67fde0, FUN_6f67fc70).
    pub fn reveal_attacker(&mut self, attacker: u32, victim: u32, radius: f32) {
        let (Some(a), Some(v)) = (self.unit(attacker), self.unit(victim)) else { return };
        if a.owner == v.owner || v.dead || v.hidden || crate::natives::players::is_ally(self, v.owner, a.owner) {
            return;
        }
        let (x, y, player) = (a.x, a.y, v.owner as usize % PLAYER_COUNT);
        let radius = self.misc_f32("FoggedAttackRevealRadius", 200.0).max(radius);
        let until = self.time + self.misc_f32("FogFlashTime", 3.0);
        match self.fog_flashes.iter_mut().find(|f| f.unit == attacker) {
            Some(f) => {
                f.players |= 1 << player;
                (f.x, f.y, f.radius, f.until) = (x, y, radius, until);
            }
            None => self.fog_flashes.push(FogFlash { unit: attacker, players: 1 << player, x, y, radius, until }),
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

    /// The local player's fog as bytes for rendering, two a cell: its level (255
    /// visible, 0 masked, the fogged level in between) and 255 where the cell lies in the
    /// world's boundary (the terrain's boundary flag) while EnableWorldFogBoundary holds,
    /// which the renderer shades with the boundary colors (FUN_6f744950).
    pub fn fog_image(&self, player: u8, fogged: u8) -> Option<(usize, usize, Vec<u8>)> {
        let fog = self.fog.as_ref()?;
        let p = player as usize % PLAYER_COUNT;
        let boundary = |c: usize| {
            if !self.fog_boundary {
                return false;
            }
            let Some(t) = self.terrain_heights.as_ref() else { return false };
            let (x, y) = (fog.origin.0 + ((c % fog.width) as f32 + 0.5) * FOG_CELL, fog.origin.1 + ((c / fog.width) as f32 + 0.5) * FOG_CELL);
            let (tx, ty) = (((x - t.offset[0]) / 128.0).floor(), ((y - t.offset[1]) / 128.0).floor());
            if tx < 0.0 || ty < 0.0 || tx as u32 >= t.width || ty as u32 >= t.height {
                return true;
            }
            t.boundary.get((ty as u32 * t.width + tx as u32) as usize).copied().unwrap_or(false)
        };
        let mut img = Vec::with_capacity(fog.width * fog.height * 2);
        for c in 0..fog.width * fog.height {
            let level = if !self.fog_enabled || fog.visible[p][c] {
                255
            } else if fog.explored[p][c] || !self.fog_mask_enabled {
                fogged
            } else {
                0
            };
            img.push(level);
            img.push(if boundary(c) { 255 } else { 0 });
        }
        Some((fog.width, fog.height, img))
    }
}

/// One cell of a sight mask: its offset from the viewer's cell and the two mask
/// entries nearer the viewer whose shadows it takes.
#[derive(Clone, Copy)]
struct SightNode {
    dx: i16,
    dy: i16,
    a: u16,
    b: u16,
}

/// The sight mask of a radius in cells, as CFogMask builds it (FUN_6f24de40): the
/// cells about the viewer, then ring by ring outward the cells of eight octants out to
/// the edge of a midpoint circle (FUN_6f250e00, FUN_6f24fad0).  Each octant runs along
/// an axis, a cell `s` out and `j` aside taking its shadows from the cells `s - 1` out
/// and `j` and `j - 1` aside.  The second four octants start a cell aside of the axis
/// and stop a cell short of the first four, which leaves the masks lopsided as the
/// original's are (measured: a peasant sees one cell further west than east 5 cells
/// north, and further east than west 5 cells south).
fn sight_table(r: usize) -> std::sync::Arc<Vec<SightNode>> {
    static TABLES: std::sync::Mutex<Vec<Option<std::sync::Arc<Vec<SightNode>>>>> = std::sync::Mutex::new(Vec::new());
    let mut tables = TABLES.lock().unwrap();
    if tables.len() <= r {
        tables.resize(r + 1, None);
    }
    tables[r].get_or_insert_with(|| std::sync::Arc::new(build_sight_table(r))).clone()
}

/// The circle's half-width on each row from the top, 2r + 1 rows.
fn circle_spans(r: i32) -> Vec<i32> {
    let mut widths = Vec::new();
    let (mut c, mut x, mut d, mut e, mut y) = (6, 0, 3 - 2 * r, 10 - 4 * r, r);
    loop {
        if d < 0 {
            d += c;
            e += 4;
        } else {
            widths.push(x);
            y -= 1;
            d += e;
            e += 8;
        }
        c += 4;
        x += 1;
        if x > y {
            break;
        }
    }
    let mut spans = vec![0; (2 * r + 1) as usize];
    spans[r as usize] = r;
    let (mut lo, mut hi, mut next, mut p) = (r - 1, r + 1, 1, r);
    for (k, &u) in widths.iter().enumerate() {
        spans[k] = u;
        spans[(2 * r) as usize - k] = u;
        while next <= u {
            spans[lo as usize] = p;
            spans[hi as usize] = p;
            lo -= 1;
            hi += 1;
            next += 1;
        }
        p -= 1;
    }
    spans
}

fn build_sight_table(r: usize) -> Vec<SightNode> {
    const OCTANTS: [((i16, i16), (i16, i16)); 8] = [
        ((0, -1), (1, 0)),
        ((0, 1), (-1, 0)),
        ((1, 0), (0, 1)),
        ((-1, 0), (0, -1)),
        ((0, -1), (-1, 0)),
        ((0, 1), (1, 0)),
        ((1, 0), (0, -1)),
        ((-1, 0), (0, 1)),
    ];
    let r = r.max(1) as i32;
    let spans = circle_spans(r);
    let size = 2 * r + 1;
    let mut index = vec![u16::MAX; (size * size) as usize];
    let at = |dx: i32, dy: i32| ((dy + r) * size + dx + r) as usize;
    let mut nodes = Vec::new();
    for dy in -1..=1 {
        for dx in -1..=1 {
            index[at(dx, dy)] = nodes.len() as u16;
            nodes.push(SightNode { dx: dx as i16, dy: dy as i16, a: 0, b: 0 });
        }
    }
    for s in 1..=r {
        let w = spans[(r - s) as usize];
        for (o, &((ax, ay), (px, py))) in OCTANTS.iter().enumerate() {
            let (ax, ay, px, py) = (ax as i32, ay as i32, px as i32, py as i32);
            let (first, last) = if o > 3 { (1, s.min(w) - 1) } else { (0, s.min(w)) };
            for j in first..=last {
                let (cx, cy) = (ax * s + px * j, ay * s + py * j);
                if index[at(cx, cy)] != u16::MAX {
                    continue;
                }
                let (ix, iy) = (cx - ax, cy - ay);
                let (a, b) = if j == 0 {
                    ((ix, iy), (ix, iy))
                } else if j == s {
                    ((ix - px, iy - py), (ix - px, iy - py))
                } else {
                    ((ix, iy), (ix - px, iy - py))
                };
                index[at(cx, cy)] = nodes.len() as u16;
                nodes.push(SightNode { dx: cx as i16, dy: cy as i16, a: index[at(a.0, a.1)], b: index[at(b.0, b.1)] });
            }
        }
    }
    nodes
}
