use livekit::webrtc::video_frame::I420Buffer;

// SMPTE-style color bars in I420, followed by a neutral grayscale ramp.
const BARS: [(u8, u8, u8); 8] = [
    (235, 128, 128), // white
    (210, 16, 146),  // yellow
    (170, 166, 16),  // cyan
    (145, 54, 34),   // green
    (107, 202, 222), // magenta
    (82, 90, 240),   // red
    (41, 240, 110),  // blue
    (16, 128, 128),  // black
];

pub(crate) fn draw(buffer: &mut I420Buffer, width: u32, height: u32) {
    let (stride_y, stride_u, stride_v) = buffer.strides();
    let (data_y, data_u, data_v) = buffer.data_mut();
    let stride_y = stride_y as usize;
    let stride_u = stride_u as usize;
    let stride_v = stride_v as usize;
    let width = width as usize;
    let height = height as usize;
    let top_height = height * 2 / 3;
    let chroma_width = (width + 1) / 2;
    let chroma_height = (height + 1) / 2;

    for y in 0..height {
        let row = &mut data_y[y * stride_y..y * stride_y + width];
        if y < top_height {
            for (x, pixel) in row.iter_mut().enumerate() {
                let bar = (x * BARS.len()) / width.max(1);
                *pixel = BARS[bar].0;
            }
        } else {
            for (x, pixel) in row.iter_mut().enumerate() {
                *pixel = (16 + (x * 219) / width.max(1)) as u8;
            }
        }
    }

    for y in 0..chroma_height {
        let row_u = &mut data_u[y * stride_u..y * stride_u + chroma_width];
        let row_v = &mut data_v[y * stride_v..y * stride_v + chroma_width];
        if y * 2 < top_height {
            for x in 0..chroma_width {
                let bar = ((x * 2) * BARS.len()) / width.max(1);
                row_u[x] = BARS[bar].1;
                row_v[x] = BARS[bar].2;
            }
        } else {
            row_u.fill(128);
            row_v.fill(128);
        }
    }
}
