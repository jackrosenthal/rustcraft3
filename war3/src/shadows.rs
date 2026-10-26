//! Shadow images (ReplaceableTextures\Shadows).  A structure's (UnitUI buildingShadow)
//! or destructable's (DestructableData shadow) image is stamped into the terrain's
//! shadow cells, a pixel a 32-unit cell, where its alpha is over half; the pixel with
//! red in it marks where the owner stands (game.dll FUN_6f74c940 and FUN_6f731070).
//! A unit's (unitShadow) or an item's is drawn under it on the ground, shadowW by
//! shadowH with the unit at shadowX, shadowY from its lower left, in UI\MiscData.txt
//! [FogOfWar] ShadowImageColor.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use w3_game::types::Obj;
use w3_game::world::World;

/// A stamped shadow: its size, the cell its owner stands on, and which pixels cast
/// shadow (rows from the bottom).
struct Stamp {
    w: i32,
    h: i32,
    anchor: (i32, i32),
    mask: Vec<bool>,
}

#[derive(Default)]
pub struct Shadows {
    stamps: HashMap<String, Option<Stamp>>,
    /// What was last stamped, to restamp only on a change.
    signature: u64,
}

fn load_stamp(chain: &w3_mpq::Chain, name: &str) -> Option<Stamp> {
    let path = format!("ReplaceableTextures\\Shadows\\{name}.blp");
    let data = chain.read(&path).ok()?;
    let img = w3_formats::blp::decode_texture(&path, &data).ok()?.into_iter().next()?;
    let (w, h) = (img.width as i32, img.height as i32);
    let mut mask = vec![false; (w * h) as usize];
    let mut anchor = (0, 0);
    for yb in 0..h {
        for x in 0..w {
            let p = (((h - 1 - yb) * w + x) * 4) as usize;
            if img.rgba[p] != 0 {
                anchor = (x, yb);
            }
            mask[(yb * w + x) as usize] = img.rgba[p + 3] > 0x80;
        }
    }
    Some(Stamp { w, h, anchor, mask })
}

impl Shadows {
    /// The structures and destructables casting shadows: their image and cell.
    /// Each caster's stamp name, cell, and whether it is a structure.
    fn casters(world: &World, offset: [f32; 2]) -> Vec<(String, i32, i32, bool)> {
        let cell = |x: f32, y: f32| (((x - offset[0]) / 32.0) as i32, ((y - offset[1]) / 32.0) as i32);
        let mut out = Vec::new();
        for &h in &world.unit_order {
            let Some(u) = world.unit(h) else { continue };
            if u.dead || u.hidden || !u.is_building {
                continue;
            }
            let Some(o) = world.objects.units.get(u.type_id) else { continue };
            let name = o.str("buildingShadow");
            if !name.is_empty() && name != "_" {
                let (cx, cy) = cell(u.x, u.y);
                out.push((name.to_string(), cx, cy, true));
            }
        }
        for (_, o) in world.handles.iter() {
            let Obj::Destructable(d) = o else { continue };
            if d.life <= 0.0 || !d.visible {
                continue;
            }
            let Some(od) = world.objects.destructables.get(d.type_id) else { continue };
            let name = od.str("shadow");
            if !name.is_empty() && name != "_" {
                let (cx, cy) = cell(d.x, d.y);
                out.push((name.to_string(), cx, cy, false));
            }
        }
        out
    }

    /// What could cast a shadow: each structure's and destructable's type and position
    /// while it stands.
    fn signature(world: &World) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        for &h in &world.unit_order {
            if let Some(u) = world.unit(h).filter(|u| u.is_building && !u.dead && !u.hidden) {
                (u.type_id, u.x.to_bits(), u.y.to_bits()).hash(&mut hasher);
            }
        }
        for (h, o) in world.handles.iter() {
            if let Obj::Destructable(d) = o {
                (h, d.type_id, d.life > 0.0 && d.visible).hash(&mut hasher);
            }
        }
        hasher.finish()
    }

    /// Restamps the terrain's shadow cells when the casters changed.
    pub fn update(&mut self, world: &World, terrain: &mut w3_render::terrain::TerrainRenderer, device: &wgpu::Device, queue: &wgpu::Queue) {
        let signature = Self::signature(world);
        if signature == self.signature {
            return;
        }
        self.signature = signature;
        let casters = Self::casters(world, terrain.offset);
        let (sw, sh) = terrain.shadow_size();
        let (sw, sh) = (sw as i32, sh as i32);
        let mut mask = terrain.shadow_base.clone();
        let mut ground = terrain.shadow_base.clone();
        for (name, cx, cy, structure) in &casters {
            let stamp = self.stamps.entry(name.clone()).or_insert_with(|| load_stamp(&world.chain, name));
            let Some(s) = stamp else { continue };
            let (x0, y0) = (cx - s.anchor.0, cy - s.anchor.1);
            for yb in 0..s.h {
                for x in 0..s.w {
                    let (gx, gy) = (x0 + x, y0 + yb);
                    if s.mask[(yb * s.w + x) as usize] && (0..sw).contains(&gx) && (0..sh).contains(&gy) {
                        mask[(gy * sw + gx) as usize] = 255;
                        if !structure {
                            ground[(gy * sw + gx) as usize] = 255;
                        }
                    }
                }
            }
        }
        terrain.set_shadows(device, queue, &mask);
        terrain.splat_shadows = ground;
    }
}

/// UI\MiscData.txt [FogOfWar] ShadowImageColor (alpha, red, green, blue) as RGBA.
pub fn image_color(world: &World) -> [f32; 4] {
    let v: Vec<f32> = world.misc.get_list("FogOfWar", "ShadowImageColor").iter().filter_map(|x| x.trim().parse().ok()).collect();
    if v.len() == 4 { [v[1] / 255.0, v[2] / 255.0, v[3] / 255.0, v[0] / 255.0] } else { [0.0, 0.0, 0.0, 170.0 / 255.0] }
}
