use chrono::{DateTime, Datelike, Timelike, Utc};

const TIMESTAMP_TEXT_LEN: usize = 23; // YYYY-MM-DD HH:MM:SS:SSS
const TIMESTAMP_GLYPH_COUNT: usize = 13; // 0-9, :, -, space
const TIMESTAMP_GLYPH_WIDTH: usize = 5;
const TIMESTAMP_GLYPH_HEIGHT: usize = 7;
const TIMESTAMP_GLYPH_SCALE: usize = 4;
const TIMESTAMP_GLYPH_SPACING: usize = 2;
const TIMESTAMP_PADDING_X: usize = 4;
const TIMESTAMP_PADDING_Y: usize = 4;
const TIMESTAMP_MARGIN: usize = 8;
const TIMESTAMP_BG_LUMA: u8 = 16;
const TIMESTAMP_FG_LUMA: u8 = 235;
const TIMESTAMP_RASTER_WIDTH: usize = TIMESTAMP_GLYPH_WIDTH * TIMESTAMP_GLYPH_SCALE;
const TIMESTAMP_RASTER_HEIGHT: usize = TIMESTAMP_GLYPH_HEIGHT * TIMESTAMP_GLYPH_SCALE;
const TIMESTAMP_GLYPH_COLON: u8 = 10;
const TIMESTAMP_GLYPH_DASH: u8 = 11;
const TIMESTAMP_GLYPH_SPACE: u8 = 12;

type TimestampGlyph = [[u8; TIMESTAMP_RASTER_WIDTH]; TIMESTAMP_RASTER_HEIGHT];

const TIMESTAMP_GLYPH_PATTERNS: [[u8; TIMESTAMP_GLYPH_HEIGHT]; TIMESTAMP_GLYPH_COUNT] = [
    [0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110], // 0
    [0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110], // 1
    [0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111], // 2
    [0b11110, 0b00001, 0b00001, 0b01110, 0b00001, 0b00001, 0b11110], // 3
    [0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010], // 4
    [0b11111, 0b10000, 0b10000, 0b11110, 0b00001, 0b00001, 0b11110], // 5
    [0b01110, 0b10000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110], // 6
    [0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000], // 7
    [0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110], // 8
    [0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00001, 0b01110], // 9
    [0b00000, 0b00000, 0b00100, 0b00000, 0b00100, 0b00000, 0b00000], // :
    [0b00000, 0b00000, 0b00000, 0b01110, 0b00000, 0b00000, 0b00000], // -
    [0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b00000], // space
];

pub struct TimestampOverlay {
    glyphs: [TimestampGlyph; TIMESTAMP_GLYPH_COUNT],
    glyph_ids: [u8; TIMESTAMP_TEXT_LEN],
    box_x: usize,
    box_y: usize,
    box_width: usize,
    box_height: usize,
    text_x: usize,
    text_y: usize,
    enabled: bool,
}

impl TimestampOverlay {
    pub fn new(frame_width: u32, frame_height: u32) -> Self {
        let text_width = TIMESTAMP_TEXT_LEN * TIMESTAMP_RASTER_WIDTH
            + (TIMESTAMP_TEXT_LEN.saturating_sub(1)) * TIMESTAMP_GLYPH_SPACING;
        let box_width = text_width + TIMESTAMP_PADDING_X * 2;
        let box_height = TIMESTAMP_RASTER_HEIGHT + TIMESTAMP_PADDING_Y * 2;
        let frame_width = frame_width as usize;
        let frame_height = frame_height as usize;
        let enabled = frame_width >= box_width + TIMESTAMP_MARGIN
            && frame_height >= box_height + TIMESTAMP_MARGIN;
        let box_x = TIMESTAMP_MARGIN;
        let box_y = frame_height.saturating_sub(TIMESTAMP_MARGIN + box_height);

        Self {
            glyphs: rasterize_timestamp_glyphs(),
            glyph_ids: [0; TIMESTAMP_TEXT_LEN],
            box_x,
            box_y,
            box_width,
            box_height,
            text_x: box_x + TIMESTAMP_PADDING_X,
            text_y: box_y + TIMESTAMP_PADDING_Y,
            enabled,
        }
    }

    pub fn draw(&mut self, data_y: &mut [u8], stride_y: usize, timestamp_us: u64) {
        if !self.enabled {
            return;
        }

        format_timestamp_glyphs(timestamp_us, &mut self.glyph_ids);

        for row in 0..self.box_height {
            let row_start = (self.box_y + row) * stride_y + self.box_x;
            let row_end = row_start + self.box_width;
            data_y[row_start..row_end].fill(TIMESTAMP_BG_LUMA);
        }

        for (glyph_pos, glyph_id) in self.glyph_ids.iter().copied().enumerate() {
            let glyph = &self.glyphs[glyph_id as usize];
            let glyph_x =
                self.text_x + glyph_pos * (TIMESTAMP_RASTER_WIDTH + TIMESTAMP_GLYPH_SPACING);
            for (row, glyph_row) in glyph.iter().enumerate() {
                let row_start = (self.text_y + row) * stride_y + glyph_x;
                let row_end = row_start + TIMESTAMP_RASTER_WIDTH;
                data_y[row_start..row_end].copy_from_slice(glyph_row);
            }
        }
    }
}

fn rasterize_timestamp_glyphs() -> [TimestampGlyph; TIMESTAMP_GLYPH_COUNT] {
    let mut glyphs = [[[TIMESTAMP_BG_LUMA; TIMESTAMP_RASTER_WIDTH]; TIMESTAMP_RASTER_HEIGHT];
        TIMESTAMP_GLYPH_COUNT];

    for (glyph_idx, pattern) in TIMESTAMP_GLYPH_PATTERNS.iter().enumerate() {
        for (src_y, row_bits) in pattern.iter().copied().enumerate() {
            for scale_y in 0..TIMESTAMP_GLYPH_SCALE {
                let dst_row = &mut glyphs[glyph_idx][src_y * TIMESTAMP_GLYPH_SCALE + scale_y];
                for src_x in 0..TIMESTAMP_GLYPH_WIDTH {
                    let bit = 1 << (TIMESTAMP_GLYPH_WIDTH - 1 - src_x);
                    if row_bits & bit != 0 {
                        let dst_x = src_x * TIMESTAMP_GLYPH_SCALE;
                        dst_row[dst_x..dst_x + TIMESTAMP_GLYPH_SCALE].fill(TIMESTAMP_FG_LUMA);
                    }
                }
            }
        }
    }

    glyphs
}

fn format_timestamp_glyphs(timestamp_us: u64, out: &mut [u8; TIMESTAMP_TEXT_LEN]) {
    let Some(dt) = DateTime::<Utc>::from_timestamp_micros(timestamp_us as i64) else {
        out.fill(0);
        return;
    };

    write_four_digits(&mut out[0..4], dt.year_ce().1);
    out[4] = TIMESTAMP_GLYPH_DASH;
    write_two_digits(&mut out[5..7], dt.month());
    out[7] = TIMESTAMP_GLYPH_DASH;
    write_two_digits(&mut out[8..10], dt.day());
    out[10] = TIMESTAMP_GLYPH_SPACE;
    write_two_digits(&mut out[11..13], dt.hour());
    out[13] = TIMESTAMP_GLYPH_COLON;
    write_two_digits(&mut out[14..16], dt.minute());
    out[16] = TIMESTAMP_GLYPH_COLON;
    write_two_digits(&mut out[17..19], dt.second());
    out[19] = TIMESTAMP_GLYPH_COLON;
    write_three_digits(&mut out[20..23], dt.timestamp_subsec_millis());
}

fn write_two_digits(dst: &mut [u8], value: u32) {
    dst[0] = (value / 10) as u8;
    dst[1] = (value % 10) as u8;
}

fn write_three_digits(dst: &mut [u8], value: u32) {
    dst[0] = (value / 100) as u8;
    dst[1] = ((value / 10) % 10) as u8;
    dst[2] = (value % 10) as u8;
}

fn write_four_digits(dst: &mut [u8], value: u32) {
    dst[0] = ((value / 1_000) % 10) as u8;
    dst[1] = ((value / 100) % 10) as u8;
    dst[2] = ((value / 10) % 10) as u8;
    dst[3] = (value % 10) as u8;
}
