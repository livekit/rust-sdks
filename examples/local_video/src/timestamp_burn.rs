use chrono::{DateTime, Datelike, Timelike, Utc};
use std::time::{Duration, Instant};

const GLYPH_WIDTH: usize = 5;
const GLYPH_HEIGHT: usize = 7;
const GLYPH_SPACING: usize = 2;
const LINE_SPACING: usize = 4;
const PADDING_X: usize = 4;
const PADDING_Y: usize = 4;
const MARGIN: usize = 8;
const BG_LUMA: u8 = 16;
const FG_LUMA: u8 = 235;
#[allow(dead_code)]
const LATENCY_DISPLAY_UPDATE_INTERVAL: Duration = Duration::from_millis(100);
#[allow(dead_code)]
const LATENCY_DISPLAY_STALE_AFTER: Duration = Duration::from_secs(2);

/// Text scale used for burned-in timing metrics overlays.
#[allow(dead_code)]
pub(crate) const METRICS_OVERLAY_SCALE: usize = 3;

/// Holds a latency string that refreshes at a readable 10 Hz cadence.
#[allow(dead_code)]
#[derive(Default)]
pub(crate) struct LatencyDisplay {
    value: String,
    last_update: Option<Instant>,
}

#[allow(dead_code)]
impl LatencyDisplay {
    /// Return the latency string to display, refreshing it when the 10 Hz interval has elapsed.
    pub(crate) fn value(&mut self, now: Instant, latest_value: Option<String>) -> &str {
        let should_update = match self.last_update {
            Some(last_update) => now.duration_since(last_update) >= LATENCY_DISPLAY_UPDATE_INTERVAL,
            None => true,
        };

        if let Some(latest_value) = latest_value {
            if should_update {
                self.value = latest_value;
                self.last_update = Some(now);
            }
        } else if self.last_update.is_some_and(|last_update| {
            now.duration_since(last_update) >= LATENCY_DISPLAY_STALE_AFTER
        }) {
            self.value.clear();
            self.last_update = None;
        }

        if self.value.is_empty() {
            "NA"
        } else {
            self.value.as_str()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latency_display_starts_as_na_without_sample() {
        let mut display = LatencyDisplay::default();

        assert_eq!("NA", display.value(Instant::now(), None));
    }

    #[test]
    fn latency_display_keeps_last_value_through_transient_missing_sample() {
        let mut display = LatencyDisplay::default();
        let now = Instant::now();

        assert_eq!("12.3MS", display.value(now, Some("12.3MS".to_string())));
        assert_eq!("12.3MS", display.value(now + LATENCY_DISPLAY_UPDATE_INTERVAL, None));
        assert_eq!(
            "13.4MS",
            display.value(
                now + LATENCY_DISPLAY_UPDATE_INTERVAL + Duration::from_millis(1),
                Some("13.4MS".to_string())
            )
        );
    }

    #[test]
    fn latency_display_returns_to_na_after_stale_missing_samples() {
        let mut display = LatencyDisplay::default();
        let now = Instant::now();

        assert_eq!("12.3MS", display.value(now, Some("12.3MS".to_string())));
        assert_eq!("NA", display.value(now + LATENCY_DISPLAY_STALE_AFTER, None));
    }
}

#[allow(dead_code)]
pub struct TimestampOverlay {
    text: TextBurner,
}

#[allow(dead_code)]
impl TimestampOverlay {
    pub fn new(frame_width: u32, frame_height: u32) -> Self {
        Self { text: TextBurner::new_bottom_left(frame_width, frame_height, 4) }
    }

    pub fn draw(
        &mut self,
        data_y: &mut [u8],
        stride_y: usize,
        timestamp_us: u64,
        frame_id: Option<u32>,
    ) {
        let mut text = format_timestamp_us(timestamp_us);
        if let Some(frame_id) = frame_id {
            text.push(' ');
            text.push_str(&frame_id.to_string());
        }
        self.text.draw_lines(data_y, stride_y, &[text.as_str()]);
    }
}

pub struct TextBurner {
    frame_width: usize,
    frame_height: usize,
    x: usize,
    y: usize,
    scale: usize,
    anchor: TextAnchor,
}

#[allow(dead_code)]
#[derive(Clone, Copy)]
enum TextAnchor {
    TopLeft,
    BottomLeft,
}

impl TextBurner {
    #[allow(dead_code)]
    pub fn new_top_left(frame_width: u32, frame_height: u32, scale: usize) -> Self {
        Self::new(frame_width, frame_height, scale, TextAnchor::TopLeft)
    }

    fn new_bottom_left(frame_width: u32, frame_height: u32, scale: usize) -> Self {
        Self::new(frame_width, frame_height, scale, TextAnchor::BottomLeft)
    }

    fn new(frame_width: u32, frame_height: u32, scale: usize, anchor: TextAnchor) -> Self {
        Self {
            frame_width: frame_width as usize,
            frame_height: frame_height as usize,
            x: MARGIN,
            y: MARGIN,
            scale: scale.max(1),
            anchor,
        }
    }

    pub fn draw_lines(&self, data_y: &mut [u8], stride_y: usize, lines: &[&str]) {
        let Some((box_width, box_height)) = self.box_size(lines) else {
            return;
        };
        if self.frame_width < self.x + box_width || self.frame_height < self.y + box_height {
            return;
        }

        let box_x = self.x;
        let box_y = match self.anchor {
            TextAnchor::TopLeft => self.y,
            TextAnchor::BottomLeft => self.frame_height.saturating_sub(self.y + box_height),
        };

        for row in 0..box_height {
            let row_start = (box_y + row) * stride_y + box_x;
            let row_end = row_start + box_width;
            if row_end <= data_y.len() {
                data_y[row_start..row_end].fill(BG_LUMA);
            }
        }

        let text_x = box_x + PADDING_X;
        let text_y = box_y + PADDING_Y;
        let line_step = self.raster_height() + LINE_SPACING;
        for (line_idx, line) in lines.iter().enumerate() {
            self.draw_text(data_y, stride_y, text_x, text_y + line_idx * line_step, line);
        }
    }

    fn box_size(&self, lines: &[&str]) -> Option<(usize, usize)> {
        if lines.is_empty() {
            return None;
        }
        let widest_line = lines.iter().map(|line| line.chars().count()).max().unwrap_or(0);
        if widest_line == 0 {
            return None;
        }
        let text_width = self.text_width(widest_line);
        let text_height =
            lines.len() * self.raster_height() + lines.len().saturating_sub(1) * LINE_SPACING;
        Some((text_width + PADDING_X * 2, text_height + PADDING_Y * 2))
    }

    fn text_width(&self, chars: usize) -> usize {
        chars * self.raster_width() + chars.saturating_sub(1) * GLYPH_SPACING
    }

    fn raster_width(&self) -> usize {
        GLYPH_WIDTH * self.scale
    }

    fn raster_height(&self) -> usize {
        GLYPH_HEIGHT * self.scale
    }

    fn draw_text(&self, data_y: &mut [u8], stride_y: usize, x: usize, y: usize, text: &str) {
        for (glyph_idx, ch) in text.chars().enumerate() {
            let glyph_x = x + glyph_idx * (self.raster_width() + GLYPH_SPACING);
            self.draw_glyph(data_y, stride_y, glyph_x, y, ch);
        }
    }

    fn draw_glyph(&self, data_y: &mut [u8], stride_y: usize, x: usize, y: usize, ch: char) {
        let pattern = glyph_pattern(ch.to_ascii_uppercase());
        for (src_y, row_bits) in pattern.iter().copied().enumerate() {
            for scale_y in 0..self.scale {
                let row_start = (y + src_y * self.scale + scale_y) * stride_y + x;
                for src_x in 0..GLYPH_WIDTH {
                    let bit = 1 << (GLYPH_WIDTH - 1 - src_x);
                    let luma = if row_bits & bit != 0 { FG_LUMA } else { BG_LUMA };
                    let dst_x = row_start + src_x * self.scale;
                    let dst_end = dst_x + self.scale;
                    if dst_end <= data_y.len() {
                        data_y[dst_x..dst_end].fill(luma);
                    }
                }
            }
        }
    }
}

pub fn format_timestamp_us(timestamp_us: u64) -> String {
    DateTime::<Utc>::from_timestamp_micros(timestamp_us as i64)
        .map(|dt| {
            format!(
                "{:04}-{:02}-{:02} {:02}:{:02}:{:02}:{:03}",
                dt.year_ce().1,
                dt.month(),
                dt.day(),
                dt.hour(),
                dt.minute(),
                dt.second(),
                dt.timestamp_subsec_millis()
            )
        })
        .unwrap_or_else(|| format!("INVALID TIMESTAMP {timestamp_us}"))
}

fn glyph_pattern(ch: char) -> [u8; GLYPH_HEIGHT] {
    match ch {
        '0' => [0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110],
        '1' => [0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110],
        '2' => [0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111],
        '3' => [0b11110, 0b00001, 0b00001, 0b01110, 0b00001, 0b00001, 0b11110],
        '4' => [0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010],
        '5' => [0b11111, 0b10000, 0b10000, 0b11110, 0b00001, 0b00001, 0b11110],
        '6' => [0b01110, 0b10000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110],
        '7' => [0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000],
        '8' => [0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110],
        '9' => [0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00001, 0b01110],
        'A' => [0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001],
        'B' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110],
        'C' => [0b01110, 0b10001, 0b10000, 0b10000, 0b10000, 0b10001, 0b01110],
        'D' => [0b11110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b11110],
        'E' => [0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111],
        'F' => [0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000],
        'G' => [0b01110, 0b10001, 0b10000, 0b10111, 0b10001, 0b10001, 0b01110],
        'H' => [0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001],
        'I' => [0b01110, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110],
        'J' => [0b00111, 0b00010, 0b00010, 0b00010, 0b10010, 0b10010, 0b01100],
        'K' => [0b10001, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001],
        'L' => [0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111],
        'M' => [0b10001, 0b11011, 0b10101, 0b10101, 0b10001, 0b10001, 0b10001],
        'N' => [0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001, 0b10001],
        'O' => [0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110],
        'P' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000],
        'Q' => [0b01110, 0b10001, 0b10001, 0b10001, 0b10101, 0b10010, 0b01101],
        'R' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001],
        'S' => [0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110],
        'T' => [0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100],
        'U' => [0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110],
        'V' => [0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01010, 0b00100],
        'W' => [0b10001, 0b10001, 0b10001, 0b10101, 0b10101, 0b10101, 0b01010],
        'X' => [0b10001, 0b10001, 0b01010, 0b00100, 0b01010, 0b10001, 0b10001],
        'Y' => [0b10001, 0b10001, 0b01010, 0b00100, 0b00100, 0b00100, 0b00100],
        'Z' => [0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b11111],
        ':' => [0b00000, 0b00000, 0b00100, 0b00000, 0b00100, 0b00000, 0b00000],
        '-' => [0b00000, 0b00000, 0b00000, 0b01110, 0b00000, 0b00000, 0b00000],
        '.' => [0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b01100, 0b01100],
        '/' => [0b00001, 0b00010, 0b00010, 0b00100, 0b01000, 0b01000, 0b10000],
        ' ' => [0b00000; GLYPH_HEIGHT],
        _ => [0b00000; GLYPH_HEIGHT],
    }
}
