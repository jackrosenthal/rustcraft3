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
    water_tint: vec4<f32>,
    /// The light grid's first cell and its size in cells (zero: no grid).
    light_grid: vec4<f32>,
};

struct Light {
    pos: vec4<f32>,
    diffuse: vec4<f32>,
    ambient: vec4<f32>,
};

@group(0) @binding(0) var<uniform> globals: Globals;
@group(0) @binding(1) var<uniform> extent: vec4<f32>;
@group(1) @binding(0) var tex: texture_2d<f32>;
@group(1) @binding(1) var samp: sampler;
@group(2) @binding(0) var shadow_tex: texture_2d<f32>;
@group(2) @binding(1) var shadow_samp: sampler;
@group(0) @binding(2) var fow_tex: texture_2d<f32>;
@group(0) @binding(3) var fow_samp: sampler;
@group(0) @binding(4) var<storage, read> lights: array<Light>;
@group(0) @binding(5) var<storage, read> light_cells: array<u32>;

// The lights of the cell the ground is in (see lights.rs, and the model shader).
fn lighting(n: vec3<f32>, world: vec3<f32>) -> vec3<f32> {
    let sun = globals.terrain_ambient.rgb + globals.terrain_light_color.rgb * max(dot(n, -normalize(globals.light_dir.xyz)), 0.0);
    let g = globals.light_grid;
    let ci = floor((world.xy + vec2<f32>(512.0)) / 1024.0) - g.xy;
    if (g.z <= 0.0 || ci.x < 0.0 || ci.y < 0.0 || ci.x >= g.z || ci.y >= g.w) {
        return clamp(sun, vec3<f32>(0.0), vec3<f32>(1.0));
    }
    let cell = u32(ci.y) * u32(g.z) + u32(ci.x);
    var sum = vec3<f32>(0.0);
    for (var k = 0u; k < 8u; k++) {
        let i = light_cells[cell * 8u + k];
        if (i == 0xffffffffu) {
            break;
        }
        if (i == 0xfffffffeu) {
            sum += sun;
            continue;
        }
        let l = lights[i];
        if (l.pos.w < 0.5) {
            sum += l.ambient.rgb + l.diffuse.rgb * max(dot(n, -normalize(l.pos.xyz)), 0.0);
            continue;
        }
        let d = l.pos.xyz - world;
        let dist = length(d);
        if (dist < 10000.0) {
            let att = 1.0 / (1.0 + 0.0005 * dist * dist);
            sum += (l.ambient.rgb + l.diffuse.rgb * max(dot(n, d / max(dist, 0.001)), 0.0)) * att;
        }
    }
    return clamp(sum, vec3<f32>(0.0), vec3<f32>(1.0));
}

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

// Direct3D's fog modes as game.dll sets them (style 0 linear from start to end, 1
// exponential, 2 squared exponential, by density): fog.z is the style + 1, fog.w the
// density.
fn fog_amount(d: f32) -> f32 {
    if (globals.fog.z > 2.5) {
        let e = d * globals.fog.w;
        return 1.0 - exp(-e * e);
    }
    if (globals.fog.z > 1.5) {
        return 1.0 - exp(-d * globals.fog.w);
    }
    return clamp((d - globals.fog.x) / max(globals.fog.y - globals.fog.x, 1.0), 0.0, 1.0);
}

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
    let f = fog_amount(d);
    return mix(c, globals.fog_color.rgb, f);
}

@fragment
fn fs_ground(in: VsOut) -> @location(0) vec4<f32> {
    let t = textureSample(tex, samp, in.uv);
    var lit = lighting(normalize(in.normal), in.world);
    let suv = (in.world.xy - extent.xy) / extent.zw;
    // Rows run south to north.
    let shadow = textureSample(shadow_tex, shadow_samp, suv).r;
    lit = lit * (1.0 - shadow * SHADOW_STRENGTH);
    return vec4<f32>(fog(fow(t.rgb * lit, in.world), in.world), t.a);
}

@fragment
fn fs_water(in: VsOut) -> @location(0) vec4<f32> {
    let t = textureSample(tex, samp, in.uv) * in.color * globals.water_tint;
    return vec4<f32>(fog(fow(t.rgb, in.world), in.world), t.a);
}
