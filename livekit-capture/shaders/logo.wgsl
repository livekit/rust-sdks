// Bouncing LiveKit logo: the built-in logo pattern.
//
// A white 7x7-cell tile carries the LiveKit glyph in black at its
// center. The tile moves in a straight line and reflects off the frame
// edges. Position is a pure function of time.
//
// All rectangles get about two pixels of edge feather. The soft edges
// make sub-pixel motion smooth, and they survive chroma subsampling and
// video encoding.

// The glyph as a 5x5 bitmap. Each row is a 5-bit mask. The highest bit
// is the leftmost column:
//
//   1 0 0 0 1
//   1 0 0 1 0
//   1 0 1 0 0
//   1 0 0 1 0
//   1 1 1 0 1
const GLYPH_ROWS: array<u32, 5> = array<u32, 5>(0x11u, 0x12u, 0x14u, 0x12u, 0x1du);

// Cells per tile side, and the glyph offset into the tile, in cells.
const TILE_CELLS: f32 = 7.0;
const GLYPH_OFFSET: f32 = 1.0;

// Tile height as a fraction of the frame height. This keeps the logo
// the same visual size at every resolution.
const LOGO_SIZE: f32 = 0.25;

// Speed along each axis, in frame heights per second. The two values
// have no small common multiple, so the bounce path repeats slowly.
const SPEED: vec2<f32> = vec2<f32>(0.23, 0.17);

// Starting phase, so the logo does not start in a corner.
const START_PHASE: vec2<f32> = vec2<f32>(0.34, 0.71);

// White and black keep every edge luma-only. Luma has full resolution
// in 4:2:0 video, so these edges encode cleanly.
const TILE_COLOR: vec3<f32> = vec3<f32>(1.0, 1.0, 1.0);
const GLYPH_COLOR: vec3<f32> = vec3<f32>(0.0, 0.0, 0.0);
const BACKGROUND: vec3<f32> = vec3<f32>(0.0, 0.0, 0.0);

// Folds a growing phase into ping-pong motion between 0.0 and 1.0.
fn ping_pong(phase: f32) -> f32 {
    return 1.0 - abs(1.0 - 2.0 * fract(phase * 0.5));
}

// Coverage of the rectangle [min_p, max_p] at point `p`. The interior
// is fully opaque. Alpha falls to zero over `feather` outside the edge,
// so touching rectangles union without seams.
fn rect_alpha(p: vec2<f32>, min_p: vec2<f32>, max_p: vec2<f32>, feather: f32) -> f32 {
    let center = (min_p + max_p) * 0.5;
    let half_size = (max_p - min_p) * 0.5;
    let d = abs(p - center) - half_size;
    let outside = length(max(d, vec2<f32>(0.0)));
    let inside = min(max(d.x, d.y), 0.0);
    return 1.0 - smoothstep(0.0, feather, outside + inside);
}

fn shade(uv: vec2<f32>) -> vec4<f32> {
    // Work in a space that is `aspect` wide and 1.0 tall, so the cells
    // stay square. Pixels in this space are 1.0 / height on both axes.
    let height = max(lk.resolution.y, 1.0);
    let aspect = lk.resolution.x / height;
    let p = vec2<f32>(uv.x * aspect, uv.y);

    // Distance the logo can travel along each axis. The lower bound
    // keeps the math finite when the frame is narrower than the logo.
    let travel = max(vec2<f32>(aspect, 1.0) - vec2<f32>(LOGO_SIZE), vec2<f32>(0.0001));
    let phase = START_PHASE + lk.time_s * SPEED / travel;
    let origin = vec2<f32>(ping_pong(phase.x), ping_pong(phase.y)) * travel;

    // About two output pixels of edge feather.
    let feather = 2.0 / height;

    // Skip the coverage math outside the tile and its feather band.
    let local = p - origin;
    if local.x < -feather || local.x > LOGO_SIZE + feather
        || local.y < -feather || local.y > LOGO_SIZE + feather {
        return vec4<f32>(BACKGROUND, 1.0);
    }

    // Coverage of the tile, and of the glyph cells inside it.
    let tile = rect_alpha(p, origin, origin + vec2<f32>(LOGO_SIZE), feather);

    let cell_size = LOGO_SIZE / TILE_CELLS;
    var glyph = 0.0;
    for (var row = 0u; row < 5u; row = row + 1u) {
        let bits = GLYPH_ROWS[row];
        for (var col = 0u; col < 5u; col = col + 1u) {
            if ((bits >> (4u - col)) & 1u) == 0u {
                continue;
            }
            let cell_min = origin
                + (vec2<f32>(f32(col), f32(row)) + vec2<f32>(GLYPH_OFFSET)) * cell_size;
            glyph = max(glyph, rect_alpha(p, cell_min, cell_min + vec2<f32>(cell_size), feather));
        }
    }

    let logo = mix(TILE_COLOR, GLYPH_COLOR, glyph);
    return vec4<f32>(mix(BACKGROUND, logo, tile), 1.0);
}
