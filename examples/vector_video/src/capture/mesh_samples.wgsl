override width: u32;
override pixels: u32;

@group(0) @binding(0) var<storage, read> input: array<u32>;
@group(0) @binding(1) var<storage, read_write> output: array<u32>;

// Match Nokhwa's integer YUYV conversion, clipping before the RGB average.
fn rgb(y: u32, u: u32, v: u32) -> vec3<u32> {
    let luma = 298 * (i32(y) - 16) + 128;
    let cb = i32(u) - 128;
    let cr = i32(v) - 128;
    let channels = vec3<i32>(luma + 409 * cr, luma - 100 * cb - 208 * cr, luma + 516 * cb);
    return vec3<u32>(clamp(channels >> vec3<u32>(8), vec3<i32>(0), vec3<i32>(255)));
}

fn pair(word: u32) -> vec3<u32> {
    let u = (word >> 8u) & 255u;
    let v = word >> 24u;
    return rgb(word & 255u, u, v) + rgb((word >> 16u) & 255u, u, v);
}

fn sample(index: u32) -> u32 {
    if index >= pixels {
        return 0u;
    }
    let top = (index / width) * (2u * width) + index % width;
    let average = (pair(input[top]) + pair(input[top + width]) + vec3<u32>(2)) / 4u;
    return average.r | (average.g << 8u) | (average.b << 16u);
}

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) thread: vec3<u32>) {
    let i = thread.x * 4u;
    if i >= pixels {
        return;
    }
    let a = sample(i);
    let b = sample(i + 1u);
    let c = sample(i + 2u);
    let d = sample(i + 3u);
    // Pack four RGB24 pixels into three storage words for a direct copy to the pipe.
    let offset = thread.x * 3u;
    output[offset] = a | (b << 24u);
    output[offset + 1u] = (b >> 8u) | (c << 16u);
    output[offset + 2u] = (c >> 16u) | (d << 8u);
}
