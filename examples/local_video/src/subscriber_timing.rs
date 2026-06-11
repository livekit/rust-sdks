use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
    time::{Duration, Instant},
};

use livekit::track::{SubscribeTimingEvent, SubscribeTimingStage};
use log::info;
use parking_lot::Mutex;

const MAX_SUBSCRIBER_TIMING_SAMPLES: usize = 300;
const DISPLAY_UPDATE_INTERVAL: Duration = Duration::from_millis(100);
pub(crate) const TIMING_LINE_WIDTH: usize =
    TIMING_LABEL_WIDTH + 1 + TIMING_TIMESTAMP_WIDTH + 1 + TIMING_DELTA_WIDTH;

const TIMING_LABEL_WIDTH: usize = 22;
const TIMING_TIMESTAMP_WIDTH: usize = 12;
const TIMING_DELTA_WIDTH: usize = 10;
const TIMING_VALUE_WIDTH: usize = TIMING_TIMESTAMP_WIDTH + 1 + TIMING_DELTA_WIDTH;

#[derive(Clone, Default)]
pub(crate) struct SubscriberTimingHandle {
    inner: Arc<Mutex<SubscriberTimingState>>,
}

impl SubscriberTimingHandle {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn record_subscribe_event(&self, event: SubscribeTimingEvent) {
        self.inner.lock().record_subscribe_event(event);
    }

    pub(crate) fn record_frame_received_by_sink(
        &self,
        sensor_exposure_timestamp_us: u64,
        frame_id: Option<u32>,
        frame_sink_timestamp_us: u64,
    ) {
        self.inner.lock().record_frame_received_by_sink(
            sensor_exposure_timestamp_us,
            frame_id,
            frame_sink_timestamp_us,
        );
    }

    pub(crate) fn record_frame_selected_for_render(
        &self,
        sensor_exposure_timestamp_us: u64,
        frame_id: Option<u32>,
        frame_selected_timestamp_us: u64,
    ) {
        self.inner.lock().record_frame_selected_for_render(
            sensor_exposure_timestamp_us,
            frame_id,
            frame_selected_timestamp_us,
        );
    }

    pub(crate) fn record_frame_painted(
        &self,
        sensor_exposure_timestamp_us: u64,
        frame_id: Option<u32>,
        frame_prepare_timestamp_us: u64,
        frame_painted_timestamp_us: u64,
    ) {
        self.inner.lock().record_frame_painted(
            sensor_exposure_timestamp_us,
            frame_id,
            frame_prepare_timestamp_us,
            frame_painted_timestamp_us,
        );
    }

    pub(crate) fn display_overlay_lines(&self, now: Instant) -> Option<Vec<String>> {
        self.inner.lock().display_overlay_lines(now)
    }

    pub(crate) fn reset(&self) {
        self.inner.lock().reset();
    }
}

#[derive(Clone, Copy, Debug)]
struct SubscriberTimingSample {
    frame_id: Option<u32>,
    sensor_exposure_timestamp_us: u64,
    webrtc_receive_timestamp_us: Option<u64>,
    decoder_upload_timestamp_us: Option<u64>,
    decoder_output_timestamp_us: Option<u64>,
    frame_sink_timestamp_us: Option<u64>,
    frame_prepare_timestamp_us: Option<u64>,
    frame_painted_timestamp_us: Option<u64>,
}

impl SubscriberTimingSample {
    fn new(sensor_exposure_timestamp_us: u64, frame_id: Option<u32>) -> Self {
        Self {
            frame_id,
            sensor_exposure_timestamp_us,
            webrtc_receive_timestamp_us: None,
            decoder_upload_timestamp_us: None,
            decoder_output_timestamp_us: None,
            frame_sink_timestamp_us: None,
            frame_prepare_timestamp_us: None,
            frame_painted_timestamp_us: None,
        }
    }
}

#[derive(Default)]
struct SubscriberTimingState {
    samples: HashMap<u64, SubscriberTimingSample>,
    order: VecDeque<u64>,
    latest_display_sample: Option<SubscriberTimingSample>,
    render_latency_window: RenderLatencyWindow,
    displayed_timing_deltas: Option<SubscriberTimingDeltaValues>,
    displayed_exp2recv_latency: Option<String>,
    displayed_receive_to_render_latency: Option<String>,
    displayed_e2e_latency: Option<String>,
    last_latency_update: Option<Instant>,
}

impl SubscriberTimingState {
    fn record_subscribe_event(&mut self, event: SubscribeTimingEvent) {
        if event.capture_timestamp_us == 0 {
            return;
        }

        let updated_sample = {
            let sample = self.get_or_insert_sample(event.capture_timestamp_us, event.frame_id);
            match event.stage {
                SubscribeTimingStage::WebrtcReceive => {
                    sample.webrtc_receive_timestamp_us = Some(event.timestamp_us);
                }
                SubscribeTimingStage::DecoderUpload => {
                    sample.decoder_upload_timestamp_us = Some(event.timestamp_us);
                }
                SubscribeTimingStage::DecoderOutput => {
                    sample.decoder_output_timestamp_us = Some(event.timestamp_us);
                }
            }
            *sample
        };

        if self
            .latest_display_sample
            .is_some_and(|sample| sample.sensor_exposure_timestamp_us == event.capture_timestamp_us)
        {
            self.latest_display_sample = Some(updated_sample);
        }
    }

    fn record_frame_received_by_sink(
        &mut self,
        sensor_exposure_timestamp_us: u64,
        frame_id: Option<u32>,
        frame_sink_timestamp_us: u64,
    ) {
        let sample = self.get_or_insert_sample(sensor_exposure_timestamp_us, frame_id);
        sample.frame_sink_timestamp_us = Some(frame_sink_timestamp_us);
    }

    fn record_frame_selected_for_render(
        &mut self,
        sensor_exposure_timestamp_us: u64,
        frame_id: Option<u32>,
        frame_selected_timestamp_us: u64,
    ) {
        let sample = self.get_or_insert_sample(sensor_exposure_timestamp_us, frame_id);
        sample.frame_prepare_timestamp_us.get_or_insert(frame_selected_timestamp_us);
        sample.frame_painted_timestamp_us.get_or_insert(frame_selected_timestamp_us);
        self.latest_display_sample = Some(*sample);
    }

    fn record_frame_painted(
        &mut self,
        sensor_exposure_timestamp_us: u64,
        frame_id: Option<u32>,
        frame_prepare_timestamp_us: u64,
        frame_painted_timestamp_us: u64,
    ) {
        let sample = self.get_or_insert_sample(sensor_exposure_timestamp_us, frame_id);
        sample.frame_prepare_timestamp_us.get_or_insert(frame_prepare_timestamp_us);
        sample.frame_painted_timestamp_us = Some(frame_painted_timestamp_us);
        let sample = *sample;
        self.latest_display_sample = Some(sample);
        self.render_latency_window.record(sample, Instant::now());
    }

    fn display_sample(&self) -> Option<SubscriberTimingSample> {
        self.latest_display_sample
    }

    fn display_overlay_lines(&mut self, now: Instant) -> Option<Vec<String>> {
        let sample = self.display_sample()?;
        let overlay_values = self.overlay_values(sample, now);
        Some(build_timing_overlay_lines(sample, &overlay_values))
    }

    fn reset(&mut self) {
        *self = Self::default();
    }

    fn overlay_values(
        &mut self,
        sample: SubscriberTimingSample,
        now: Instant,
    ) -> SubscriberTimingOverlayValues {
        let should_update = self
            .last_latency_update
            .map_or(true, |last_update| now.duration_since(last_update) >= DISPLAY_UPDATE_INTERVAL);

        if should_update {
            self.displayed_timing_deltas = Some(SubscriberTimingDeltaValues::from_sample(sample));
            self.displayed_exp2recv_latency =
                sample.webrtc_receive_timestamp_us.map(|webrtc_receive_timestamp_us| {
                    format_latency_ms(
                        webrtc_receive_timestamp_us,
                        sample.sensor_exposure_timestamp_us,
                    )
                });
            self.displayed_receive_to_render_latency =
                sample.frame_painted_timestamp_us.and_then(|frame_painted_timestamp_us| {
                    sample.webrtc_receive_timestamp_us.map(|webrtc_receive_timestamp_us| {
                        format_latency_ms(frame_painted_timestamp_us, webrtc_receive_timestamp_us)
                    })
                });
            self.displayed_e2e_latency =
                sample.frame_painted_timestamp_us.map(|frame_painted_timestamp_us| {
                    format_latency_ms(
                        frame_painted_timestamp_us,
                        sample.sensor_exposure_timestamp_us,
                    )
                });
            self.last_latency_update = Some(now);
        }

        SubscriberTimingOverlayValues {
            deltas: self
                .displayed_timing_deltas
                .clone()
                .unwrap_or_else(|| SubscriberTimingDeltaValues::from_sample(sample)),
            exp2recv_latency: self
                .displayed_exp2recv_latency
                .clone()
                .unwrap_or_else(|| "NA".to_string()),
            receive_to_render_latency: self
                .displayed_receive_to_render_latency
                .clone()
                .unwrap_or_else(|| "NA".to_string()),
            e2e_latency: self.displayed_e2e_latency.clone().unwrap_or_else(|| "NA".to_string()),
        }
    }

    fn get_or_insert_sample(
        &mut self,
        sensor_exposure_timestamp_us: u64,
        frame_id: Option<u32>,
    ) -> &mut SubscriberTimingSample {
        if !self.samples.contains_key(&sensor_exposure_timestamp_us) {
            self.samples.insert(
                sensor_exposure_timestamp_us,
                SubscriberTimingSample::new(sensor_exposure_timestamp_us, frame_id),
            );
            self.order.push_back(sensor_exposure_timestamp_us);
            self.prune();
        }

        let sample = self
            .samples
            .get_mut(&sensor_exposure_timestamp_us)
            .expect("timing sample should exist after insertion");
        if frame_id.is_some() {
            sample.frame_id = frame_id;
        }
        sample
    }

    fn prune(&mut self) {
        while self.order.len() > MAX_SUBSCRIBER_TIMING_SAMPLES {
            if let Some(oldest) = self.order.pop_front() {
                self.samples.remove(&oldest);
                if self
                    .latest_display_sample
                    .is_some_and(|sample| sample.sensor_exposure_timestamp_us == oldest)
                {
                    self.latest_display_sample = None;
                }
            }
        }
    }
}

#[derive(Clone, Copy, Default)]
struct LatencyStats {
    count: u64,
    sum_us: u128,
    min_us: Option<u64>,
    max_us: Option<u64>,
}

impl LatencyStats {
    fn record_delta(&mut self, start_us: u64, end_us: u64) {
        let latency_us = end_us.saturating_sub(start_us);
        self.count += 1;
        self.sum_us += u128::from(latency_us);
        self.min_us = Some(self.min_us.map_or(latency_us, |min| min.min(latency_us)));
        self.max_us = Some(self.max_us.map_or(latency_us, |max| max.max(latency_us)));
    }

    fn avg_us(&self) -> Option<u64> {
        (self.count > 0).then(|| (self.sum_us / u128::from(self.count)) as u64)
    }
}

#[derive(Default)]
struct RenderLatencyWindow {
    receive_to_decode: LatencyStats,
    decode_to_sink: LatencyStats,
    sink_to_prepare: LatencyStats,
    decode_to_prepare: LatencyStats,
    prepare_to_paint: LatencyStats,
    receive_to_paint: LatencyStats,
    e2e: LatencyStats,
    last_log: Option<Instant>,
}

impl RenderLatencyWindow {
    fn record(&mut self, sample: SubscriberTimingSample, now: Instant) {
        let Some(frame_painted_timestamp_us) = sample.frame_painted_timestamp_us else {
            return;
        };

        if let (Some(webrtc_receive), Some(decoder_output)) =
            (sample.webrtc_receive_timestamp_us, sample.decoder_output_timestamp_us)
        {
            self.receive_to_decode.record_delta(webrtc_receive, decoder_output);
        }

        if let (Some(decoder_output), Some(frame_sink)) =
            (sample.decoder_output_timestamp_us, sample.frame_sink_timestamp_us)
        {
            self.decode_to_sink.record_delta(decoder_output, frame_sink);
        }

        if let (Some(frame_sink), Some(frame_prepare)) =
            (sample.frame_sink_timestamp_us, sample.frame_prepare_timestamp_us)
        {
            self.sink_to_prepare.record_delta(frame_sink, frame_prepare);
        }

        if let (Some(decoder_output), Some(frame_prepare)) =
            (sample.decoder_output_timestamp_us, sample.frame_prepare_timestamp_us)
        {
            self.decode_to_prepare.record_delta(decoder_output, frame_prepare);
        }

        if let Some(frame_prepare) = sample.frame_prepare_timestamp_us {
            self.prepare_to_paint.record_delta(frame_prepare, frame_painted_timestamp_us);
        }

        if let Some(webrtc_receive) = sample.webrtc_receive_timestamp_us {
            self.receive_to_paint.record_delta(webrtc_receive, frame_painted_timestamp_us);
        }

        self.e2e.record_delta(sample.sensor_exposure_timestamp_us, frame_painted_timestamp_us);

        if self
            .last_log
            .map_or(true, |last_log| now.duration_since(last_log) >= Duration::from_secs(2))
        {
            self.log_and_reset(now);
        }
    }

    fn log_and_reset(&mut self, now: Instant) {
        if self.e2e.count == 0 {
            self.last_log = Some(now);
            return;
        }

        info!(
            "Subscriber render latency: frames={}, receive_to_decode avg={} min={} max={}, decoder_to_sink avg={} min={} max={}, sink_to_prepare avg={} min={} max={}, decoder_to_prepare avg={} min={} max={}, prepare_to_paint avg={} min={} max={}, receive_to_paint avg={} min={} max={}, e2e avg={} min={} max={}",
            self.e2e.count,
            latency_log_value(self.receive_to_decode.avg_us()),
            latency_log_value(self.receive_to_decode.min_us),
            latency_log_value(self.receive_to_decode.max_us),
            latency_log_value(self.decode_to_sink.avg_us()),
            latency_log_value(self.decode_to_sink.min_us),
            latency_log_value(self.decode_to_sink.max_us),
            latency_log_value(self.sink_to_prepare.avg_us()),
            latency_log_value(self.sink_to_prepare.min_us),
            latency_log_value(self.sink_to_prepare.max_us),
            latency_log_value(self.decode_to_prepare.avg_us()),
            latency_log_value(self.decode_to_prepare.min_us),
            latency_log_value(self.decode_to_prepare.max_us),
            latency_log_value(self.prepare_to_paint.avg_us()),
            latency_log_value(self.prepare_to_paint.min_us),
            latency_log_value(self.prepare_to_paint.max_us),
            latency_log_value(self.receive_to_paint.avg_us()),
            latency_log_value(self.receive_to_paint.min_us),
            latency_log_value(self.receive_to_paint.max_us),
            latency_log_value(self.e2e.avg_us()),
            latency_log_value(self.e2e.min_us),
            latency_log_value(self.e2e.max_us),
        );

        *self = Self { last_log: Some(now), ..Self::default() };
    }
}

#[derive(Clone, Debug)]
struct SubscriberTimingDeltaValues {
    sensor_exposure: String,
    webrtc_receive: String,
    decoder_upload: String,
    decoder_output: String,
    frame_painted: String,
}

impl SubscriberTimingDeltaValues {
    fn from_sample(sample: SubscriberTimingSample) -> Self {
        let base = sample.sensor_exposure_timestamp_us;
        Self {
            sensor_exposure: format_timing_delta_ms(base, base),
            webrtc_receive: format_optional_timing_delta_ms(
                sample.webrtc_receive_timestamp_us,
                Some(base),
            ),
            decoder_upload: format_optional_timing_delta_ms(
                sample.decoder_upload_timestamp_us,
                sample.webrtc_receive_timestamp_us,
            ),
            decoder_output: format_optional_timing_delta_ms(
                sample.decoder_output_timestamp_us,
                sample.decoder_upload_timestamp_us,
            ),
            frame_painted: format_optional_timing_delta_ms(
                sample.frame_painted_timestamp_us,
                sample.decoder_output_timestamp_us,
            ),
        }
    }
}

struct SubscriberTimingOverlayValues {
    deltas: SubscriberTimingDeltaValues,
    exp2recv_latency: String,
    receive_to_render_latency: String,
    e2e_latency: String,
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

fn latency_log_value(latency_us: Option<u64>) -> String {
    latency_us.map_or_else(
        || "NA".to_string(),
        |latency_us| format!("{:.1}ms", latency_us as f64 / 1_000.0),
    )
}

fn timing_value_line(label: &str, value: &str) -> String {
    let label = format!("{label}:");
    format!(
        "{label:<label_width$} {value:>value_width$}",
        label_width = TIMING_LABEL_WIDTH,
        value_width = TIMING_VALUE_WIDTH
    )
}

fn timing_line(label: &str, timestamp_us: Option<u64>, delta: &str) -> String {
    let label = format!("{label}:");
    match timestamp_us {
        Some(timestamp_us) => format!(
            "{label:<label_width$} {timestamp:>timestamp_width$} {delta:>delta_width$}",
            timestamp = format_time_of_day_us(timestamp_us),
            delta = delta,
            label_width = TIMING_LABEL_WIDTH,
            timestamp_width = TIMING_TIMESTAMP_WIDTH,
            delta_width = TIMING_DELTA_WIDTH
        ),
        None => format!(
            "{label:<label_width$} {timestamp:>timestamp_width$} {delta:>delta_width$}",
            timestamp = "--:--:--:---",
            delta = "+--.-ms",
            label_width = TIMING_LABEL_WIDTH,
            timestamp_width = TIMING_TIMESTAMP_WIDTH,
            delta_width = TIMING_DELTA_WIDTH
        ),
    }
}

fn build_timing_overlay_lines(
    sample: SubscriberTimingSample,
    overlay_values: &SubscriberTimingOverlayValues,
) -> Vec<String> {
    let base = sample.sensor_exposure_timestamp_us;
    let frame_id = sample.frame_id.map(|id| id.to_string()).unwrap_or_else(|| "NA".to_string());
    let mut lines = vec![
        timing_value_line("Frame ID", &frame_id),
        timing_line("sensor exposure", Some(base), &overlay_values.deltas.sensor_exposure),
        timing_line(
            "webrtc receive",
            sample.webrtc_receive_timestamp_us,
            &overlay_values.deltas.webrtc_receive,
        ),
        timing_line(
            "decoder upload",
            sample.decoder_upload_timestamp_us,
            &overlay_values.deltas.decoder_upload,
        ),
        timing_line(
            "decoder output",
            sample.decoder_output_timestamp_us,
            &overlay_values.deltas.decoder_output,
        ),
        timing_line(
            "frame painted",
            sample.frame_painted_timestamp_us,
            &overlay_values.deltas.frame_painted,
        ),
    ];
    lines.extend([
        timing_value_line("Exposure to Receive", &overlay_values.exp2recv_latency),
        timing_value_line("Receive to Render", &overlay_values.receive_to_render_latency),
        timing_value_line("e2e latency", &overlay_values.e2e_latency),
    ]);
    lines
}

#[cfg(test)]
fn assert_timing_lines_are_stable(lines: &[String]) {
    assert!(lines.iter().all(|line| line.len() == TIMING_LINE_WIDTH));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn timestamp_us(hour: u64, minute: u64, second: u64, millisecond: u64) -> u64 {
        (((hour * 3_600 + minute * 60 + second) * 1_000) + millisecond) * 1_000
    }

    fn subscribe_event(
        stage: SubscribeTimingStage,
        capture_timestamp_us: u64,
        timestamp_us: u64,
    ) -> SubscribeTimingEvent {
        SubscribeTimingEvent { stage, timestamp_us, capture_timestamp_us, frame_id: Some(123) }
    }

    fn overlay_values(
        sample: SubscriberTimingSample,
        exp2recv_latency: &str,
        receive_to_render_latency: &str,
        e2e_latency: &str,
    ) -> SubscriberTimingOverlayValues {
        SubscriberTimingOverlayValues {
            deltas: SubscriberTimingDeltaValues::from_sample(sample),
            exp2recv_latency: exp2recv_latency.to_string(),
            receive_to_render_latency: receive_to_render_latency.to_string(),
            e2e_latency: e2e_latency.to_string(),
        }
    }

    #[test]
    fn subscriber_timing_lines_match_requested_format() {
        let base = timestamp_us(1, 2, 3, 456);
        let sample = SubscriberTimingSample {
            frame_id: Some(123),
            sensor_exposure_timestamp_us: base,
            webrtc_receive_timestamp_us: Some(base + 32_400),
            decoder_upload_timestamp_us: Some(base + 35_500),
            decoder_output_timestamp_us: Some(base + 55_300),
            frame_sink_timestamp_us: Some(base + 55_900),
            frame_prepare_timestamp_us: Some(base + 56_100),
            frame_painted_timestamp_us: Some(base + 56_900),
        };

        let overlay_values = overlay_values(sample, "32.4ms", "24.5ms", "56.9ms");
        let lines = build_timing_overlay_lines(sample, &overlay_values);
        assert_timing_lines_are_stable(&lines);
        assert_eq!(
            lines,
            vec![
                "Frame ID:                                  123",
                "sensor exposure:       01:02:03:456      0.0ms",
                "webrtc receive:        01:02:03:488    +32.4ms",
                "decoder upload:        01:02:03:491     +3.1ms",
                "decoder output:        01:02:03:511    +19.8ms",
                "frame painted:         01:02:03:512     +1.6ms",
                "Exposure to Receive:                    32.4ms",
                "Receive to Render:                      24.5ms",
                "e2e latency:                            56.9ms",
            ]
        );
    }

    #[test]
    fn subscriber_timing_lines_use_placeholders_for_missing_stages() {
        let base = timestamp_us(1, 2, 3, 456);
        let sample = SubscriberTimingSample::new(base, None);

        let overlay_values = overlay_values(sample, "NA", "NA", "NA");
        let lines = build_timing_overlay_lines(sample, &overlay_values);
        assert_timing_lines_are_stable(&lines);
        assert_eq!(
            lines,
            vec![
                "Frame ID:                                   NA",
                "sensor exposure:       01:02:03:456      0.0ms",
                "webrtc receive:        --:--:--:---    +--.-ms",
                "decoder upload:        --:--:--:---    +--.-ms",
                "decoder output:        --:--:--:---    +--.-ms",
                "frame painted:         --:--:--:---    +--.-ms",
                "Exposure to Receive:                        NA",
                "Receive to Render:                          NA",
                "e2e latency:                                NA",
            ]
        );
    }

    #[test]
    fn subscriber_latency_formatter_rejects_negative_latency() {
        assert_eq!(format_latency_ms(900, 1_000), "NA");
    }

    #[test]
    fn subscriber_timing_state_uses_selection_timestamp_until_paint_callback() {
        let mut state = SubscriberTimingState::default();
        state.record_subscribe_event(subscribe_event(
            SubscribeTimingStage::WebrtcReceive,
            1_000,
            1_200,
        ));
        state.record_subscribe_event(subscribe_event(
            SubscribeTimingStage::DecoderUpload,
            1_000,
            1_300,
        ));
        state.record_subscribe_event(subscribe_event(
            SubscribeTimingStage::DecoderOutput,
            1_000,
            1_400,
        ));

        state.record_frame_selected_for_render(1_000, Some(123), 1_500);

        let sample = state.display_sample().expect("selected frame should be displayable");
        assert_eq!(sample.frame_id, Some(123));
        assert_eq!(sample.webrtc_receive_timestamp_us, Some(1_200));
        assert_eq!(sample.frame_prepare_timestamp_us, Some(1_500));
        assert_eq!(sample.frame_painted_timestamp_us, Some(1_500));

        let lines = state.display_overlay_lines(Instant::now()).expect("overlay should render");
        assert_eq!(lines[5], "frame painted:         00:00:00:001     +0.1ms");
    }

    #[test]
    fn subscriber_timing_summary_latencies_refresh_at_ten_hz() {
        let mut state = SubscriberTimingState::default();
        let now = Instant::now();

        state.record_subscribe_event(subscribe_event(
            SubscribeTimingStage::WebrtcReceive,
            1_000,
            33_400,
        ));
        state.record_subscribe_event(subscribe_event(
            SubscribeTimingStage::DecoderUpload,
            1_000,
            36_500,
        ));
        state.record_subscribe_event(subscribe_event(
            SubscribeTimingStage::DecoderOutput,
            1_000,
            56_300,
        ));
        state.record_frame_painted(1_000, Some(1), 57_200, 57_900);
        let lines = state.display_overlay_lines(now).expect("overlay should render");
        assert_eq!(lines[3], "decoder upload:        00:00:00:036     +3.1ms");
        assert_eq!(lines[4], "decoder output:        00:00:00:056    +19.8ms");
        assert_eq!(lines[5], "frame painted:         00:00:00:057     +1.6ms");
        assert_eq!(lines[6], "Exposure to Receive:                    32.4ms");
        assert_eq!(lines[7], "Receive to Render:                      24.5ms");
        assert_eq!(lines[8], "e2e latency:                            56.9ms");

        state.record_subscribe_event(subscribe_event(
            SubscribeTimingStage::WebrtcReceive,
            1_000_000,
            1_050_000,
        ));
        state.record_subscribe_event(subscribe_event(
            SubscribeTimingStage::DecoderUpload,
            1_000_000,
            1_060_000,
        ));
        state.record_subscribe_event(subscribe_event(
            SubscribeTimingStage::DecoderOutput,
            1_000_000,
            1_080_000,
        ));
        state.record_frame_painted(1_000_000, Some(2), 1_090_000, 1_100_000);
        let lines = state
            .display_overlay_lines(now + Duration::from_millis(99))
            .expect("overlay should render");
        assert_eq!(lines[3], "decoder upload:        00:00:01:060     +3.1ms");
        assert_eq!(lines[4], "decoder output:        00:00:01:080    +19.8ms");
        assert_eq!(lines[5], "frame painted:         00:00:01:100     +1.6ms");
        assert_eq!(lines[6], "Exposure to Receive:                    32.4ms");
        assert_eq!(lines[7], "Receive to Render:                      24.5ms");
        assert_eq!(lines[8], "e2e latency:                            56.9ms");

        let lines = state
            .display_overlay_lines(now + Duration::from_millis(100))
            .expect("overlay should render");
        assert_eq!(lines[3], "decoder upload:        00:00:01:060    +10.0ms");
        assert_eq!(lines[4], "decoder output:        00:00:01:080    +20.0ms");
        assert_eq!(lines[5], "frame painted:         00:00:01:100    +20.0ms");
        assert_eq!(lines[6], "Exposure to Receive:                    50.0ms");
        assert_eq!(lines[7], "Receive to Render:                      50.0ms");
        assert_eq!(lines[8], "e2e latency:                           100.0ms");
    }
}
