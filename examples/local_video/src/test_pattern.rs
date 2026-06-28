/// Selects the generated test pattern.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TestPatternKind {
    /// Static SMPTE-style 75% color bars.
    StaticColorBars,
    /// Animated motion graphic for exercising video encoders.
    AnimatedGraphic,
}

/// Returned when a numeric test pattern selector is unsupported.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct UnsupportedTestPatternKind;

impl TryFrom<u8> for TestPatternKind {
    type Error = UnsupportedTestPatternKind;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::StaticColorBars),
            1 => Ok(Self::AnimatedGraphic),
            _ => Err(UnsupportedTestPatternKind),
        }
    }
}

impl TestPatternKind {
    /// Returns a short label for logs and help text.
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::StaticColorBars => "SMPTE 75% color bars",
            Self::AnimatedGraphic => "animated encoder exercise graphic",
        }
    }
}

/// Generates a test pattern in I420 format.
pub(super) struct TestPattern {
    width: usize,
    height: usize,
    chroma_width: usize,
    chroma_height: usize,
    frames: TestPatternFrames,
}

enum TestPatternFrames {
    Static(I420Frame),
    AnimatedCached(Vec<I420Frame>),
    AnimatedDynamic,
}

struct I420Frame {
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

const ANIMATED_CACHE_TARGET_FRAMES: usize = 60;
const ANIMATED_CACHE_MIN_FRAMES: usize = 2;
const ANIMATED_CACHE_MAX_BYTES: usize = 128 * 1024 * 1024;

const BARS: [I420Color; 7] = [
    rgb_to_i420(191, 191, 191), // white
    rgb_to_i420(191, 191, 0),   // yellow
    rgb_to_i420(0, 191, 191),   // cyan
    rgb_to_i420(0, 191, 0),     // green
    rgb_to_i420(191, 0, 191),   // magenta
    rgb_to_i420(191, 0, 0),     // red
    rgb_to_i420(0, 0, 191),     // blue
];

const ANIMATED_PALETTE: [I420Color; 6] = [
    rgb_to_i420(235, 64, 32),
    rgb_to_i420(64, 224, 72),
    rgb_to_i420(48, 128, 255),
    rgb_to_i420(245, 220, 64),
    rgb_to_i420(224, 72, 220),
    rgb_to_i420(64, 224, 224),
];

impl TestPattern {
    /// Precompute the reusable planes for the requested pattern and resolution.
    pub(super) fn new(width: u32, height: u32, kind: TestPatternKind) -> Self {
        let width = width as usize;
        let height = height as usize;
        let chroma_width = width.div_ceil(2);
        let chroma_height = height.div_ceil(2);

        let frames = match kind {
            TestPatternKind::StaticColorBars => TestPatternFrames::Static(color_bars_frame(
                width,
                height,
                chroma_width,
                chroma_height,
            )),
            TestPatternKind::AnimatedGraphic => {
                if let Some(frame_count) =
                    cached_animation_frame_count(width, height, chroma_width, chroma_height)
                {
                    TestPatternFrames::AnimatedCached(animated_frames(
                        width,
                        height,
                        chroma_width,
                        chroma_height,
                        frame_count,
                    ))
                } else {
                    TestPatternFrames::AnimatedDynamic
                }
            }
        };

        Self { width, height, chroma_width, chroma_height, frames }
    }

    /// Render the selected pattern into the provided I420 destination planes.
    pub(super) fn render(
        &self,
        data_y: &mut [u8],
        stride_y: i32,
        data_u: &mut [u8],
        stride_u: i32,
        data_v: &mut [u8],
        stride_v: i32,
        frame_index: u64,
    ) {
        match &self.frames {
            TestPatternFrames::Static(frame) => {
                frame.copy_to(
                    data_y,
                    stride_y as usize,
                    data_u,
                    stride_u as usize,
                    data_v,
                    stride_v as usize,
                    self.width,
                    self.height,
                    self.chroma_width,
                    self.chroma_height,
                );
            }
            TestPatternFrames::AnimatedCached(frames) => {
                let frame = &frames[(frame_index % frames.len() as u64) as usize];
                frame.copy_to(
                    data_y,
                    stride_y as usize,
                    data_u,
                    stride_u as usize,
                    data_v,
                    stride_v as usize,
                    self.width,
                    self.height,
                    self.chroma_width,
                    self.chroma_height,
                );
            }
            TestPatternFrames::AnimatedDynamic => {
                render_animated_pattern(
                    data_y,
                    stride_y as usize,
                    data_u,
                    stride_u as usize,
                    data_v,
                    stride_v as usize,
                    self.width,
                    self.height,
                    self.chroma_width,
                    self.chroma_height,
                    frame_index,
                );
            }
        }
    }
}

impl I420Frame {
    fn new(width: usize, height: usize, chroma_width: usize, chroma_height: usize) -> Self {
        Self {
            y_plane: vec![0; width * height],
            u_plane: vec![128; chroma_width * chroma_height],
            v_plane: vec![128; chroma_width * chroma_height],
        }
    }

    fn copy_to(
        &self,
        data_y: &mut [u8],
        stride_y: usize,
        data_u: &mut [u8],
        stride_u: usize,
        data_v: &mut [u8],
        stride_v: usize,
        width: usize,
        height: usize,
        chroma_width: usize,
        chroma_height: usize,
    ) {
        copy_plane(data_y, stride_y, &self.y_plane, width, height);
        copy_plane(data_u, stride_u, &self.u_plane, chroma_width, chroma_height);
        copy_plane(data_v, stride_v, &self.v_plane, chroma_width, chroma_height);
    }
}

fn color_bars_frame(
    width: usize,
    height: usize,
    chroma_width: usize,
    chroma_height: usize,
) -> I420Frame {
    let mut frame = I420Frame::new(width, height, chroma_width, chroma_height);

    for row in 0..height {
        let row_start = row * width;
        for col in 0..width {
            frame.y_plane[row_start + col] = color_for_luma_column(col, width).y;
        }
    }

    for row in 0..chroma_height {
        let row_start = row * chroma_width;
        for col in 0..chroma_width {
            let color = color_for_luma_column(col * 2, width);
            frame.u_plane[row_start + col] = color.u;
            frame.v_plane[row_start + col] = color.v;
        }
    }

    frame
}

fn cached_animation_frame_count(
    width: usize,
    height: usize,
    chroma_width: usize,
    chroma_height: usize,
) -> Option<usize> {
    let bytes_per_frame = i420_frame_len(width, height, chroma_width, chroma_height);
    if bytes_per_frame == 0 {
        return Some(1);
    }

    let max_frames = ANIMATED_CACHE_MAX_BYTES / bytes_per_frame;
    (max_frames >= ANIMATED_CACHE_MIN_FRAMES)
        .then_some(max_frames.min(ANIMATED_CACHE_TARGET_FRAMES))
}

fn animated_frames(
    width: usize,
    height: usize,
    chroma_width: usize,
    chroma_height: usize,
    frame_count: usize,
) -> Vec<I420Frame> {
    (0..frame_count)
        .map(|frame_index| {
            let mut frame = I420Frame::new(width, height, chroma_width, chroma_height);
            render_animated_pattern(
                &mut frame.y_plane,
                width,
                &mut frame.u_plane,
                chroma_width,
                &mut frame.v_plane,
                chroma_width,
                width,
                height,
                chroma_width,
                chroma_height,
                frame_index as u64,
            );
            frame
        })
        .collect()
}

fn i420_frame_len(width: usize, height: usize, chroma_width: usize, chroma_height: usize) -> usize {
    width
        .saturating_mul(height)
        .saturating_add(chroma_width.saturating_mul(chroma_height).saturating_mul(2))
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

fn render_animated_pattern(
    data_y: &mut [u8],
    stride_y: usize,
    data_u: &mut [u8],
    stride_u: usize,
    data_v: &mut [u8],
    stride_v: usize,
    width: usize,
    height: usize,
    chroma_width: usize,
    chroma_height: usize,
    frame_index: u64,
) {
    if width == 0 || height == 0 {
        return;
    }

    let frame = frame_index as usize;
    let tile = (width.min(height) / 10).clamp(16, 96);
    let sweep_x = frame.wrapping_mul(7) % width;
    let sweep_y = frame.wrapping_mul(5) % height;
    let box_w = (width / 5).clamp(32, 256).min(width);
    let box_h = (height / 4).clamp(24, 192).min(height);
    let box_x = bouncing_offset(frame.wrapping_mul(9), width.saturating_sub(box_w));
    let box_y = bouncing_offset(frame.wrapping_mul(5), height.saturating_sub(box_h));

    for row in 0..height {
        let dst_start = row * stride_y;
        for col in 0..width {
            let shifted_x = col.wrapping_add(sweep_x);
            let shifted_y = row.wrapping_add(sweep_y);
            let checker = ((shifted_x / tile) ^ (shifted_y / tile)) & 1;
            let ramp = if width > 1 { (col * 144) / (width - 1) } else { 0 };
            let diagonal = (col.wrapping_add(row).wrapping_add(frame.wrapping_mul(11)) % tile) < 3;
            let mut luma = 42 + ramp as i32 + (checker as i32 * 42);

            if diagonal {
                luma += 64;
            }
            if in_box(col, row, box_x, box_y, box_w, box_h) {
                luma = if ((col / 8) ^ (row / 8) ^ (frame / 2)) & 1 == 0 { 235 } else { 24 };
            }

            data_y[dst_start + col] = clamp_to_u8(luma);
        }
    }

    for row in 0..chroma_height {
        let dst_u_start = row * stride_u;
        let dst_v_start = row * stride_v;
        for col in 0..chroma_width {
            let luma_col = col * 2;
            let luma_row = row * 2;
            let color = if in_box(luma_col, luma_row, box_x, box_y, box_w, box_h) {
                ANIMATED_PALETTE[(frame / 4) % ANIMATED_PALETTE.len()]
            } else {
                let palette_index = ((luma_col.wrapping_add(sweep_x) / tile)
                    + (luma_row.wrapping_add(sweep_y) / tile)
                    + (frame / 12))
                    % ANIMATED_PALETTE.len();
                ANIMATED_PALETTE[palette_index]
            };
            data_u[dst_u_start + col] = color.u;
            data_v[dst_v_start + col] = color.v;
        }
    }
}

fn bouncing_offset(position: usize, travel: usize) -> usize {
    if travel == 0 {
        return 0;
    }

    let period = travel.saturating_mul(2);
    let phase = position % period;
    if phase <= travel {
        phase
    } else {
        period - phase
    }
}

fn in_box(col: usize, row: usize, box_x: usize, box_y: usize, box_w: usize, box_h: usize) -> bool {
    (box_x..box_x + box_w).contains(&col) && (box_y..box_y + box_h).contains(&row)
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

    fn render_frame(kind: TestPatternKind, frame_index: u64) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
        let pattern = TestPattern::new(64, 36, kind);
        let mut y = vec![0; 64 * 36];
        let mut u = vec![0; 32 * 18];
        let mut v = vec![0; 32 * 18];
        pattern.render(&mut y, 64, &mut u, 32, &mut v, 32, frame_index);
        (y, u, v)
    }

    #[test]
    fn test_pattern_kind_accepts_supported_numeric_selectors() {
        assert_eq!(TestPatternKind::try_from(0), Ok(TestPatternKind::StaticColorBars));
        assert_eq!(TestPatternKind::try_from(1), Ok(TestPatternKind::AnimatedGraphic));
        assert_eq!(TestPatternKind::try_from(2), Err(UnsupportedTestPatternKind));
    }

    #[test]
    fn animated_graphic_uses_cached_frames_when_memory_allows() {
        let pattern = TestPattern::new(64, 36, TestPatternKind::AnimatedGraphic);

        let TestPatternFrames::AnimatedCached(frames) = pattern.frames else {
            panic!("small animated pattern should use cached frames");
        };
        assert_eq!(frames.len(), ANIMATED_CACHE_TARGET_FRAMES);
    }

    #[test]
    fn animated_cache_is_bounded_by_memory_budget() {
        let frame_count = cached_animation_frame_count(1920, 1080, 960, 540)
            .expect("1080p should still cache multiple frames");

        assert!(frame_count >= ANIMATED_CACHE_MIN_FRAMES);
        assert!(frame_count < ANIMATED_CACHE_TARGET_FRAMES);
        assert!(frame_count * i420_frame_len(1920, 1080, 960, 540) <= ANIMATED_CACHE_MAX_BYTES);
    }

    #[test]
    fn very_large_animated_patterns_fall_back_to_dynamic_rendering() {
        assert_eq!(cached_animation_frame_count(16_384, 9_216, 8_192, 4_608), None);
    }

    #[test]
    fn static_color_bars_do_not_change_between_frames() {
        assert_eq!(
            render_frame(TestPatternKind::StaticColorBars, 0),
            render_frame(TestPatternKind::StaticColorBars, 24)
        );
    }

    #[test]
    fn animated_graphic_changes_between_frames() {
        assert_ne!(
            render_frame(TestPatternKind::AnimatedGraphic, 0),
            render_frame(TestPatternKind::AnimatedGraphic, 24)
        );
    }
}
