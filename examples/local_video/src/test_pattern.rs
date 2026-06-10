/// Generates an animated SMPTE-style 75% color-bar pattern in I420 format.
///
/// The pattern is deterministically animated so encoders produce meaningful
/// inter-frame data instead of collapsing to near-zero bitrate: the bars
/// scroll horizontally, an inverted-luma block bounces across the frame and a
/// binary frame counter strip is rendered along the bottom edge.
pub struct TestPattern {
    width: usize,
    height: usize,
    chroma_width: usize,
    chroma_height: usize,
    y_plane: Vec<u8>,
    u_plane: Vec<u8>,
    v_plane: Vec<u8>,
    frame_idx: u64,
}

/// Horizontal scroll speed in luma pixels per frame (kept even for chroma alignment).
const SCROLL_PX_PER_FRAME: u64 = 4;
/// Edge length of the bouncing inverted-luma block in pixels.
const BLOCK_SIZE: usize = 64;
/// Height of the binary frame counter strip in pixels.
const COUNTER_STRIP_HEIGHT: usize = 8;
/// Width of one frame counter bit cell in pixels.
const COUNTER_BIT_WIDTH: usize = 8;

#[derive(Clone, Copy)]
struct I420Color {
    y: u8,
    u: u8,
    v: u8,
}

const BARS: [I420Color; 7] = [
    rgb_to_i420(191, 191, 191), // white
    rgb_to_i420(191, 191, 0),   // yellow
    rgb_to_i420(0, 191, 191),   // cyan
    rgb_to_i420(0, 191, 0),     // green
    rgb_to_i420(191, 0, 191),   // magenta
    rgb_to_i420(191, 0, 0),     // red
    rgb_to_i420(0, 0, 191),     // blue
];

impl TestPattern {
    /// Precompute a static SMPTE-style 75% color-bar pattern for the requested resolution.
    pub fn new(width: u32, height: u32) -> Self {
        let width = width as usize;
        let height = height as usize;
        let chroma_width = width.div_ceil(2);
        let chroma_height = height.div_ceil(2);
        let mut y_plane = vec![0; width * height];
        let mut u_plane = vec![128; chroma_width * chroma_height];
        let mut v_plane = vec![128; chroma_width * chroma_height];

        for row in 0..height {
            let row_start = row * width;
            for col in 0..width {
                y_plane[row_start + col] = color_for_luma_column(col, width).y;
            }
        }

        for row in 0..chroma_height {
            let row_start = row * chroma_width;
            for col in 0..chroma_width {
                let color = color_for_luma_column(col * 2, width);
                u_plane[row_start + col] = color.u;
                v_plane[row_start + col] = color.v;
            }
        }

        Self { width, height, chroma_width, chroma_height, y_plane, u_plane, v_plane, frame_idx: 0 }
    }

    /// Render the next animation frame into the provided I420 destination planes.
    pub fn render(
        &mut self,
        data_y: &mut [u8],
        stride_y: i32,
        data_u: &mut [u8],
        stride_u: i32,
        data_v: &mut [u8],
        stride_v: i32,
    ) {
        let offset = self.scroll_offset();
        copy_plane_rotated(data_y, stride_y as usize, &self.y_plane, self.width, self.height, offset);
        copy_plane_rotated(
            data_u,
            stride_u as usize,
            &self.u_plane,
            self.chroma_width,
            self.chroma_height,
            offset / 2,
        );
        copy_plane_rotated(
            data_v,
            stride_v as usize,
            &self.v_plane,
            self.chroma_width,
            self.chroma_height,
            offset / 2,
        );
        self.draw_bouncing_block(data_y, stride_y as usize);
        self.draw_counter_strip(data_y, stride_y as usize, data_u, stride_u as usize, data_v, stride_v as usize);
        self.frame_idx = self.frame_idx.wrapping_add(1);
    }

    /// Current horizontal scroll of the color bars, kept even so the half
    /// resolution chroma planes stay aligned with luma.
    fn scroll_offset(&self) -> usize {
        if self.width == 0 {
            return 0;
        }
        ((self.frame_idx * SCROLL_PX_PER_FRAME) % self.width as u64) as usize & !1
    }

    /// Invert luma inside a block bouncing along both axes.
    fn draw_bouncing_block(&self, data_y: &mut [u8], stride_y: usize) {
        let block = BLOCK_SIZE.min(self.width / 4).min(self.height / 4);
        if block == 0 {
            return;
        }
        let x_range = self.width - block;
        let y_range = self.height.saturating_sub(block + COUNTER_STRIP_HEIGHT);
        let x = bounce(self.frame_idx.wrapping_mul(3), x_range);
        let y = bounce(self.frame_idx.wrapping_mul(2), y_range);

        for row in y..y + block {
            let row_start = row * stride_y;
            for value in &mut data_y[row_start + x..row_start + x + block] {
                *value = 255 - *value;
            }
        }
    }

    /// Render the frame index as a binary strip of white/black cells along the
    /// bottom edge, most significant bit first.
    fn draw_counter_strip(
        &self,
        data_y: &mut [u8],
        stride_y: usize,
        data_u: &mut [u8],
        stride_u: usize,
        data_v: &mut [u8],
        stride_v: usize,
    ) {
        if self.height <= COUNTER_STRIP_HEIGHT {
            return;
        }
        let bits = (self.width / COUNTER_BIT_WIDTH).min(32);
        if bits == 0 {
            return;
        }

        for row in self.height - COUNTER_STRIP_HEIGHT..self.height {
            let row_start = row * stride_y;
            for bit in 0..bits {
                let set = (self.frame_idx >> (bits - 1 - bit)) & 1 == 1;
                let luma = if set { 235 } else { 16 };
                let col_start = row_start + bit * COUNTER_BIT_WIDTH;
                data_y[col_start..col_start + COUNTER_BIT_WIDTH].fill(luma);
            }
        }

        // neutral chroma under the strip for legibility
        let strip_chroma_rows = COUNTER_STRIP_HEIGHT.div_ceil(2);
        let strip_chroma_width = (bits * COUNTER_BIT_WIDTH).div_ceil(2).min(self.chroma_width);
        for row in self.chroma_height.saturating_sub(strip_chroma_rows)..self.chroma_height {
            data_u[row * stride_u..row * stride_u + strip_chroma_width].fill(128);
            data_v[row * stride_v..row * stride_v + strip_chroma_width].fill(128);
        }
    }
}

/// Triangle wave over [0, range] advancing with t, yielding a bounce.
fn bounce(t: u64, range: usize) -> usize {
    if range == 0 {
        return 0;
    }
    let period = (2 * range) as u64;
    let m = (t % period) as usize;
    if m <= range {
        m
    } else {
        2 * range - m
    }
}

const fn rgb_to_i420(r: u8, g: u8, b: u8) -> I420Color {
    let r = r as i32;
    let g = g as i32;
    let b = b as i32;
    I420Color {
        y: clamp_to_u8(((66 * r + 129 * g + 25 * b + 128) >> 8) + 16),
        u: clamp_to_u8(((-38 * r - 74 * g + 112 * b + 128) >> 8) + 128),
        v: clamp_to_u8(((112 * r - 94 * g - 18 * b + 128) >> 8) + 128),
    }
}

const fn clamp_to_u8(value: i32) -> u8 {
    if value < 0 {
        0
    } else if value > u8::MAX as i32 {
        u8::MAX
    } else {
        value as u8
    }
}

fn color_for_luma_column(col: usize, width: usize) -> I420Color {
    if width == 0 {
        return BARS[0];
    }

    let bar = (col * BARS.len()) / width;
    BARS[bar.min(BARS.len() - 1)]
}

/// Copy a plane shifted left by `offset` pixels with horizontal wrap-around.
fn copy_plane_rotated(
    dst: &mut [u8],
    dst_stride: usize,
    src: &[u8],
    width: usize,
    height: usize,
    offset: usize,
) {
    if width == 0 || height == 0 {
        return;
    }

    let offset = offset % width;
    for row in 0..height {
        let dst_start = row * dst_stride;
        let src_start = row * width;
        dst[dst_start..dst_start + width - offset]
            .copy_from_slice(&src[src_start + offset..src_start + width]);
        dst[dst_start + width - offset..dst_start + width]
            .copy_from_slice(&src[src_start..src_start + offset]);
    }
}
