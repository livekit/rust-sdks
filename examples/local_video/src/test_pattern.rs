/// Generates a static SMPTE-style 75% color-bar pattern in I420 format.
pub struct TestPattern {
    width: usize,
    height: usize,
    chroma_width: usize,
    chroma_height: usize,
    y_plane: Vec<u8>,
    u_plane: Vec<u8>,
    v_plane: Vec<u8>,
}

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

        Self { width, height, chroma_width, chroma_height, y_plane, u_plane, v_plane }
    }

    /// Copy the precomputed pattern into the provided I420 destination planes.
    pub fn render(
        &self,
        data_y: &mut [u8],
        stride_y: i32,
        data_u: &mut [u8],
        stride_u: i32,
        data_v: &mut [u8],
        stride_v: i32,
    ) {
        copy_plane(data_y, stride_y as usize, &self.y_plane, self.width, self.height);
        copy_plane(data_u, stride_u as usize, &self.u_plane, self.chroma_width, self.chroma_height);
        copy_plane(data_v, stride_v as usize, &self.v_plane, self.chroma_width, self.chroma_height);
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

fn copy_plane(dst: &mut [u8], dst_stride: usize, src: &[u8], width: usize, height: usize) {
    if width == 0 || height == 0 {
        return;
    }

    if dst_stride == width {
        let len = width * height;
        dst[..len].copy_from_slice(&src[..len]);
        return;
    }

    for row in 0..height {
        let dst_start = row * dst_stride;
        let src_start = row * width;
        dst[dst_start..dst_start + width].copy_from_slice(&src[src_start..src_start + width]);
    }
}
