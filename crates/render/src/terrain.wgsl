struct Globals {
    view_proj: mat4x4<f32>,
    light_dir: vec4<f32>,
    light_color: vec4<f32>,
    ambient: vec4<f32>,
    fog_color: vec4<f32>,
    fog: vec4<f32>,
    camera_pos: vec4<f32>,
};

@group(0) @binding(0) var<uniform> globals: Globals;
@group(1) @binding(0) var tex: texture_2d<f32>;
@group(1) @binding(1) var samp: sampler;

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
    let lit = clamp(globals.ambient.rgb + globals.light_color.rgb * l, vec3<f32>(0.0), vec3<f32>(1.0));
    return vec4<f32>(fog(t.rgb * lit, in.world), t.a);
}

@fragment
fn fs_water(in: VsOut) -> @location(0) vec4<f32> {
    let t = textureSample(tex, samp, in.uv) * in.color;
    return vec4<f32>(fog(t.rgb, in.world), t.a);
}
