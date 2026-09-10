// Wall clock: the shader for the clock video source.
//
// The clock shows HH:MM:SS.mmm as seven-segment digits, with a grid of
// cells below it. Each grid row fills to show one millisecond digit, so
// a viewer can read sub-frame time from a paused frame.
//
// The CPU samples the wall clock once per frame and sends the twelve
// character codes in the uniform. Codes 0 to 9 are digits, 10 is a
// colon, and 11 is a dot.
//
// Shapes get about 1.5 output pixels of edge feather, so they stay
// clean through video encoding.

struct ClockUniform {
    viewport_size: vec2<f32>,
    _pad0: vec2<u32>,
    chars0: vec4<u32>,
    chars1: vec4<u32>,
    chars2: vec4<u32>,
}

@group(0) @binding(0) var<uniform> clock: ClockUniform;

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

const CHAR_COUNT: u32 = 12u;
const COLON_CODE: u32 = 10u;
const DOT_CODE: u32 = 11u;

// Layout metrics, in layout units. The digits sit in one row, and the
// millisecond grid sits below them.
const DIGIT_HEIGHT: f32 = 1.85;
const CELL_WIDTH: f32 = 1.0;
const SEGMENT_THICKNESS: f32 = 0.16;
const COLON_WIDTH: f32 = 0.34;
const DOT_WIDTH: f32 = 0.24;
const GAP: f32 = 0.14;
const TOTAL_WIDTH: f32 = 9.0 * CELL_WIDTH + 2.0 * COLON_WIDTH + DOT_WIDTH + 11.0 * GAP;
const GRID_COLUMNS: u32 = 9u;
const GRID_ROWS: u32 = 3u;
const GRID_CELL: f32 = 0.72;
const GRID_COLUMN_GAP: f32 = 0.30;
const GRID_ROW_GAP: f32 = 0.30;
const GRID_TOP_GAP: f32 = 0.22;
const GRID_WIDTH: f32 = 9.0 * GRID_CELL + 8.0 * GRID_COLUMN_GAP;
const GRID_HEIGHT: f32 = 3.0 * GRID_CELL + 2.0 * GRID_ROW_GAP;
const GROUP_HEIGHT: f32 = DIGIT_HEIGHT + GRID_TOP_GAP + GRID_HEIGHT;

// Warm white for lit shapes, dark gray for unfilled grid cells.
const FOREGROUND: vec3<f32> = vec3<f32>(1.0, 0.98, 0.92);
const EMPTY_CELL: vec3<f32> = vec3<f32>(0.14, 0.14, 0.14);

// Segment masks for the digits 0 to 9. Bit `n` lights segment `n` of
// the seven-segment layout in segment_rect.
const DIGIT_MASKS: array<u32, 10> = array<u32, 10>(
    0x3fu,
    0x06u,
    0x5bu,
    0x4fu,
    0x66u,
    0x6du,
    0x7du,
    0x07u,
    0x7fu,
    0x6fu,
);

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOut {
    // One triangle that covers the full target. `uv` runs from (0, 0)
    // at the top left to (1, 1) at the bottom right.
    let corner = vec2<f32>(f32((vertex_index << 1u) & 2u), f32(vertex_index & 2u));
    var out: VertexOut;
    out.position = vec4<f32>(corner * 2.0 - 1.0, 0.0, 1.0);
    out.uv = vec2<f32>(corner.x, 1.0 - corner.y);
    return out;
}

// Returns the character code at `index`, from the uniform.
fn char_at(index: u32) -> u32 {
    if index < 4u {
        return clock.chars0[index];
    }
    if index < 8u {
        return clock.chars1[index - 4u];
    }
    return clock.chars2[index - 8u];
}

fn char_width(code: u32) -> f32 {
    if code < 10u {
        return CELL_WIDTH;
    }
    if code == COLON_CODE {
        return COLON_WIDTH;
    }
    return DOT_WIDTH;
}

// Bounds of one seven-segment segment, as (min_x, min_y, max_x, max_y).
fn segment_rect(segment: u32) -> vec4<f32> {
    let t = SEGMENT_THICKNESS;
    let mid = DIGIT_HEIGHT * 0.5;

    switch segment {
        case 0u: { return vec4<f32>(t, 0.0, CELL_WIDTH - t, t); }
        case 1u: { return vec4<f32>(CELL_WIDTH - t, t, CELL_WIDTH, mid); }
        case 2u: { return vec4<f32>(CELL_WIDTH - t, mid, CELL_WIDTH, DIGIT_HEIGHT - t); }
        case 3u: { return vec4<f32>(t, DIGIT_HEIGHT - t, CELL_WIDTH - t, DIGIT_HEIGHT); }
        case 4u: { return vec4<f32>(0.0, mid, t, DIGIT_HEIGHT - t); }
        case 5u: { return vec4<f32>(0.0, t, t, mid); }
        case 6u: { return vec4<f32>(t, mid - t * 0.5, CELL_WIDTH - t, mid + t * 0.5); }
        default: { return vec4<f32>(0.0); }
    }
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

fn circle_alpha(p: vec2<f32>, center: vec2<f32>, radius: f32, feather: f32) -> f32 {
    return 1.0 - smoothstep(0.0, feather, length(p - center) - radius);
}

// Coverage of one seven-segment digit with its origin at `origin`.
fn digit_alpha(p: vec2<f32>, origin: vec2<f32>, digit: u32, feather: f32) -> f32 {
    if digit > 9u {
        return 0.0;
    }

    let local = p - origin;
    let mask = DIGIT_MASKS[digit];
    var alpha = 0.0;

    for (var segment = 0u; segment < 7u; segment = segment + 1u) {
        if (mask & (1u << segment)) != 0u {
            let r = segment_rect(segment);
            alpha = max(alpha, rect_alpha(local, r.xy, r.zw, feather));
        }
    }

    return alpha;
}

// Coverage of a colon or dot separator with its origin at `origin`.
fn separator_alpha(p: vec2<f32>, origin: vec2<f32>, code: u32, feather: f32) -> f32 {
    let local = p - origin;
    let center_x = char_width(code) * 0.5;

    if code == COLON_CODE {
        let r = 0.095;
        let top = circle_alpha(local, vec2<f32>(center_x, DIGIT_HEIGHT * 0.38), r, feather);
        let bottom = circle_alpha(local, vec2<f32>(center_x, DIGIT_HEIGHT * 0.62), r, feather);
        return max(top, bottom);
    }

    if code == DOT_CODE {
        return circle_alpha(local, vec2<f32>(center_x, DIGIT_HEIGHT - 0.095), 0.08, feather);
    }

    return 0.0;
}

// Coverage of the twelve clock characters.
fn chars_alpha(p: vec2<f32>, feather: f32) -> f32 {
    // Skip the character loop outside the digit row.
    if p.x < -feather || p.x > TOTAL_WIDTH + feather
        || p.y < -feather || p.y > DIGIT_HEIGHT + feather {
        return 0.0;
    }

    var cursor = 0.0;
    var alpha = 0.0;

    for (var index = 0u; index < CHAR_COUNT; index = index + 1u) {
        let code = char_at(index);
        let origin = vec2<f32>(cursor, 0.0);

        if code < 10u {
            alpha = max(alpha, digit_alpha(p, origin, code, feather));
        } else {
            alpha = max(alpha, separator_alpha(p, origin, code, feather));
        }

        cursor = cursor + char_width(code) + GAP;
    }

    return alpha;
}

// Coverage of the millisecond grid: filled cells in x, unfilled cells
// in y. Each row fills to show one millisecond digit.
fn grid_alpha(p: vec2<f32>, feather: f32) -> vec2<f32> {
    let grid_origin =
        vec2<f32>((TOTAL_WIDTH - GRID_WIDTH) * 0.5, DIGIT_HEIGHT + GRID_TOP_GAP);

    // Skip the cell loop outside the grid.
    let local = p - grid_origin;
    if local.x < -feather || local.x > GRID_WIDTH + feather
        || local.y < -feather || local.y > GRID_HEIGHT + feather {
        return vec2<f32>(0.0, 0.0);
    }

    var filled = 0.0;
    var unfilled = 0.0;

    for (var row = 0u; row < GRID_ROWS; row = row + 1u) {
        let row_digit = char_at(9u + row);
        for (var column = 0u; column < GRID_COLUMNS; column = column + 1u) {
            let cell_origin = grid_origin + vec2<f32>(
                f32(column) * (GRID_CELL + GRID_COLUMN_GAP),
                f32(row) * (GRID_CELL + GRID_ROW_GAP),
            );
            let cell_alpha = rect_alpha(p, cell_origin, cell_origin + vec2<f32>(GRID_CELL), feather);
            if column < row_digit {
                filled = max(filled, cell_alpha);
            } else {
                unfilled = max(unfilled, cell_alpha);
            }
        }
    }

    return vec2<f32>(filled, unfilled);
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    // Work in a space that is `aspect` wide and 1.0 tall.
    let height = max(clock.viewport_size.y, 1.0);
    let aspect = max(clock.viewport_size.x / height, 0.1);
    let p = vec2<f32>(in.uv.x * aspect, in.uv.y);

    // Fit the clock into the frame with a margin, and center it.
    let scale = min((aspect * 0.94) / TOTAL_WIDTH, 0.82 / GROUP_HEIGHT);
    let scaled_size = vec2<f32>(TOTAL_WIDTH, GROUP_HEIGHT) * scale;
    let origin = vec2<f32>((aspect - scaled_size.x) * 0.5, (1.0 - scaled_size.y) * 0.5);
    let local_p = (p - origin) / scale;

    // About 1.5 output pixels of edge feather, in layout units.
    let feather = 1.5 / (height * scale);

    let chars = chars_alpha(local_p, feather);
    let grid = grid_alpha(local_p, feather);
    let alpha = max(chars, grid.x);

    let color = max(EMPTY_CELL * grid.y, FOREGROUND * alpha);
    return vec4<f32>(color, 1.0);
}
