@group(0) @binding(0) var tex: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;

struct VsIn {
    @location(0) pos: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) mode: u32,
};

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) @interpolate(flat) mode: u32,
};

@vertex
fn vs_main(v: VsIn) -> VsOut {
    var o: VsOut;
    o.clip = vec4<f32>(v.pos, 0.0, 1.0);
    o.uv = v.uv;
    o.color = v.color;
    o.mode = v.mode;
    return o;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let t = textureSample(tex, samp, in.uv);
    if (in.mode == 1u) {
        return vec4<f32>(in.color.rgb, in.color.a * t.r);
    }
    let c = t * in.color;
    if (in.mode == 2u && c.a < 0.75) {
        discard;
    }
    return c;
}
