struct Globals {
    view_proj: mat4x4<f32>,
    light_dir: vec4<f32>,
    light_color: vec4<f32>,
    ambient: vec4<f32>,
    fog_color: vec4<f32>,
    fog: vec4<f32>,
    camera_pos: vec4<f32>,
    terrain_light_color: vec4<f32>,
    terrain_ambient: vec4<f32>,
    /// Fog of war grid: origin xy and size zw in world units; z of 0 disables it.
    fow_extent: vec4<f32>,
    /// [FogOfWar] FoggedTerrain and FoggedObject (rgb, alpha).
    fow_terrain: vec4<f32>,
    fow_object: vec4<f32>,
    fow_masked_terrain: vec4<f32>,
    fow_masked_object: vec4<f32>,
};

@group(0) @binding(0) var<uniform> globals: Globals;
@group(0) @binding(1) var<uniform> extent: vec4<f32>;
@group(1) @binding(0) var tex: texture_2d<f32>;
@group(1) @binding(1) var samp: sampler;
@group(2) @binding(0) var shadow_tex: texture_2d<f32>;
@group(2) @binding(1) var shadow_samp: sampler;
@group(0) @binding(2) var fow_tex: texture_2d<f32>;
@group(0) @binding(3) var fow_samp: sampler;

/// Fog of war over terrain: fogged cells blend toward FoggedTerrain by its alpha, masked
/// cells toward the masked color by its.  The texture holds 255 visible, 128 fogged, and 0
/// masked.
fn fow(c: vec3<f32>, world: vec3<f32>) -> vec3<f32> {
    if (globals.fow_extent.z <= 0.0) {
        return c;
    }
    let fuv = (world.xy - globals.fow_extent.xy) / globals.fow_extent.zw;
    let v = textureSample(fow_tex, fow_samp, fuv).r;
    let fogged = mix(c, globals.fow_terrain.rgb, globals.fow_terrain.a);
    let masked = mix(c, globals.fow_masked_terrain.rgb, globals.fow_masked_terrain.a);
    return mix(mix(masked, fogged, clamp(v / 0.5, 0.0, 1.0)), c, clamp(v * 2.0 - 1.0, 0.0, 1.0));
}

/// Darkening applied by the map's baked shadows: the alpha of UI\MiscData.txt
/// [FogOfWar] ShadowImageColor (170 of 255).
const SHADOW_STRENGTH: f32 = 0.6667;

struct VsIn {
    @location(0) pos: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) color: vec4<f32>,
};

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec4<f32>,
    @location(3) world: vec3<f32>,
};

@vertex
fn vs_main(v: VsIn) -> VsOut {
    var out: VsOut;
    out.clip = globals.view_proj * vec4<f32>(v.pos, 1.0);
    out.uv = v.uv;
    out.normal = v.normal;
    out.color = v.color;
    out.world = v.pos;
    return out;
}

fn fog(c: vec3<f32>, world: vec3<f32>) -> vec3<f32> {
    if (globals.fog.z <= 0.0) {
        return c;
    }
    let d = distance(world, globals.camera_pos.xyz);
    let f = clamp((d - globals.fog.x) / max(globals.fog.y - globals.fog.x, 1.0), 0.0, 1.0);
    return mix(c, globals.fog_color.rgb, f);
}

@fragment
fn fs_ground(in: VsOut) -> @location(0) vec4<f32> {
    let t = textureSample(tex, samp, in.uv);
    let l = max(dot(normalize(in.normal), -normalize(globals.light_dir.xyz)), 0.0);
    var lit = clamp(globals.terrain_ambient.rgb + globals.terrain_light_color.rgb * l, vec3<f32>(0.0), vec3<f32>(1.0));
    let suv = (in.world.xy - extent.xy) / extent.zw;
    // Rows run south to north.
    let shadow = textureSample(shadow_tex, shadow_samp, suv).r;
    lit = lit * (1.0 - shadow * SHADOW_STRENGTH);
    return vec4<f32>(fog(fow(t.rgb * lit, in.world), in.world), t.a);
}

@fragment
fn fs_water(in: VsOut) -> @location(0) vec4<f32> {
    let t = textureSample(tex, samp, in.uv) * in.color;
    return vec4<f32>(fog(fow(t.rgb, in.world), in.world), t.a);
}
