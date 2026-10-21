// Prelude prepended to every pattern fragment snippet by the pattern
// video source. It declares the uniforms, draws one triangle that covers
// the full target, and calls the snippet's `shade` function once per
// pixel.
//
// Each pattern snippet must define `fn shade(uv: vec2<f32>) -> vec4<f32>`,
// and must not redeclare the names below.

struct LkUniforms {
    resolution: vec2<f32>,
    time_s: f32,
    frame_index: u32,
}

@group(0) @binding(0) var<uniform> lk: LkUniforms;

struct LkVertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> LkVertexOutput {
    let corner = vec2<f32>(f32((vertex_index << 1u) & 2u), f32(vertex_index & 2u));
    var out: LkVertexOutput;
    out.position = vec4<f32>(corner * 2.0 - 1.0, 0.0, 1.0);
    out.uv = vec2<f32>(corner.x, 1.0 - corner.y);
    return out;
}

@fragment
fn fs_main(in: LkVertexOutput) -> @location(0) vec4<f32> {
    return shade(in.uv);
}
