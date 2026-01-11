struct VSOut {
  @builtin(position) pos: vec4<f32>,
  @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vid: u32) -> VSOut {
  var pos = array<vec2<f32>, 3>(
    vec2<f32>(-1.0, -3.0),
    vec2<f32>(-1.0,  1.0),
    vec2<f32>( 3.0,  1.0)
  );
  let p = pos[vid];
  var out: VSOut;
  out.pos = vec4<f32>(p, 0.0, 1.0);
  out.uv = 0.5 * (p + vec2<f32>(1.0, 1.0));
  return out;
}

@group(0) @binding(0) var samp: sampler;
@group(0) @binding(1) var y_tex: texture_2d<f32>;
@group(0) @binding(2) var u_tex: texture_2d<f32>;
@group(0) @binding(3) var v_tex: texture_2d<f32>;

struct Params {
  src_w: u32,
  src_h: u32,
  y_tex_w: u32,
  uv_tex_w: u32,
};
@group(0) @binding(4) var<uniform> params: Params;

fn yuv_to_rgb(y: f32, u: f32, v: f32) -> vec3<f32> {
  let c = y - (16.0/255.0);
  let d = u - 0.5;
  let e = v - 0.5;
  let r = 1.164 * c + 1.596 * e;
  let g = 1.164 * c - 0.392 * d - 0.813 * e;
  let b = 1.164 * c + 2.017 * d;
  return clamp(vec3<f32>(r, g, b), vec3<f32>(0.0), vec3<f32>(1.0));
}

@fragment
fn fs_main(in_: VSOut) -> @location(0) vec4<f32> {
  let src_w = f32(params.src_w);
  let src_h = f32(params.src_h);
  let y_tex_w = f32(params.y_tex_w);
  let uv_tex_w = f32(params.uv_tex_w);

  // Flip vertically and scale X to avoid sampling padded columns
  let flipped = vec2<f32>(in_.uv.x, 1.0 - in_.uv.y);
  let uv_y = vec2<f32>(flipped.x * (src_w / y_tex_w), flipped.y);
  let uv_uv = vec2<f32>(flipped.x * ((src_w * 0.5) / uv_tex_w), flipped.y);

  let y = textureSample(y_tex, samp, uv_y).r;
  let u = textureSample(u_tex, samp, uv_uv).r;
  let v = textureSample(v_tex, samp, uv_uv).r;

  let rgb = yuv_to_rgb(y, u, v);
  return vec4<f32>(rgb, 1.0);
}


