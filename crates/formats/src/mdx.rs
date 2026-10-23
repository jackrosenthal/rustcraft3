//! MDX models (format version 800).

use crate::{Error, Reader};

pub type Vec3 = [f32; 3];
pub type Quat = [f32; 4];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Interp {
    #[default]
    None,
    Linear,
    Hermite,
    Bezier,
}

#[derive(Clone, Debug)]
pub struct Key<T> {
    pub frame: i32,
    pub value: T,
    pub in_tan: T,
    pub out_tan: T,
}

#[derive(Clone, Debug, Default)]
pub struct Track<T> {
    pub interp: Interp,
    pub global_seq: Option<u32>,
    pub keys: Vec<Key<T>>,
}

pub trait TrackValue: Sized + Clone + Default {
    fn read(r: &mut Reader) -> Result<Self, Error>;
}
impl TrackValue for f32 {
    fn read(r: &mut Reader) -> Result<Self, Error> {
        r.f32()
    }
}
impl TrackValue for u32 {
    fn read(r: &mut Reader) -> Result<Self, Error> {
        r.u32()
    }
}
impl TrackValue for Vec3 {
    fn read(r: &mut Reader) -> Result<Self, Error> {
        r.vec3()
    }
}
impl TrackValue for Quat {
    fn read(r: &mut Reader) -> Result<Self, Error> {
        r.vec4()
    }
}

impl<T: TrackValue> Track<T> {
    fn read(r: &mut Reader) -> Result<Self, Error> {
        let n = r.u32()? as usize;
        let interp = match r.u32()? {
            1 => Interp::Linear,
            2 => Interp::Hermite,
            3 => Interp::Bezier,
            _ => Interp::None,
        };
        let gs = r.i32()?;
        let mut keys = Vec::with_capacity(n.min(65536));
        for _ in 0..n {
            let frame = r.i32()?;
            let value = T::read(r)?;
            let (in_tan, out_tan) = if matches!(interp, Interp::Hermite | Interp::Bezier) {
                (T::read(r)?, T::read(r)?)
            } else {
                (T::default(), T::default())
            };
            keys.push(Key { frame, value, in_tan, out_tan });
        }
        Ok(Track {
            interp,
            global_seq: (gs >= 0).then_some(gs as u32),
            keys,
        })
    }
}

/// Named tracks found in the trailing part of an object.
#[derive(Clone, Debug, Default)]
pub struct Tracks {
    pub f32s: Vec<([u8; 4], Track<f32>)>,
    pub u32s: Vec<([u8; 4], Track<u32>)>,
    pub vec3s: Vec<([u8; 4], Track<Vec3>)>,
    pub quats: Vec<([u8; 4], Track<Quat>)>,
}

impl Tracks {
    fn read(r: &mut Reader) -> Result<Self, Error> {
        let mut t = Tracks::default();
        while r.remaining() >= 16 {
            let tag = r.tag()?;
            match &tag {
                b"KGRT" | b"KTAR" => t.quats.push((tag, Track::read(r)?)),
                b"KGTR" | b"KGSC" | b"KTAT" | b"KTAS" | b"KGAC" | b"KLAC" | b"KLBC" | b"KRCO"
                | b"KCTR" | b"KTTR" => t.vec3s.push((tag, Track::read(r)?)),
                b"KMTF" | b"KRTX" => t.u32s.push((tag, Track::read(r)?)),
                b"KMTA" | b"KGAO" | b"KLAS" | b"KLAE" | b"KLAI" | b"KLBI" | b"KLAV" | b"KATV"
                | b"KPEE" | b"KPEG" | b"KPLN" | b"KPLT" | b"KPEL" | b"KPES" | b"KPEV" | b"KP2S"
                | b"KP2R" | b"KP2L" | b"KP2G" | b"KP2E" | b"KP2N" | b"KP2W" | b"KP2V" | b"KRHA"
                | b"KRHB" | b"KRAL" | b"KRVS" | b"KCRL" => t.f32s.push((tag, Track::read(r)?)),
                _ => return Err(Error::Format("unknown MDX track tag")),
            }
        }
        Ok(t)
    }
    pub fn f32(&self, tag: &[u8; 4]) -> Option<&Track<f32>> {
        self.f32s.iter().find(|(t, _)| t == tag).map(|(_, v)| v)
    }
    pub fn u32(&self, tag: &[u8; 4]) -> Option<&Track<u32>> {
        self.u32s.iter().find(|(t, _)| t == tag).map(|(_, v)| v)
    }
    pub fn vec3(&self, tag: &[u8; 4]) -> Option<&Track<Vec3>> {
        self.vec3s.iter().find(|(t, _)| t == tag).map(|(_, v)| v)
    }
    pub fn quat(&self, tag: &[u8; 4]) -> Option<&Track<Quat>> {
        self.quats.iter().find(|(t, _)| t == tag).map(|(_, v)| v)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Extent {
    pub radius: f32,
    pub min: Vec3,
    pub max: Vec3,
}

fn extent(r: &mut Reader) -> Result<Extent, Error> {
    Ok(Extent {
        radius: r.f32()?,
        min: r.vec3()?,
        max: r.vec3()?,
    })
}

#[derive(Clone, Debug)]
pub struct Sequence {
    pub name: String,
    pub start: u32,
    pub end: u32,
    pub move_speed: f32,
    pub non_looping: bool,
    pub rarity: f32,
    pub extent: Extent,
}

#[derive(Clone, Debug)]
pub struct Texture {
    pub replaceable_id: u32,
    pub path: String,
    pub flags: u32,
}

pub mod filter {
    pub const NONE: u32 = 0;
    pub const TRANSPARENT: u32 = 1;
    pub const BLEND: u32 = 2;
    pub const ADDITIVE: u32 = 3;
    pub const ADD_ALPHA: u32 = 4;
    pub const MODULATE: u32 = 5;
    pub const MODULATE_2X: u32 = 6;
}

pub mod shading {
    pub const UNSHADED: u32 = 0x1;
    pub const SPHERE_ENV_MAP: u32 = 0x2;
    pub const TWO_SIDED: u32 = 0x10;
    pub const UNFOGGED: u32 = 0x20;
    pub const NO_DEPTH_TEST: u32 = 0x40;
    pub const NO_DEPTH_SET: u32 = 0x80;
}

#[derive(Clone, Debug)]
pub struct Layer {
    pub filter_mode: u32,
    pub shading: u32,
    pub texture_id: u32,
    pub texture_anim_id: Option<u32>,
    pub coord_id: u32,
    pub alpha: f32,
    pub tracks: Tracks,
}

#[derive(Clone, Debug)]
pub struct Material {
    pub priority_plane: i32,
    pub flags: u32,
    pub layers: Vec<Layer>,
}

#[derive(Clone, Debug, Default)]
pub struct Geoset {
    pub vertices: Vec<Vec3>,
    pub normals: Vec<Vec3>,
    pub faces: Vec<u16>,
    pub vertex_groups: Vec<u8>,
    pub group_sizes: Vec<u32>,
    pub matrix_indices: Vec<u32>,
    pub material_id: u32,
    pub selection_group: u32,
    pub unselectable: bool,
    pub extent: Extent,
    pub sequence_extents: Vec<Extent>,
    pub uvs: Vec<Vec<[f32; 2]>>,
}

#[derive(Clone, Debug)]
pub struct GeosetAnim {
    pub alpha: f32,
    pub flags: u32,
    pub color: Vec3,
    pub geoset_id: u32,
    pub tracks: Tracks,
}

pub mod node_flags {
    pub const DONT_INHERIT_TRANSLATION: u32 = 0x1;
    pub const DONT_INHERIT_ROTATION: u32 = 0x2;
    pub const DONT_INHERIT_SCALING: u32 = 0x4;
    pub const BILLBOARDED: u32 = 0x8;
    pub const BILLBOARDED_LOCK_X: u32 = 0x10;
    pub const BILLBOARDED_LOCK_Y: u32 = 0x20;
    pub const BILLBOARDED_LOCK_Z: u32 = 0x40;
    pub const CAMERA_ANCHORED: u32 = 0x80;
}

#[derive(Clone, Debug)]
pub struct Node {
    pub name: String,
    pub object_id: u32,
    pub parent: Option<u32>,
    pub flags: u32,
    pub tracks: Tracks,
}

fn node(r: &mut Reader) -> Result<Node, Error> {
    let size = r.u32()? as usize;
    let mut s = r.sub(size.checked_sub(4).ok_or(Error::Format("node size"))?)?;
    let name = s.fixed_str(80)?;
    let object_id = s.u32()?;
    let parent = s.u32()?;
    let flags = s.u32()?;
    let tracks = Tracks::read(&mut s)?;
    Ok(Node {
        name,
        object_id,
        parent: (parent != u32::MAX).then_some(parent),
        flags,
        tracks,
    })
}

#[derive(Clone, Debug)]
pub struct Bone {
    pub node: Node,
    pub geoset_id: Option<u32>,
    pub geoset_anim_id: Option<u32>,
}

#[derive(Clone, Debug)]
pub struct Light {
    pub node: Node,
    pub kind: u32,
    pub attenuation: [f32; 2],
    pub color: Vec3,
    pub intensity: f32,
    pub ambient_color: Vec3,
    pub ambient_intensity: f32,
    pub tracks: Tracks,
}

#[derive(Clone, Debug)]
pub struct Attachment {
    pub node: Node,
    pub path: String,
    pub attachment_id: u32,
    pub tracks: Tracks,
}

#[derive(Clone, Debug)]
pub struct ParticleEmitter {
    pub node: Node,
    pub emission_rate: f32,
    pub gravity: f32,
    pub longitude: f32,
    pub latitude: f32,
    pub path: String,
    pub lifespan: f32,
    pub speed: f32,
    pub tracks: Tracks,
}

#[derive(Clone, Debug)]
pub struct ParticleEmitter2 {
    pub node: Node,
    pub speed: f32,
    pub variation: f32,
    pub latitude: f32,
    pub gravity: f32,
    pub lifespan: f32,
    pub emission_rate: f32,
    pub width: f32,
    pub length: f32,
    pub filter_mode: u32,
    pub rows: u32,
    pub columns: u32,
    pub head_or_tail: u32,
    pub tail_length: f32,
    pub time: f32,
    pub segment_color: [Vec3; 3],
    pub segment_alpha: [u8; 3],
    pub segment_scaling: Vec3,
    pub head_intervals: [[u32; 3]; 2],
    pub tail_intervals: [[u32; 3]; 2],
    pub texture_id: u32,
    pub squirt: bool,
    pub priority_plane: i32,
    pub replaceable_id: u32,
    pub tracks: Tracks,
}

#[derive(Clone, Debug)]
pub struct RibbonEmitter {
    pub node: Node,
    pub height_above: f32,
    pub height_below: f32,
    pub alpha: f32,
    pub color: Vec3,
    pub lifespan: f32,
    pub texture_slot: u32,
    pub emission_rate: u32,
    pub rows: u32,
    pub columns: u32,
    pub material_id: u32,
    pub gravity: f32,
    pub tracks: Tracks,
}

#[derive(Clone, Debug)]
pub struct EventObject {
    pub node: Node,
    pub global_seq: Option<u32>,
    pub frames: Vec<u32>,
}

#[derive(Clone, Debug)]
pub struct Camera {
    pub name: String,
    pub position: Vec3,
    pub fov: f32,
    pub far_clip: f32,
    pub near_clip: f32,
    pub target: Vec3,
    pub tracks: Tracks,
}

#[derive(Clone, Debug)]
pub enum Shape {
    Box(Vec3, Vec3),
    Plane(Vec3, Vec3),
    Sphere(Vec3, f32),
    Cylinder(Vec3, Vec3, f32),
}

#[derive(Clone, Debug)]
pub struct CollisionShape {
    pub node: Node,
    pub shape: Shape,
}

#[derive(Clone, Debug, Default)]
pub struct Model {
    pub version: u32,
    pub name: String,
    pub anim_file: String,
    pub extent: Extent,
    pub blend_time: u32,
    pub sequences: Vec<Sequence>,
    pub global_sequences: Vec<u32>,
    pub textures: Vec<Texture>,
    pub materials: Vec<Material>,
    pub texture_anims: Vec<Tracks>,
    pub geosets: Vec<Geoset>,
    pub geoset_anims: Vec<GeosetAnim>,
    pub bones: Vec<Bone>,
    pub lights: Vec<Light>,
    pub helpers: Vec<Node>,
    pub attachments: Vec<Attachment>,
    pub pivots: Vec<Vec3>,
    pub particle_emitters: Vec<ParticleEmitter>,
    pub particle_emitters2: Vec<ParticleEmitter2>,
    pub ribbon_emitters: Vec<RibbonEmitter>,
    pub event_objects: Vec<EventObject>,
    pub cameras: Vec<Camera>,
    pub collision_shapes: Vec<CollisionShape>,
}

fn counted<T: Copy>(
    r: &mut Reader,
    tag: &[u8; 4],
    mut f: impl FnMut(&mut Reader) -> Result<T, Error>,
) -> Result<Vec<T>, Error> {
    if &r.tag()? != tag {
        return Err(Error::Format("unexpected geoset sub-chunk"));
    }
    let n = r.u32()? as usize;
    if n > r.remaining() {
        return Err(Error::Eof);
    }
    (0..n).map(|_| f(r)).collect()
}

/// Splits a chunk into objects each prefixed by an inclusive size.
fn sized_objects<'a>(mut r: Reader<'a>) -> Result<Vec<Reader<'a>>, Error> {
    let mut out = Vec::new();
    while r.remaining() >= 4 {
        let size = r.u32()? as usize;
        if size < 4 {
            return Err(Error::Format("object size"));
        }
        out.push(r.sub(size - 4)?);
    }
    Ok(out)
}

/// Reads a node header from a reader positioned at an object's start (the object's own
/// inclusive size has already been consumed).
fn inner_node(r: &mut Reader) -> Result<Node, Error> {
    let size = r.u32()? as usize;
    let mut s = r.sub(size.checked_sub(4).ok_or(Error::Format("node size"))?)?;
    let name = s.fixed_str(80)?;
    let object_id = s.u32()?;
    let parent = s.u32()?;
    let flags = s.u32()?;
    let tracks = Tracks::read(&mut s)?;
    Ok(Node {
        name,
        object_id,
        parent: (parent != u32::MAX).then_some(parent),
        flags,
        tracks,
    })
}

impl Model {
    pub fn parse(data: &[u8]) -> Result<Model, Error> {
        let mut r = Reader::new(data);
        if &r.tag()? != b"MDLX" {
            return Err(Error::Format("not an MDX model"));
        }
        let mut m = Model::default();
        while r.remaining() >= 8 {
            let tag = r.tag()?;
            let size = r.u32()? as usize;
            let mut c = r.sub(size)?;
            match &tag {
                b"VERS" => m.version = c.u32()?,
                b"MODL" => {
                    m.name = c.fixed_str(80)?;
                    m.anim_file = c.fixed_str(260)?;
                    m.extent = extent(&mut c)?;
                    m.blend_time = c.u32()?;
                }
                b"SEQS" => {
                    while c.remaining() >= 132 {
                        let name = c.fixed_str(80)?;
                        let start = c.u32()?;
                        let end = c.u32()?;
                        let move_speed = c.f32()?;
                        let flags = c.u32()?;
                        let rarity = c.f32()?;
                        let _sync = c.u32()?;
                        let extent = extent(&mut c)?;
                        m.sequences.push(Sequence {
                            name,
                            start,
                            end,
                            move_speed,
                            non_looping: flags & 1 != 0,
                            rarity,
                            extent,
                        });
                    }
                }
                b"GLBS" => {
                    while c.remaining() >= 4 {
                        m.global_sequences.push(c.u32()?);
                    }
                }
                b"TEXS" => {
                    while c.remaining() >= 268 {
                        let replaceable_id = c.u32()?;
                        let path = c.fixed_str(260)?;
                        let flags = c.u32()?;
                        m.textures.push(Texture { replaceable_id, path, flags });
                    }
                }
                b"MTLS" => {
                    for mut o in sized_objects(c)? {
                        let priority_plane = o.i32()?;
                        let flags = o.u32()?;
                        if &o.tag()? != b"LAYS" {
                            return Err(Error::Format("material without LAYS"));
                        }
                        let n = o.u32()?;
                        let mut layers = Vec::new();
                        for _ in 0..n {
                            let size = o.u32()? as usize;
                            let mut l = o.sub(size - 4)?;
                            let filter_mode = l.u32()?;
                            let shading = l.u32()?;
                            let texture_id = l.u32()?;
                            let tani = l.u32()?;
                            let coord_id = l.u32()?;
                            let alpha = l.f32()?;
                            let tracks = Tracks::read(&mut l)?;
                            layers.push(Layer {
                                filter_mode,
                                shading,
                                texture_id,
                                texture_anim_id: (tani != u32::MAX).then_some(tani),
                                coord_id,
                                alpha,
                                tracks,
                            });
                        }
                        m.materials.push(Material { priority_plane, flags, layers });
                    }
                }
                b"TXAN" => {
                    for mut o in sized_objects(c)? {
                        m.texture_anims.push(Tracks::read(&mut o)?);
                    }
                }
                b"GEOS" => {
                    for mut o in sized_objects(c)? {
                        let mut g = Geoset {
                            vertices: counted(&mut o, b"VRTX", |r| r.vec3())?,
                            normals: counted(&mut o, b"NRMS", |r| r.vec3())?,
                            ..Default::default()
                        };
                        let _types = counted(&mut o, b"PTYP", |r| r.u32())?;
                        let _counts = counted(&mut o, b"PCNT", |r| r.u32())?;
                        g.faces = counted(&mut o, b"PVTX", |r| r.u16())?;
                        g.vertex_groups = counted(&mut o, b"GNDX", |r| r.u8())?;
                        g.group_sizes = counted(&mut o, b"MTGC", |r| r.u32())?;
                        g.matrix_indices = counted(&mut o, b"MATS", |r| r.u32())?;
                        g.material_id = o.u32()?;
                        g.selection_group = o.u32()?;
                        g.unselectable = o.u32()? & 4 != 0;
                        g.extent = extent(&mut o)?;
                        let n = o.u32()?;
                        for _ in 0..n {
                            g.sequence_extents.push(extent(&mut o)?);
                        }
                        if &o.tag()? != b"UVAS" {
                            return Err(Error::Format("geoset without UVAS"));
                        }
                        let sets = o.u32()?;
                        for _ in 0..sets {
                            g.uvs.push(counted(&mut o, b"UVBS", |r| Ok([r.f32()?, r.f32()?]))?);
                        }
                        m.geosets.push(g);
                    }
                }
                b"GEOA" => {
                    for mut o in sized_objects(c)? {
                        m.geoset_anims.push(GeosetAnim {
                            alpha: o.f32()?,
                            flags: o.u32()?,
                            color: o.vec3()?,
                            geoset_id: o.u32()?,
                            tracks: Tracks::read(&mut o)?,
                        });
                    }
                }
                b"BONE" => {
                    while c.remaining() > 0 {
                        let node = node(&mut c)?;
                        let gs = c.u32()?;
                        let ga = c.u32()?;
                        m.bones.push(Bone {
                            node,
                            geoset_id: (gs != u32::MAX).then_some(gs),
                            geoset_anim_id: (ga != u32::MAX).then_some(ga),
                        });
                    }
                }
                b"HELP" => {
                    while c.remaining() > 0 {
                        m.helpers.push(node(&mut c)?);
                    }
                }
                b"LITE" => {
                    for mut o in sized_objects(c)? {
                        let node = inner_node(&mut o)?;
                        m.lights.push(Light {
                            node,
                            kind: o.u32()?,
                            attenuation: [o.f32()?, o.f32()?],
                            color: o.vec3()?,
                            intensity: o.f32()?,
                            ambient_color: o.vec3()?,
                            ambient_intensity: o.f32()?,
                            tracks: Tracks::read(&mut o)?,
                        });
                    }
                }
                b"ATCH" => {
                    for mut o in sized_objects(c)? {
                        let node = inner_node(&mut o)?;
                        m.attachments.push(Attachment {
                            node,
                            path: o.fixed_str(260)?,
                            attachment_id: o.u32()?,
                            tracks: Tracks::read(&mut o)?,
                        });
                    }
                }
                b"PIVT" => {
                    while c.remaining() >= 12 {
                        m.pivots.push(c.vec3()?);
                    }
                }
                b"PREM" => {
                    for mut o in sized_objects(c)? {
                        let node = inner_node(&mut o)?;
                        m.particle_emitters.push(ParticleEmitter {
                            node,
                            emission_rate: o.f32()?,
                            gravity: o.f32()?,
                            longitude: o.f32()?,
                            latitude: o.f32()?,
                            path: o.fixed_str(260)?,
                            lifespan: o.f32()?,
                            speed: o.f32()?,
                            tracks: Tracks::read(&mut o)?,
                        });
                    }
                }
                b"PRE2" => {
                    for mut o in sized_objects(c)? {
                        let node = inner_node(&mut o)?;
                        let speed = o.f32()?;
                        let variation = o.f32()?;
                        let latitude = o.f32()?;
                        let gravity = o.f32()?;
                        let lifespan = o.f32()?;
                        let emission_rate = o.f32()?;
                        let width = o.f32()?;
                        let length = o.f32()?;
                        let filter_mode = o.u32()?;
                        let rows = o.u32()?;
                        let columns = o.u32()?;
                        let head_or_tail = o.u32()?;
                        let tail_length = o.f32()?;
                        let time = o.f32()?;
                        let segment_color = [o.vec3()?, o.vec3()?, o.vec3()?];
                        let segment_alpha = [o.u8()?, o.u8()?, o.u8()?];
                        let segment_scaling = o.vec3()?;
                        let mut iv = [[0u32; 3]; 4];
                        for v in iv.iter_mut().flatten() {
                            *v = o.u32()?;
                        }
                        m.particle_emitters2.push(ParticleEmitter2 {
                            node,
                            speed,
                            variation,
                            latitude,
                            gravity,
                            lifespan,
                            emission_rate,
                            width,
                            length,
                            filter_mode,
                            rows,
                            columns,
                            head_or_tail,
                            tail_length,
                            time,
                            segment_color,
                            segment_alpha,
                            segment_scaling,
                            head_intervals: [iv[0], iv[1]],
                            tail_intervals: [iv[2], iv[3]],
                            texture_id: o.u32()?,
                            squirt: o.u32()? != 0,
                            priority_plane: o.i32()?,
                            replaceable_id: o.u32()?,
                            tracks: Tracks::read(&mut o)?,
                        });
                    }
                }
                b"RIBB" => {
                    for mut o in sized_objects(c)? {
                        let node = inner_node(&mut o)?;
                        m.ribbon_emitters.push(RibbonEmitter {
                            node,
                            height_above: o.f32()?,
                            height_below: o.f32()?,
                            alpha: o.f32()?,
                            color: o.vec3()?,
                            lifespan: o.f32()?,
                            texture_slot: o.u32()?,
                            emission_rate: o.u32()?,
                            rows: o.u32()?,
                            columns: o.u32()?,
                            material_id: o.u32()?,
                            gravity: o.f32()?,
                            tracks: Tracks::read(&mut o)?,
                        });
                    }
                }
                b"EVTS" => {
                    while c.remaining() > 0 {
                        let node = node(&mut c)?;
                        let mut global_seq = None;
                        let mut frames = Vec::new();
                        if c.remaining() >= 4 && &c.data[c.pos..c.pos + 4] == b"KEVT" {
                            c.skip(4)?;
                            let n = c.u32()?;
                            let gs = c.i32()?;
                            global_seq = (gs >= 0).then_some(gs as u32);
                            for _ in 0..n {
                                frames.push(c.u32()?);
                            }
                        }
                        m.event_objects.push(EventObject { node, global_seq, frames });
                    }
                }
                b"CAMS" => {
                    for mut o in sized_objects(c)? {
                        m.cameras.push(Camera {
                            name: o.fixed_str(80)?,
                            position: o.vec3()?,
                            fov: o.f32()?,
                            far_clip: o.f32()?,
                            near_clip: o.f32()?,
                            target: o.vec3()?,
                            tracks: Tracks::read(&mut o)?,
                        });
                    }
                }
                b"CLID" => {
                    while c.remaining() > 0 {
                        let node = node(&mut c)?;
                        let shape = match c.u32()? {
                            0 => Shape::Box(c.vec3()?, c.vec3()?),
                            1 => Shape::Plane(c.vec3()?, c.vec3()?),
                            2 => Shape::Sphere(c.vec3()?, c.f32()?),
                            _ => Shape::Cylinder(c.vec3()?, c.vec3()?, c.f32()?),
                        };
                        m.collision_shapes.push(CollisionShape { node, shape });
                    }
                }
                _ => log::debug!("skipping MDX chunk {}", String::from_utf8_lossy(&tag)),
            }
        }
        Ok(m)
    }

    /// Every animated node in the model, indexed by object id.
    pub fn nodes(&self) -> Vec<Option<&Node>> {
        let mut all: Vec<&Node> = Vec::new();
        all.extend(self.bones.iter().map(|b| &b.node));
        all.extend(self.lights.iter().map(|b| &b.node));
        all.extend(self.helpers.iter());
        all.extend(self.attachments.iter().map(|b| &b.node));
        all.extend(self.particle_emitters.iter().map(|b| &b.node));
        all.extend(self.particle_emitters2.iter().map(|b| &b.node));
        all.extend(self.ribbon_emitters.iter().map(|b| &b.node));
        all.extend(self.event_objects.iter().map(|b| &b.node));
        all.extend(self.collision_shapes.iter().map(|b| &b.node));
        let max = all.iter().map(|n| n.object_id as usize + 1).max().unwrap_or(0);
        let mut out = vec![None; max];
        for n in all {
            out[n.object_id as usize] = Some(n);
        }
        out
    }
}
