use clap::ValueEnum;

/// Selects the generated video test pattern.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub(crate) enum TestPatternMode {
    /// Static SMPTE-style 75% color bars.
    #[value(name = "0")]
    Static,
    /// Scrolling color bars with a moving checkerboard.
    #[value(name = "1")]
    Animated,
}

impl TestPatternMode {
    /// Returns a short description suitable for status logs.
    pub(crate) fn description(self) -> &'static str {
        match self {
            Self::Static => "static SMPTE 75% color bars",
            Self::Animated => "animated scrolling color bars and checkerboard",
        }
    }
}

/// Generates an SMPTE-style 75% color-bar pattern in I420 format.
pub(crate) struct TestPattern {
    width: usize,
    height: usize,
    chroma_width: usize,
    chroma_height: usize,
    y_plane: Vec<u8>,
    u_plane: Vec<u8>,
    v_plane: Vec<u8>,
    mode: TestPatternMode,
    frame_index: u64,
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
    /// Precomputes the base color bars for the requested resolution and mode.
    pub(crate) fn new(width: u32, height: u32, mode: TestPatternMode) -> Self {
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

        Self {
            width,
            height,
            chroma_width,
            chroma_height,
            y_plane,
            u_plane,
            v_plane,
            mode,
            frame_index: 0,
        }
    }

    /// Renders the next frame into the provided I420 destination planes.
    pub(crate) fn render(
        &mut self,
        data_y: &mut [u8],
        stride_y: i32,
        data_u: &mut [u8],
        stride_u: i32,
        data_v: &mut [u8],
        stride_v: i32,
    ) {
        match self.mode {
            TestPatternMode::Static => {
                copy_plane(data_y, stride_y as usize, &self.y_plane, self.width, self.height);
                copy_plane(
                    data_u,
                    stride_u as usize,
                    &self.u_plane,
                    self.chroma_width,
                    self.chroma_height,
                );
                copy_plane(
                    data_v,
                    stride_v as usize,
                    &self.v_plane,
                    self.chroma_width,
                    self.chroma_height,
                );
            }
            TestPatternMode::Animated => self.render_animated(
                data_y,
                stride_y as usize,
                data_u,
                stride_u as usize,
                data_v,
                stride_v as usize,
            ),
        }
        self.frame_index = self.frame_index.wrapping_add(1);
    }

    fn render_animated(
        &self,
        data_y: &mut [u8],
        stride_y: usize,
        data_u: &mut [u8],
        stride_u: usize,
        data_v: &mut [u8],
        stride_v: usize,
    ) {
        if self.width == 0 || self.height == 0 {
            return;
        }

        let bar_offset = animation_position(self.frame_index, self.width, 4);
        let box_size = (self.width.min(self.height) / 4).max(1);
        let box_x = animation_position(self.frame_index, self.width.saturating_sub(box_size), 7);
        let box_y = animation_position(self.frame_index, self.height.saturating_sub(box_size), 5);
        let checker_size = (box_size / 8).max(1);

        for row in 0..self.height {
            let row_start = row * stride_y;
            for col in 0..self.width {
                let source_col = (col + bar_offset) % self.width;
                let mut y = color_for_luma_column(source_col, self.width).y;
                if point_is_in_box(col, row, box_x, box_y, box_size) {
                    let checker_col = (col - box_x) / checker_size;
                    let checker_row = (row - box_y) / checker_size;
                    y = if (checker_col + checker_row).is_multiple_of(2) { 235 } else { 16 };
                }
                data_y[row_start + col] = y;
            }
        }

        for row in 0..self.chroma_height {
            let row_start_u = row * stride_u;
            let row_start_v = row * stride_v;
            for col in 0..self.chroma_width {
                let luma_col = (col * 2).min(self.width - 1);
                let luma_row = (row * 2).min(self.height - 1);
                let source_col = (luma_col + bar_offset) % self.width;
                let color = color_for_luma_column(source_col, self.width);
                let (u, v) = if point_is_in_box(luma_col, luma_row, box_x, box_y, box_size) {
                    (128, 128)
                } else {
                    (color.u, color.v)
                };
                data_u[row_start_u + col] = u;
                data_v[row_start_v + col] = v;
            }
        }
    }
}

fn animation_position(frame_index: u64, distance: usize, pixels_per_frame: usize) -> usize {
    if distance == 0 {
        return 0;
    }

    let period = distance as u128 * 2;
    let position = (frame_index as u128 * pixels_per_frame as u128) % period;
    if position <= distance as u128 {
        position as usize
    } else {
        (period - position) as usize
    }
}

fn point_is_in_box(col: usize, row: usize, box_x: usize, box_y: usize, box_size: usize) -> bool {
    col >= box_x && col < box_x + box_size && row >= box_y && row < box_y + box_size
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

#[cfg(test)]
mod tests {
    use super::*;

    fn render_frame(pattern: &mut TestPattern, width: usize, height: usize) -> Vec<u8> {
        let chroma_width = width.div_ceil(2);
        let chroma_height = height.div_ceil(2);
        let mut y = vec![0; width * height];
        let mut u = vec![0; chroma_width * chroma_height];
        let mut v = vec![0; chroma_width * chroma_height];
        pattern.render(
            &mut y,
            width as i32,
            &mut u,
            chroma_width as i32,
            &mut v,
            chroma_width as i32,
        );
        y.extend(u);
        y.extend(v);
        y
    }

    #[test]
    fn static_pattern_does_not_change_between_frames() {
        let mut pattern = TestPattern::new(64, 36, TestPatternMode::Static);

        let first = render_frame(&mut pattern, 64, 36);
        let second = render_frame(&mut pattern, 64, 36);

        assert_eq!(first, second);
    }

    #[test]
    fn animated_pattern_changes_between_frames() {
        let mut pattern = TestPattern::new(64, 36, TestPatternMode::Animated);

        let first = render_frame(&mut pattern, 64, 36);
        let second = render_frame(&mut pattern, 64, 36);

        assert_ne!(first, second);
    }

    #[test]
    fn animation_position_moves_back_and_forth() {
        let positions: Vec<_> = (0..=4).map(|frame| animation_position(frame, 2, 1)).collect();

        assert_eq!(positions, [0, 1, 2, 1, 0]);
    }
}
