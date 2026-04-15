use anyhow::Result;

// ---------------------------------------------------------------------------
// Source mode
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum VideoSourceMode {
    Camera(u32),
    Static,
    Timecode,
}

pub fn parse_video_source(s: &str) -> Result<VideoSourceMode> {
    match s {
        "static" => Ok(VideoSourceMode::Static),
        "timecode" => Ok(VideoSourceMode::Timecode),
        other => {
            let idx: u32 = other.parse().map_err(|_| {
                anyhow::anyhow!(
                    "Invalid --camera-index '{}': use a number, 'static', or 'timecode'",
                    other
                )
            })?;
            Ok(VideoSourceMode::Camera(idx))
        }
    }
}

// ---------------------------------------------------------------------------
// Bitmap font (5×7, digits + colon)
// ---------------------------------------------------------------------------

const GLYPH_W: u32 = 5;
const GLYPH_H: u32 = 7;

#[rustfmt::skip]
const DIGIT_GLYPHS: [[u8; 7]; 10] = [
    [0x0E, 0x11, 0x13, 0x15, 0x19, 0x11, 0x0E], // 0
    [0x04, 0x0C, 0x04, 0x04, 0x04, 0x04, 0x0E], // 1
    [0x0E, 0x11, 0x01, 0x06, 0x08, 0x10, 0x1F], // 2
    [0x0E, 0x11, 0x01, 0x06, 0x01, 0x11, 0x0E], // 3
    [0x02, 0x06, 0x0A, 0x12, 0x1F, 0x02, 0x02], // 4
    [0x1F, 0x10, 0x1E, 0x01, 0x01, 0x11, 0x0E], // 5
    [0x0E, 0x11, 0x10, 0x1E, 0x11, 0x11, 0x0E], // 6
    [0x1F, 0x01, 0x02, 0x04, 0x08, 0x08, 0x08], // 7
    [0x0E, 0x11, 0x11, 0x0E, 0x11, 0x11, 0x0E], // 8
    [0x0E, 0x11, 0x11, 0x0F, 0x01, 0x11, 0x0E], // 9
];

const COLON_GLYPH: [u8; 7] = [0x00, 0x04, 0x04, 0x00, 0x04, 0x04, 0x00];

fn glyph_for(c: char) -> Option<&'static [u8; 7]> {
    match c {
        '0'..='9' => Some(&DIGIT_GLYPHS[(c as u8 - b'0') as usize]),
        ':' => Some(&COLON_GLYPH),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Colour helpers
// ---------------------------------------------------------------------------

/// BT.601 RGB → YCbCr
fn rgb_to_yuv(r: u8, g: u8, b: u8) -> (u8, u8, u8) {
    let (r, g, b) = (r as i32, g as i32, b as i32);
    let y = ((66 * r + 129 * g + 25 * b + 128) >> 8) + 16;
    let u = ((-38 * r - 74 * g + 112 * b + 128) >> 8) + 128;
    let v = ((112 * r - 94 * g - 18 * b + 128) >> 8) + 128;
    (y.clamp(0, 255) as u8, u.clamp(0, 255) as u8, v.clamp(0, 255) as u8)
}

// ---------------------------------------------------------------------------
// Pattern rendering (operates on raw I420 plane slices)
// ---------------------------------------------------------------------------

/// Fill I420 planes with 75 % SMPTE colour bars.
pub fn fill_color_bars(
    y_data: &mut [u8],
    u_data: &mut [u8],
    v_data: &mut [u8],
    stride_y: u32,
    stride_u: u32,
    stride_v: u32,
    width: u32,
    height: u32,
) {
    const BARS: [(u8, u8, u8); 7] = [
        (192, 192, 192), // white
        (192, 192, 0),   // yellow
        (0, 192, 192),   // cyan
        (0, 192, 0),     // green
        (192, 0, 192),   // magenta
        (192, 0, 0),     // red
        (0, 0, 192),     // blue
    ];
    let yuv: Vec<_> = BARS.iter().map(|&(r, g, b)| rgb_to_yuv(r, g, b)).collect();
    let bar_w = width / 7;
    for row in 0..height {
        for col in 0..width {
            let i = (col / bar_w).min(6) as usize;
            y_data[(row * stride_y + col) as usize] = yuv[i].0;
            if row % 2 == 0 && col % 2 == 0 {
                u_data[((row / 2) * stride_u + col / 2) as usize] = yuv[i].1;
                v_data[((row / 2) * stride_v + col / 2) as usize] = yuv[i].2;
            }
        }
    }
}

/// Render a single bitmap-font character (white on existing background).
fn draw_glyph(
    y_data: &mut [u8],
    u_data: &mut [u8],
    v_data: &mut [u8],
    stride_y: u32,
    stride_u: u32,
    stride_v: u32,
    c: char,
    x0: u32,
    y0: u32,
    scale: u32,
    frame_w: u32,
    frame_h: u32,
) {
    let glyph = match glyph_for(c) {
        Some(g) => g,
        None => return,
    };
    for gy in 0..GLYPH_H {
        let bits = glyph[gy as usize];
        for gx in 0..GLYPH_W {
            if (bits >> (GLYPH_W - 1 - gx)) & 1 == 0 {
                continue;
            }
            for sy in 0..scale {
                for sx in 0..scale {
                    let px = x0 + gx * scale + sx;
                    let py = y0 + gy * scale + sy;
                    if px >= frame_w || py >= frame_h {
                        continue;
                    }
                    y_data[(py * stride_y + px) as usize] = 235;
                    if px % 2 == 0 && py % 2 == 0 {
                        u_data[((py / 2) * stride_u + px / 2) as usize] = 128;
                        v_data[((py / 2) * stride_v + px / 2) as usize] = 128;
                    }
                }
            }
        }
    }
}

/// Draw HH:MM:SS:FF timecode and a vertical sweep line on the I420 planes.
pub fn render_timecode_overlay(
    y_data: &mut [u8],
    u_data: &mut [u8],
    v_data: &mut [u8],
    stride_y: u32,
    stride_u: u32,
    stride_v: u32,
    width: u32,
    height: u32,
    frame_count: u64,
    fps: u32,
) {
    let total_secs = frame_count / fps as u64;
    let ff = frame_count % fps as u64;
    let ss = total_secs % 60;
    let mm = (total_secs / 60) % 60;
    let hh = (total_secs / 3600) % 100;

    let tc = format!("{:02}:{:02}:{:02}:{:02}", hh, mm, ss, ff);

    let scale = (height / 120).max(1).min(12);
    let char_w = GLYPH_W * scale;
    let spacing = scale;
    let text_w = tc.len() as u32 * (char_w + spacing) - spacing;
    let char_h = GLYPH_H * scale;
    let text_x = width.saturating_sub(text_w) / 2;
    let text_y = height / 6;

    // Dark background rectangle behind timecode text
    let pad = scale * 2;
    let bx0 = text_x.saturating_sub(pad);
    let by0 = text_y.saturating_sub(pad);
    let bx1 = (text_x + text_w + pad).min(width);
    let by1 = (text_y + char_h + pad).min(height);
    for row in by0..by1 {
        for col in bx0..bx1 {
            y_data[(row * stride_y + col) as usize] = 16;
            if row % 2 == 0 && col % 2 == 0 {
                u_data[((row / 2) * stride_u + col / 2) as usize] = 128;
                v_data[((row / 2) * stride_v + col / 2) as usize] = 128;
            }
        }
    }

    let mut cx = text_x;
    for c in tc.chars() {
        draw_glyph(
            y_data, u_data, v_data, stride_y, stride_u, stride_v, c, cx, text_y, scale, width,
            height,
        );
        cx += char_w + spacing;
    }

    // Vertical sweep line moving left→right once per second
    let frac = ff as f64 / fps as f64;
    let sweep_x = (frac * width as f64) as u32;
    let lw = scale.max(2);
    for row in 0..height {
        for dx in 0..lw {
            let col = sweep_x + dx;
            if col >= width {
                break;
            }
            y_data[(row * stride_y + col) as usize] = 235;
            if row % 2 == 0 && col % 2 == 0 {
                u_data[((row / 2) * stride_u + col / 2) as usize] = 128;
                v_data[((row / 2) * stride_v + col / 2) as usize] = 128;
            }
        }
    }
}
