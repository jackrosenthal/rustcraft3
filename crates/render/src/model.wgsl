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

struct Draw {
    color: vec4<f32>,
    uv_offset: vec2<f32>,
    bone_offset: u32,
    flags: u32,
    light_cell: u32,
};

struct Light {
    pos: vec4<f32>,
    diffuse: vec4<f32>,
    ambient: vec4<f32>,
};

@group(0) @binding(0) var<uniform> globals: Globals;
@group(0) @binding(1) var<storage, read> bones: array<mat4x4<f32>>;
@group(0) @binding(2) var fow_tex: texture_2d<f32>;
@group(0) @binding(3) var fow_samp: sampler;
@group(0) @binding(4) var<storage, read> lights: array<Light>;
@group(0) @binding(5) var<storage, read> light_cells: array<u32>;

// The lights of the instance's cell (see lights.rs): the day/night light, or a model's
// light, Direct3D's way: its ambient plus its diffuse by the angle, a point light's under
// 1 / (1 + 0.0005 d^2) out to 10000.
fn lighting(n: vec3<f32>, world: vec3<f32>) -> vec3<f32> {
    let sun = globals.ambient.rgb + globals.light_color.rgb * max(dot(n, -normalize(globals.light_dir.xyz)), 0.0);
    if (globals.light_grid.z <= 0.0 || draw.light_cell == 0xffffffffu) {
        return clamp(sun, vec3<f32>(0.0), vec3<f32>(1.0));
    }
    var sum = vec3<f32>(0.0);
    for (var k = 0u; k < 8u; k++) {
        let i = light_cells[draw.light_cell * 8u + k];
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
@group(1) @binding(0) var<uniform> draw: Draw;
@group(2) @binding(0) var tex: texture_2d<f32>;
@group(2) @binding(1) var samp: sampler;

struct VsIn {
    @location(0) pos: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) bone_ids: vec4<u32>,
    @location(4) bone_count: u32,
};

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) world: vec3<f32>,
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
    var p = vec4<f32>(0.0);
    var n = vec3<f32>(0.0);
    let count = max(v.bone_count, 1u);
    for (var i = 0u; i < count; i++) {
        let m = bones[draw.bone_offset + select(0u, v.bone_ids[i], v.bone_count > 0u)];
        p += m * vec4<f32>(v.pos, 1.0);
        n += (m * vec4<f32>(v.normal, 0.0)).xyz;
    }
    p /= f32(count);
    var out: VsOut;
    out.clip = globals.view_proj * vec4<f32>(p.xyz, 1.0);
    out.uv = v.uv + draw.uv_offset;
    out.normal = normalize(n);
    out.world = p.xyz;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let t = textureSample(tex, samp, in.uv);
    var c = t * draw.color;
    // Alpha testing looks at the texture; fading (geoset or instance alpha) blends.
    if ((draw.flags & 2u) != 0u && t.a < 0.75) {
        discard;
    }
    if ((draw.flags & 1u) == 0u) {
        c = vec4<f32>(c.rgb * lighting(in.normal, in.world), c.a);
    }
    if (globals.fow_extent.z > 0.0) {
        // Fogged objects take FoggedObject's color, masked ones the masked color.
        let fuv = (in.world.xy - globals.fow_extent.xy) / globals.fow_extent.zw;
        let v = textureSample(fow_tex, fow_samp, vec2<f32>(fuv.x, fuv.y)).r;
        let fogged = c.rgb * globals.fow_object.rgb;
        let masked = c.rgb * globals.fow_masked_object.rgb;
        let lit = mix(mix(masked, fogged, clamp(v / 0.5, 0.0, 1.0)), c.rgb, clamp(v * 2.0 - 1.0, 0.0, 1.0));
        c = vec4<f32>(lit, c.a);
    }
    if (globals.fog.z > 0.0 && (draw.flags & 32u) == 0u) {
        let d = distance(in.world, globals.camera_pos.xyz);
        let f = fog_amount(d);
        var fc = globals.fog_color.rgb;
        if ((draw.flags & 4u) != 0u) {
            fc = vec3<f32>(0.0);
        } else if ((draw.flags & 8u) != 0u) {
            fc = vec3<f32>(1.0);
        }
        c = vec4<f32>(mix(c.rgb, fc, f), c.a);
    }
    if ((draw.flags & 16u) != 0u) {
        c = vec4<f32>(c.rgb * draw.color.a, c.a);
    }
    return c;
}
