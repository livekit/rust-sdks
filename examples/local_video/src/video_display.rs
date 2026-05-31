use anyhow::Result;
use eframe::egui;
use eframe::wgpu::{self, util::DeviceExt};
use egui_wgpu as egui_wgpu_backend;
use egui_wgpu_backend::CallbackTrait;
use parking_lot::Mutex;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

use crate::codec_display::codec_with_implementation;
use crate::viewport_aspect::{self, AspectConstrainedViewport};

#[derive(Default)]
pub(crate) struct SharedYuv {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) y_bytes_per_row: u32,
    pub(crate) uv_bytes_per_row: u32,
    pub(crate) y: Vec<u8>,
    pub(crate) u: Vec<u8>,
    pub(crate) v: Vec<u8>,
    pub(crate) codec: String,
    pub(crate) codec_implementation: String,
    pub(crate) fps: f32,
    pub(crate) simulcast: bool,
    pub(crate) dirty: bool,
    pub(crate) timing_sample: Option<PublisherTimingSample>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct PublisherTimingSample {
    pub(crate) frame_id: Option<u32>,
    pub(crate) sensor_exposure_timestamp_us: u64,
    pub(crate) got_frame_buffer_timestamp_us: Option<u64>,
    pub(crate) encoder_upload_timestamp_us: Option<u64>,
    pub(crate) encoder_output_timestamp_us: Option<u64>,
    pub(crate) webrtc_packetize_timestamp_us: Option<u64>,
}

impl PublisherTimingSample {
    pub(crate) fn new(sensor_exposure_timestamp_us: u64, frame_id: Option<u32>) -> Self {
        Self {
            frame_id,
            sensor_exposure_timestamp_us,
            got_frame_buffer_timestamp_us: None,
            encoder_upload_timestamp_us: None,
            encoder_output_timestamp_us: None,
            webrtc_packetize_timestamp_us: None,
        }
    }

    pub(crate) fn is_complete(&self) -> bool {
        self.got_frame_buffer_timestamp_us.is_some()
            && self.encoder_upload_timestamp_us.is_some()
            && self.encoder_output_timestamp_us.is_some()
            && self.webrtc_packetize_timestamp_us.is_some()
    }
}

pub(crate) fn align_up(value: u32, alignment: u32) -> u32 {
    ((value + alignment - 1) / alignment) * alignment
}

pub(crate) fn resize_reused_buffer(buf: &mut Vec<u8>, len: usize) {
    if buf.len() != len {
        buf.resize(len, 0);
    }
}

pub(crate) fn pack_plane(
    src: &[u8],
    src_stride: u32,
    row_width: u32,
    rows: u32,
    dst_stride: u32,
    dst: &mut Vec<u8>,
) {
    resize_reused_buffer(dst, (dst_stride * rows) as usize);
    if src_stride == dst_stride {
        let len = (dst_stride * rows) as usize;
        if src.len() >= len {
            dst[..len].copy_from_slice(&src[..len]);
            return;
        }
    }
    if src_stride == row_width && dst_stride == row_width {
        let len = (row_width * rows) as usize;
        dst[..len].copy_from_slice(&src[..len]);
        return;
    }

    for row in 0..rows {
        let src_off = (row * src_stride) as usize;
        let dst_off = (row * dst_stride) as usize;
        let row_end = dst_off + row_width as usize;
        dst[dst_off..row_end].copy_from_slice(&src[src_off..src_off + row_width as usize]);
    }
}

pub(crate) fn pack_i420_into_shared(
    shared: &Arc<Mutex<SharedYuv>>,
    width: u32,
    height: u32,
    y: &[u8],
    y_stride: u32,
    u: &[u8],
    u_stride: u32,
    v: &[u8],
    v_stride: u32,
    timing_sample: Option<PublisherTimingSample>,
) -> bool {
    let uv_w = (width + 1) / 2;
    let uv_h = (height + 1) / 2;
    let y_bytes_per_row = align_up(width, 256);
    let uv_bytes_per_row = align_up(uv_w, 256);

    let mut s = shared.lock();
    if s.dirty {
        return false;
    }

    pack_plane(y, y_stride, width, height, y_bytes_per_row, &mut s.y);
    pack_plane(u, u_stride, uv_w, uv_h, uv_bytes_per_row, &mut s.u);
    pack_plane(v, v_stride, uv_w, uv_h, uv_bytes_per_row, &mut s.v);

    s.width = width;
    s.height = height;
    s.y_bytes_per_row = y_bytes_per_row;
    s.uv_bytes_per_row = uv_bytes_per_row;
    if let Some(timing_sample) = timing_sample {
        s.timing_sample = Some(timing_sample);
    }
    s.dirty = true;
    true
}

fn format_time_of_day_us(timestamp_us: u64) -> String {
    let total_millis = timestamp_us / 1_000;
    let millis = total_millis % 1_000;
    let total_seconds = total_millis / 1_000;
    let seconds = total_seconds % 60;
    let minutes = (total_seconds / 60) % 60;
    let hours = (total_seconds / 3_600) % 24;
    format!("{hours:02}:{minutes:02}:{seconds:02}:{millis:03}")
}

fn format_timing_delta_ms(timestamp_us: u64, base_timestamp_us: u64) -> String {
    let delta_us = i128::from(timestamp_us) - i128::from(base_timestamp_us);
    if delta_us == 0 {
        return "0.0ms".to_string();
    }
    format!("{:+.1}ms", delta_us as f64 / 1_000.0)
}

fn format_optional_timing_delta_ms(
    timestamp_us: Option<u64>,
    base_timestamp_us: Option<u64>,
) -> String {
    match (timestamp_us, base_timestamp_us) {
        (Some(timestamp_us), Some(base_timestamp_us)) => {
            format_timing_delta_ms(timestamp_us, base_timestamp_us)
        }
        _ => "+--.-ms".to_string(),
    }
}

fn format_latency_ms(end_timestamp_us: u64, start_timestamp_us: u64) -> String {
    end_timestamp_us
        .checked_sub(start_timestamp_us)
        .map_or_else(|| "NA".to_string(), |delta_us| format!("{:.1}ms", delta_us as f64 / 1_000.0))
}

const PUBLISHER_TIMING_LABEL_WIDTH: usize = 17;
const PUBLISHER_TIMING_TIMESTAMP_WIDTH: usize = 12;
const PUBLISHER_TIMING_DELTA_WIDTH: usize = 10;
const PUBLISHER_TIMING_VALUE_WIDTH: usize =
    PUBLISHER_TIMING_TIMESTAMP_WIDTH + 1 + PUBLISHER_TIMING_DELTA_WIDTH;
const PUBLISHER_TIMING_LINE_WIDTH: usize =
    PUBLISHER_TIMING_LABEL_WIDTH + 1 + PUBLISHER_TIMING_VALUE_WIDTH;
const PUBLISHER_TIMING_DISPLAY_UPDATE_INTERVAL: Duration = Duration::from_millis(100);

fn publisher_timing_label(label: &str) -> String {
    format!("{label}:")
}

fn publisher_timing_value_line(label: &str, value: &str) -> String {
    let label = publisher_timing_label(label);
    format!(
        "{label:<label_width$} {value:>value_width$}",
        label_width = PUBLISHER_TIMING_LABEL_WIDTH,
        value_width = PUBLISHER_TIMING_VALUE_WIDTH
    )
}

fn publisher_timing_line(label: &str, timestamp_us: Option<u64>, delta: &str) -> String {
    let label = publisher_timing_label(label);
    match timestamp_us {
        Some(timestamp_us) => format!(
            "{label:<label_width$} {timestamp:>timestamp_width$} {delta:>delta_width$}",
            timestamp = format_time_of_day_us(timestamp_us),
            delta = delta,
            label_width = PUBLISHER_TIMING_LABEL_WIDTH,
            timestamp_width = PUBLISHER_TIMING_TIMESTAMP_WIDTH,
            delta_width = PUBLISHER_TIMING_DELTA_WIDTH
        ),
        None => format!(
            "{label:<label_width$} {timestamp:>timestamp_width$} {delta:>delta_width$}",
            timestamp = "--:--:--:---",
            delta = "+--.-ms",
            label_width = PUBLISHER_TIMING_LABEL_WIDTH,
            timestamp_width = PUBLISHER_TIMING_TIMESTAMP_WIDTH,
            delta_width = PUBLISHER_TIMING_DELTA_WIDTH
        ),
    }
}

fn publisher_timing_frame_id_line(frame_id: Option<u32>) -> String {
    let frame_id = frame_id.map(|id| id.to_string()).unwrap_or_else(|| "NA".to_string());
    publisher_timing_value_line("frame ID", &frame_id)
}

fn build_publisher_timing_lines(
    sample: PublisherTimingSample,
    overlay_values: &PublisherTimingOverlayValues,
) -> Vec<String> {
    let base = sample.sensor_exposure_timestamp_us;
    vec![
        publisher_timing_frame_id_line(sample.frame_id),
        publisher_timing_line(
            "sensor exposure",
            Some(base),
            &overlay_values.deltas.sensor_exposure,
        ),
        publisher_timing_line(
            "got frame buffer",
            sample.got_frame_buffer_timestamp_us,
            &overlay_values.deltas.got_frame_buffer,
        ),
        publisher_timing_line(
            "encoder upload",
            sample.encoder_upload_timestamp_us,
            &overlay_values.deltas.encoder_upload,
        ),
        publisher_timing_line(
            "encoder output",
            sample.encoder_output_timestamp_us,
            &overlay_values.deltas.encoder_output,
        ),
        publisher_timing_line(
            "webrtc packetize",
            sample.webrtc_packetize_timestamp_us,
            &overlay_values.deltas.webrtc_packetize,
        ),
        publisher_timing_value_line("Exposure to Send", &overlay_values.exp2send_latency),
    ]
}

#[cfg(test)]
fn assert_publisher_timing_lines_are_stable(lines: &[String]) {
    assert!(lines.iter().all(|line| line.len() == PUBLISHER_TIMING_LINE_WIDTH));
}

fn video_size(shared: &Arc<Mutex<SharedYuv>>) -> Option<(u32, u32)> {
    let s = shared.lock();
    if s.width > 0 && s.height > 0 {
        Some((s.width, s.height))
    } else {
        None
    }
}

#[derive(Default)]
struct PublisherTimingOverlayState {
    displayed_timing_deltas: Option<PublisherTimingDeltaValues>,
    displayed_exp2send_latency: Option<String>,
    last_latency_update: Option<Instant>,
}

#[derive(Clone, Debug)]
struct PublisherTimingDeltaValues {
    sensor_exposure: String,
    got_frame_buffer: String,
    encoder_upload: String,
    encoder_output: String,
    webrtc_packetize: String,
}

impl PublisherTimingDeltaValues {
    fn from_sample(sample: PublisherTimingSample) -> Self {
        let base = sample.sensor_exposure_timestamp_us;
        Self {
            sensor_exposure: format_timing_delta_ms(base, base),
            got_frame_buffer: format_optional_timing_delta_ms(
                sample.got_frame_buffer_timestamp_us,
                Some(base),
            ),
            encoder_upload: format_optional_timing_delta_ms(
                sample.encoder_upload_timestamp_us,
                sample.got_frame_buffer_timestamp_us,
            ),
            encoder_output: format_optional_timing_delta_ms(
                sample.encoder_output_timestamp_us,
                sample.encoder_upload_timestamp_us,
            ),
            webrtc_packetize: format_optional_timing_delta_ms(
                sample.webrtc_packetize_timestamp_us,
                sample.encoder_output_timestamp_us,
            ),
        }
    }
}

struct PublisherTimingOverlayValues {
    deltas: PublisherTimingDeltaValues,
    exp2send_latency: String,
}

impl PublisherTimingOverlayState {
    fn overlay_values(
        &mut self,
        sample: PublisherTimingSample,
        now: Instant,
    ) -> PublisherTimingOverlayValues {
        let should_update = self.last_latency_update.map_or(true, |last_update| {
            now.duration_since(last_update) >= PUBLISHER_TIMING_DISPLAY_UPDATE_INTERVAL
        });

        if should_update {
            self.displayed_timing_deltas = Some(PublisherTimingDeltaValues::from_sample(sample));
            self.displayed_exp2send_latency =
                sample.webrtc_packetize_timestamp_us.map(|webrtc_packetize_timestamp_us| {
                    format_latency_ms(
                        webrtc_packetize_timestamp_us,
                        sample.sensor_exposure_timestamp_us,
                    )
                });
            self.last_latency_update = Some(now);
        }

        PublisherTimingOverlayValues {
            deltas: self
                .displayed_timing_deltas
                .clone()
                .unwrap_or_else(|| PublisherTimingDeltaValues::from_sample(sample)),
            exp2send_latency: self
                .displayed_exp2send_latency
                .clone()
                .unwrap_or_else(|| "NA".to_string()),
        }
    }
}

fn video_status_line(
    width: u32,
    height: u32,
    fps: f32,
    codec: &str,
    codec_implementation: &str,
    simulcast: bool,
) -> String {
    let codec = codec_with_implementation(codec, codec_implementation);
    if simulcast {
        format!("{}x{} {:.1}fps {codec} Simulcast", width, height, fps.max(0.0))
    } else {
        format!("{}x{} {:.1}fps {codec}", width, height, fps.max(0.0))
    }
}

fn publisher_overlay_lines(
    shared: &Arc<Mutex<SharedYuv>>,
    overlay_state: &mut PublisherTimingOverlayState,
    now: Instant,
) -> Option<Vec<String>> {
    let (status_line, sample) = {
        let s = shared.lock();
        if s.width == 0 || s.height == 0 {
            return None;
        }

        (
            video_status_line(
                s.width,
                s.height,
                s.fps,
                &s.codec,
                &s.codec_implementation,
                s.simulcast,
            ),
            s.timing_sample,
        )
    };

    let mut lines = vec![status_line];
    if let Some(sample) = sample {
        let overlay_values = overlay_state.overlay_values(sample, now);
        lines.extend(build_publisher_timing_lines(sample, &overlay_values));
    }
    Some(lines)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn timestamp_us(hour: u64, minute: u64, second: u64, millisecond: u64) -> u64 {
        (((hour * 3_600 + minute * 60 + second) * 1_000) + millisecond) * 1_000
    }

    fn overlay_values(
        sample: PublisherTimingSample,
        exp2send_latency: &str,
    ) -> PublisherTimingOverlayValues {
        PublisherTimingOverlayValues {
            deltas: PublisherTimingDeltaValues::from_sample(sample),
            exp2send_latency: exp2send_latency.to_string(),
        }
    }

    #[test]
    fn publisher_overlay_shows_status_without_timing() {
        let shared = Arc::new(Mutex::new(SharedYuv::default()));
        {
            let mut s = shared.lock();
            s.width = 1280;
            s.height = 720;
            s.codec = "H264".to_string();
            s.codec_implementation = "NVIDIA H264 Encoder".to_string();
            s.fps = 29.6;
            s.simulcast = true;
        }

        let mut overlay_state = PublisherTimingOverlayState::default();
        let lines = publisher_overlay_lines(&shared, &mut overlay_state, Instant::now())
            .expect("status overlay should render");

        assert_eq!(lines, vec!["1280x720 29.6fps H264 NVENC Simulcast"]);
    }

    #[test]
    fn publisher_timing_lines_match_requested_format() {
        let base = timestamp_us(1, 2, 3, 456);
        let sample = PublisherTimingSample {
            frame_id: Some(7),
            sensor_exposure_timestamp_us: base,
            got_frame_buffer_timestamp_us: Some(base + 32_400),
            encoder_upload_timestamp_us: Some(base + 35_500),
            encoder_output_timestamp_us: Some(base + 55_300),
            webrtc_packetize_timestamp_us: Some(base + 56_900),
        };

        let overlay_values = overlay_values(sample, "56.9ms");
        let lines = build_publisher_timing_lines(sample, &overlay_values);
        assert_publisher_timing_lines_are_stable(&lines);
        assert_eq!(
            lines,
            vec![
                "frame ID:                               7",
                "sensor exposure:  01:02:03:456      0.0ms",
                "got frame buffer: 01:02:03:488    +32.4ms",
                "encoder upload:   01:02:03:491     +3.1ms",
                "encoder output:   01:02:03:511    +19.8ms",
                "webrtc packetize: 01:02:03:512     +1.6ms",
                "Exposure to Send:                  56.9ms",
            ]
        );
    }

    #[test]
    fn publisher_timing_lines_use_placeholder_for_missing_async_stages() {
        let base = timestamp_us(1, 2, 3, 456);
        let mut sample = PublisherTimingSample::new(base, None);
        sample.got_frame_buffer_timestamp_us = Some(base + 32_400);

        let overlay_values = overlay_values(sample, "NA");
        let lines = build_publisher_timing_lines(sample, &overlay_values);
        assert_publisher_timing_lines_are_stable(&lines);
        assert_eq!(
            lines,
            vec![
                "frame ID:                              NA",
                "sensor exposure:  01:02:03:456      0.0ms",
                "got frame buffer: 01:02:03:488    +32.4ms",
                "encoder upload:   --:--:--:---    +--.-ms",
                "encoder output:   --:--:--:---    +--.-ms",
                "webrtc packetize: --:--:--:---    +--.-ms",
                "Exposure to Send:                      NA",
            ]
        );
    }

    #[test]
    fn publisher_timing_deltas_are_relative_to_previous_stage() {
        let base = timestamp_us(0, 0, 1, 0);
        let sample = PublisherTimingSample {
            frame_id: None,
            sensor_exposure_timestamp_us: base,
            got_frame_buffer_timestamp_us: Some(base + 1_500_000),
            encoder_upload_timestamp_us: Some(base + 1_600_000),
            encoder_output_timestamp_us: None,
            webrtc_packetize_timestamp_us: None,
        };

        let overlay_values = overlay_values(sample, "NA");
        let lines = build_publisher_timing_lines(sample, &overlay_values);
        assert_publisher_timing_lines_are_stable(&lines);
        assert_eq!(lines[2], "got frame buffer: 00:00:02:500  +1500.0ms");
        assert_eq!(lines[3], "encoder upload:   00:00:02:600   +100.0ms");
    }

    #[test]
    fn publisher_latency_formatter_rejects_negative_latency() {
        assert_eq!(format_latency_ms(900, 1_000), "NA");
    }

    #[test]
    fn publisher_timing_exp2send_latency_refreshes_at_ten_hz() {
        let mut overlay_state = PublisherTimingOverlayState::default();
        let now = Instant::now();

        let sample = PublisherTimingSample {
            frame_id: Some(1),
            sensor_exposure_timestamp_us: 1_000,
            got_frame_buffer_timestamp_us: Some(2_000),
            encoder_upload_timestamp_us: Some(5_100),
            encoder_output_timestamp_us: Some(24_900),
            webrtc_packetize_timestamp_us: Some(57_900),
        };
        let overlay_values = overlay_state.overlay_values(sample, now);
        assert_eq!(overlay_values.deltas.encoder_upload, "+3.1ms");
        assert_eq!(overlay_values.deltas.encoder_output, "+19.8ms");
        assert_eq!(overlay_values.deltas.webrtc_packetize, "+33.0ms");
        assert_eq!(overlay_values.exp2send_latency, "56.9ms");

        let sample = PublisherTimingSample {
            frame_id: Some(2),
            sensor_exposure_timestamp_us: 1_000_000,
            got_frame_buffer_timestamp_us: Some(1_001_000),
            encoder_upload_timestamp_us: Some(1_011_000),
            encoder_output_timestamp_us: Some(1_031_000),
            webrtc_packetize_timestamp_us: Some(1_100_000),
        };
        let overlay_values = overlay_state.overlay_values(sample, now + Duration::from_millis(99));
        assert_eq!(overlay_values.deltas.encoder_upload, "+3.1ms");
        assert_eq!(overlay_values.deltas.encoder_output, "+19.8ms");
        assert_eq!(overlay_values.deltas.webrtc_packetize, "+33.0ms");
        assert_eq!(overlay_values.exp2send_latency, "56.9ms");

        let overlay_values = overlay_state.overlay_values(sample, now + Duration::from_millis(100));
        assert_eq!(overlay_values.deltas.encoder_upload, "+10.0ms");
        assert_eq!(overlay_values.deltas.encoder_output, "+20.0ms");
        assert_eq!(overlay_values.deltas.webrtc_packetize, "+69.0ms");
        assert_eq!(overlay_values.exp2send_latency, "100.0ms");
    }
}

struct VideoApp {
    shared: Arc<Mutex<SharedYuv>>,
    ctrl_c_received: Arc<AtomicBool>,
    viewport: AspectConstrainedViewport,
    timing_overlay_state: PublisherTimingOverlayState,
}

impl eframe::App for VideoApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.ctrl_c_received.load(Ordering::Acquire) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }

        if let Some((width, height)) = video_size(&self.shared) {
            self.viewport.set_video_size(ctx, width, height);
        }

        egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx, |ui| {
            ui.ctx().request_repaint();

            let available = ui.available_size();
            let size = if let Some(aspect) = self.viewport.aspect() {
                let mut w = available.x.max(1.0);
                let mut h = (w / aspect).max(1.0);
                if h > available.y.max(1.0) {
                    h = available.y.max(1.0);
                    w = (h * aspect).max(1.0);
                }
                egui::vec2(w, h)
            } else {
                egui::vec2(available.x.max(1.0), available.y.max(1.0))
            };

            ui.with_layout(
                egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
                |ui| {
                    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
                    let cb = egui_wgpu_backend::Callback::new_paint_callback(
                        rect,
                        YuvPaintCallback { shared: self.shared.clone() },
                    );
                    ui.painter().add(cb);
                },
            );
        });

        egui::Area::new("publisher_overlay".into())
            .anchor(egui::Align2::LEFT_TOP, egui::vec2(10.0, 10.0))
            .interactable(false)
            .show(ctx, |ui| {
                let Some(lines) = publisher_overlay_lines(
                    &self.shared,
                    &mut self.timing_overlay_state,
                    Instant::now(),
                ) else {
                    return;
                };
                let has_timing = lines.len() > 1;
                let text = lines.join("\n");
                egui::Frame::NONE
                    .fill(egui::Color32::from_black_alpha(160))
                    .corner_radius(egui::CornerRadius::same(4))
                    .inner_margin(egui::Margin::same(6))
                    .show(ui, |ui| {
                        if has_timing {
                            ui.set_min_width(PUBLISHER_TIMING_LINE_WIDTH as f32 * 8.0);
                        }
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(text).monospace().color(egui::Color32::WHITE),
                            )
                            .extend(),
                        );
                    });
            });

        ctx.request_repaint_after(Duration::from_millis(16));
    }
}

pub(crate) fn run_display(
    title: &str,
    shared: Arc<Mutex<SharedYuv>>,
    ctrl_c_received: Arc<AtomicBool>,
    initial_aspect: Option<f32>,
) -> Result<()> {
    let app = VideoApp {
        shared,
        ctrl_c_received: ctrl_c_received.clone(),
        viewport: AspectConstrainedViewport::new(initial_aspect),
        timing_overlay_state: PublisherTimingOverlayState::default(),
    };
    let native_options = viewport_aspect::native_options(initial_aspect);
    let result = eframe::run_native(title, native_options, Box::new(|_| Ok(Box::new(app))));

    ctrl_c_received.store(true, Ordering::Release);

    result?;

    Ok(())
}

struct YuvPaintCallback {
    shared: Arc<Mutex<SharedYuv>>,
}

struct YuvGpuState {
    pipeline: wgpu::RenderPipeline,
    sampler: wgpu::Sampler,
    bind_layout: wgpu::BindGroupLayout,
    y_tex: wgpu::Texture,
    u_tex: wgpu::Texture,
    v_tex: wgpu::Texture,
    y_view: wgpu::TextureView,
    u_view: wgpu::TextureView,
    v_view: wgpu::TextureView,
    bind_group: wgpu::BindGroup,
    params_buf: wgpu::Buffer,
    y_pad_w: u32,
    uv_pad_w: u32,
    dims: (u32, u32),
    upload_y: Vec<u8>,
    upload_u: Vec<u8>,
    upload_v: Vec<u8>,
}

impl YuvGpuState {
    fn create_textures(
        device: &wgpu::Device,
        _width: u32,
        height: u32,
        y_pad_w: u32,
        uv_pad_w: u32,
    ) -> (
        wgpu::Texture,
        wgpu::Texture,
        wgpu::Texture,
        wgpu::TextureView,
        wgpu::TextureView,
        wgpu::TextureView,
    ) {
        let y_size = wgpu::Extent3d { width: y_pad_w, height, depth_or_array_layers: 1 };
        let uv_size =
            wgpu::Extent3d { width: uv_pad_w, height: (height + 1) / 2, depth_or_array_layers: 1 };
        let usage = wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING;
        let desc = |size: wgpu::Extent3d| wgpu::TextureDescriptor {
            label: Some("yuv_plane"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage,
            view_formats: &[],
        };
        let y_tex = device.create_texture(&desc(y_size));
        let u_tex = device.create_texture(&desc(uv_size));
        let v_tex = device.create_texture(&desc(uv_size));
        let y_view = y_tex.create_view(&wgpu::TextureViewDescriptor::default());
        let u_view = u_tex.create_view(&wgpu::TextureViewDescriptor::default());
        let v_view = v_tex.create_view(&wgpu::TextureViewDescriptor::default());
        (y_tex, u_tex, v_tex, y_view, u_view, v_view)
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct ParamsUniform {
    src_w: u32,
    src_h: u32,
    y_tex_w: u32,
    uv_tex_w: u32,
    yuv_layout: u32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
}

impl CallbackTrait for YuvPaintCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen_desc: &egui_wgpu_backend::ScreenDescriptor,
        _encoder: &mut wgpu::CommandEncoder,
        resources: &mut egui_wgpu_backend::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        let mut shared = self.shared.lock();

        if shared.width == 0 || shared.height == 0 {
            return Vec::new();
        }

        if resources.get::<YuvGpuState>().is_none() {
            let shader_src = include_str!("yuv_shader.wgsl");
            let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("yuv_shader"),
                source: wgpu::ShaderSource::Wgsl(shader_src.into()),
            });

            let bind_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("yuv_bind_layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 4,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: Some(
                                std::num::NonZeroU64::new(
                                    std::mem::size_of::<ParamsUniform>() as u64
                                )
                                .unwrap(),
                            ),
                        },
                        count: None,
                    },
                ],
            });

            let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("yuv_pipeline_layout"),
                bind_group_layouts: &[&bind_layout],
                push_constant_ranges: &[],
            });

            let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("yuv_pipeline"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    buffers: &[],
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_main"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: wgpu::TextureFormat::Bgra8Unorm,
                        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    strip_index_format: None,
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: None,
                    unclipped_depth: false,
                    polygon_mode: wgpu::PolygonMode::Fill,
                    conservative: false,
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState {
                    count: 1,
                    mask: !0,
                    alpha_to_coverage_enabled: false,
                },
                multiview: None,
                cache: None,
            });

            let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("yuv_sampler"),
                address_mode_u: wgpu::AddressMode::ClampToEdge,
                address_mode_v: wgpu::AddressMode::ClampToEdge,
                address_mode_w: wgpu::AddressMode::ClampToEdge,
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                mipmap_filter: wgpu::FilterMode::Nearest,
                ..Default::default()
            });

            let params_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("yuv_params"),
                contents: bytemuck::bytes_of(&ParamsUniform {
                    src_w: 1,
                    src_h: 1,
                    y_tex_w: 1,
                    uv_tex_w: 1,
                    yuv_layout: 0,
                    _pad0: 0,
                    _pad1: 0,
                    _pad2: 0,
                }),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });

            let (y_tex, u_tex, v_tex, y_view, u_view, v_view) =
                YuvGpuState::create_textures(device, 1, 1, 256, 256);
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("yuv_bind_group"),
                layout: &bind_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::Sampler(&sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&y_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(&u_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::TextureView(&v_view),
                    },
                    wgpu::BindGroupEntry { binding: 4, resource: params_buf.as_entire_binding() },
                ],
            });

            resources.insert(YuvGpuState {
                pipeline: render_pipeline,
                sampler,
                bind_layout,
                y_tex,
                u_tex,
                v_tex,
                y_view,
                u_view,
                v_view,
                bind_group,
                params_buf,
                y_pad_w: 256,
                uv_pad_w: 256,
                dims: (0, 0),
                upload_y: Vec::new(),
                upload_u: Vec::new(),
                upload_v: Vec::new(),
            });
        }
        let state = resources.get_mut::<YuvGpuState>().unwrap();

        let dims = (shared.width, shared.height);
        let upload_row_bytes = (shared.y_bytes_per_row, shared.uv_bytes_per_row);
        let has_dirty_frame = if shared.dirty {
            std::mem::swap(&mut state.upload_y, &mut shared.y);
            std::mem::swap(&mut state.upload_u, &mut shared.u);
            std::mem::swap(&mut state.upload_v, &mut shared.v);
            shared.dirty = false;
            true
        } else {
            false
        };
        drop(shared);

        if state.dims != dims {
            let y_pad_w = align_up(dims.0, 256);
            let uv_w = (dims.0 + 1) / 2;
            let uv_pad_w = align_up(uv_w, 256);
            let (y_tex, u_tex, v_tex, y_view, u_view, v_view) =
                YuvGpuState::create_textures(device, dims.0, dims.1, y_pad_w, uv_pad_w);
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("yuv_bind_group"),
                layout: &state.bind_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::Sampler(&state.sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&y_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(&u_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::TextureView(&v_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: state.params_buf.as_entire_binding(),
                    },
                ],
            });
            state.y_tex = y_tex;
            state.u_tex = u_tex;
            state.v_tex = v_tex;
            state.y_view = y_view;
            state.u_view = u_view;
            state.v_view = v_view;
            state.bind_group = bind_group;
            state.y_pad_w = y_pad_w;
            state.uv_pad_w = uv_pad_w;
            state.dims = dims;
        }

        if has_dirty_frame {
            let uv_w = (dims.0 + 1) / 2;
            let uv_h = (dims.1 + 1) / 2;

            if upload_row_bytes.0 >= dims.0 {
                queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &state.y_tex,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    &state.upload_y,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(upload_row_bytes.0),
                        rows_per_image: Some(dims.1),
                    },
                    wgpu::Extent3d { width: dims.0, height: dims.1, depth_or_array_layers: 1 },
                );
            }

            if upload_row_bytes.1 >= uv_w {
                queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &state.u_tex,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    &state.upload_u,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(upload_row_bytes.1),
                        rows_per_image: Some(uv_h),
                    },
                    wgpu::Extent3d { width: uv_w, height: uv_h, depth_or_array_layers: 1 },
                );
                queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &state.v_tex,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    &state.upload_v,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(upload_row_bytes.1),
                        rows_per_image: Some(uv_h),
                    },
                    wgpu::Extent3d { width: uv_w, height: uv_h, depth_or_array_layers: 1 },
                );
            }

            queue.write_buffer(
                &state.params_buf,
                0,
                bytemuck::bytes_of(&ParamsUniform {
                    src_w: dims.0,
                    src_h: dims.1,
                    y_tex_w: state.y_pad_w,
                    uv_tex_w: state.uv_pad_w,
                    yuv_layout: 0,
                    _pad0: 0,
                    _pad1: 0,
                    _pad2: 0,
                }),
            );
        }

        Vec::new()
    }

    fn paint(
        &self,
        _info: egui::PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'static>,
        resources: &egui_wgpu_backend::CallbackResources,
    ) {
        let Some(state) = resources.get::<YuvGpuState>() else {
            return;
        };
        if state.dims == (0, 0) {
            return;
        }

        render_pass.set_pipeline(&state.pipeline);
        render_pass.set_bind_group(0, &state.bind_group, &[]);
        render_pass.draw(0..3, 0..1);
    }
}
