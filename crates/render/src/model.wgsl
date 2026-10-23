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
};

struct Draw {
    color: vec4<f32>,
    uv_offset: vec2<f32>,
    bone_offset: u32,
    flags: u32,
};

@group(0) @binding(0) var<uniform> globals: Globals;
@group(0) @binding(1) var<storage, read> bones: array<mat4x4<f32>>;
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
    var c = textureSample(tex, samp, in.uv) * draw.color;
    if ((draw.flags & 2u) != 0u && c.a < 0.75) {
        discard;
    }
    if ((draw.flags & 1u) == 0u) {
        let l = max(dot(in.normal, -normalize(globals.light_dir.xyz)), 0.0);
        c = vec4<f32>(c.rgb * clamp(globals.ambient.rgb + globals.light_color.rgb * l, vec3<f32>(0.0), vec3<f32>(1.0)), c.a);
    }
    if (globals.fog.z > 0.0) {
        let d = distance(in.world, globals.camera_pos.xyz);
        let f = clamp((d - globals.fog.x) / max(globals.fog.y - globals.fog.x, 1.0), 0.0, 1.0);
        c = vec4<f32>(mix(c.rgb, globals.fog_color.rgb, f), c.a);
    }
    return c;
}
