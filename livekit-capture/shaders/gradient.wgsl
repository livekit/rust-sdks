// Animated color gradient: the built-in gradient pattern.
fn shade(uv: vec2<f32>) -> vec4<f32> {
    let color = 0.5 + 0.5 * cos(lk.time_s + uv.xyx * 4.0 + vec3<f32>(0.0, 2.0, 4.0));
    return vec4<f32>(color, 1.0);
}
