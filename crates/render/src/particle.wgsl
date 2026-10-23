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
    fow_extent: vec4<f32>,
    fow_terrain: vec4<f32>,
    fow_object: vec4<f32>,
};

@group(0) @binding(0) var<uniform> globals: Globals;
@group(0) @binding(2) var fow_tex: texture_2d<f32>;
@group(0) @binding(3) var fow_samp: sampler;
@group(1) @binding(0) var tex: texture_2d<f32>;
@group(1) @binding(1) var samp: sampler;

struct VsIn {
    @location(0) pos: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) flags: u32,
};

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) world: vec3<f32>,
    @location(3) @interpolate(flat) flags: u32,
};

@vertex
fn vs_main(v: VsIn) -> VsOut {
    var out: VsOut;
    out.clip = globals.view_proj * vec4<f32>(v.pos, 1.0);
    out.uv = v.uv;
    out.color = v.color;
    out.world = v.pos;
    out.flags = v.flags;
    return out;
}

/// flags: 1 unfogged, 2 alpha key, 4 additive (fog fades it out).
@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    var c = textureSample(tex, samp, in.uv) * in.color;
    if ((in.flags & 2u) != 0u && c.a < 0.75) {
        discard;
    }
    if (globals.fow_extent.z > 0.0) {
        let fuv = (in.world.xy - globals.fow_extent.xy) / globals.fow_extent.zw;
        let v = textureSample(fow_tex, fow_samp, fuv).r;
        c = vec4<f32>(c.rgb * clamp(v, 0.0, 1.0), c.a);
    }
    if (globals.fog.z > 0.0 && (in.flags & 1u) == 0u) {
        let d = distance(in.world, globals.camera_pos.xyz);
        let f = clamp((d - globals.fog.x) / max(globals.fog.y - globals.fog.x, 1.0), 0.0, 1.0);
        var fc = globals.fog_color.rgb;
        if ((in.flags & 4u) != 0u) {
            fc = vec3<f32>(0.0);
        }
        c = vec4<f32>(mix(c.rgb, fc, f), c.a);
    }
    return c;
}
