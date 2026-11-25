//! The live window consumes the most recent reconstruction on each repaint.
use std::{
    sync::{Arc, Mutex, Weak},
    time::{Duration, Instant},
};

use crate::{
    polygon::DisplayTriangle,
    timing::{now_us, ClockSample, Trace},
    transport::{self, Output},
    video::Canvas,
    Subscriber,
};
use anyhow::{anyhow, Result};
use eframe::{egui, egui_wgpu, wgpu};

#[path = "../../local_video/src/gpu_completion.rs"]
mod gpu_completion;
use gpu_completion::GpuCompletionPoller;

/// A reconstructed frame and its capture metadata.
struct Picture {
    id: u32,
    exposure_us: Option<u64>,
    canvas: Canvas,
    triangles: Arc<Vec<DisplayTriangle>>,
}

/// Shared handoff between the Data Track receiver and the main-thread UI.
#[derive(Default)]
pub(crate) struct Window {
    state: Mutex<State>,
}

/// The one pending picture is overwritten if the graphics thread falls behind.
#[derive(Default)]
struct State {
    pending: Option<Picture>,
    clock: Option<ClockSample>,
    last_rtt: Option<Duration>,
    latency: LatencyOverlay,
    context: Option<egui::Context>,
    finished: bool,
    error: Option<anyhow::Error>,
    next_refresh: Option<Instant>,
    painted: u64,
    superseded: u64,
    received_bytes: u64,
}

/// Samples completed frames for a trailing one-second mean, displayed twice a second.
#[derive(Default)]
struct LatencyOverlay {
    samples: Vec<(Instant, u32, Duration)>,
    displayed: Option<Duration>,
    refresh_at: Option<Instant>,
}

impl LatencyOverlay {
    /// Keeps only the most recent second of GPU completions, including during a UI stall.
    fn expire(&mut self, now: Instant) {
        self.samples
            .retain(|&(at, _, _)| now.saturating_duration_since(at) < Duration::from_secs(1));
    }

    /// Counts each frame once within the averaging window.
    fn record(&mut self, now: Instant, id: u32, delay: Duration) {
        self.expire(now);
        if !self.samples.iter().any(|&(_, frame, _)| frame == id) {
            self.samples.push((now, id, delay));
        }
    }

    /// Returns the latched display value and time remaining until its next refresh.
    fn read(&mut self, now: Instant, calibrated: bool) -> (Option<Duration>, Duration) {
        let interval = Duration::from_millis(500);
        if let Some(deadline) = self.refresh_at.filter(|&deadline| deadline > now) {
            return (self.displayed, deadline - now);
        }
        self.expire(now);
        self.displayed = (calibrated && !self.samples.is_empty()).then(|| {
            self.samples.iter().map(|&(_, _, delay)| delay).sum::<Duration>()
                / self.samples.len() as u32
        });
        self.refresh_at = Some(now + interval);
        (self.displayed, interval)
    }
}

impl Window {
    /// Counts each received Data Track payload once.
    pub(crate) fn record_payload(&self, bytes: usize) {
        self.state.lock().expect("viewer handoff poisoned").received_bytes += bytes as u64;
    }

    /// Installs the lowest-delay clock mapping and the most recent measured round trip.
    pub(crate) fn clock_sample(&self, sample: ClockSample, rtt: Duration) {
        let mut state = self.state.lock().expect("viewer handoff poisoned");
        state.clock = Some(sample);
        state.last_rtt = Some(rtt);
    }

    /// Offers a picture without waiting for the operator's display refresh.
    pub(crate) fn offer(
        &self,
        frame: u32,
        exposure_us: Option<u64>,
        canvas: Canvas,
        triangles: Arc<Vec<DisplayTriangle>>,
    ) {
        let mut state = self.state.lock().expect("viewer handoff poisoned");
        if state.pending.replace(Picture { id: frame, exposure_us, canvas, triangles }).is_some() {
            state.superseded += 1;
        }
        if let Some(ctx) = &state.context {
            ctx.request_repaint_after(state.next_refresh.map_or(Duration::ZERO, |deadline| {
                deadline.saturating_duration_since(Instant::now())
            }));
        }
    }

    /// Counts new pictures painted and pending pictures replaced before a repaint.
    pub(crate) fn counts(&self) -> (u64, u64) {
        let state = self.state.lock().expect("viewer handoff poisoned");
        (state.painted, state.superseded)
    }

    /// Wakes the viewer on stream closure, including when the track never produced a frame.
    fn finish(&self) {
        let mut state = self.state.lock().expect("viewer handoff poisoned");
        state.finished = true;
        if let Some(ctx) = &state.context {
            ctx.request_repaint();
        }
    }
}

/// Records when the GPU has finished the render pass containing this frame's image.
struct FrameCompletion {
    id: u32,
    exposure_us: u64,
    clock: ClockSample,
    window: Weak<Window>,
    poller: Weak<GpuCompletionPoller>,
    trace: Trace,
}

impl egui_wgpu::CallbackTrait for FrameCompletion {
    fn paint(
        &self,
        _: egui::PaintCallbackInfo,
        pass: &mut wgpu::RenderPass<'static>,
        _: &egui_wgpu::CallbackResources,
    ) {
        let Some(poller) = self.poller.upgrade() else { return };
        let probe = poller.begin_probe();
        let (id, exposure_us, clock) = (self.id, self.exposure_us, self.clock);
        let (window, trace) = (self.window.clone(), self.trace.clone());
        pass.on_submitted_work_done(move || {
            let now = Instant::now();
            let end = clock.publisher_time(now);
            let delay = end.and_then(|end| end.checked_sub(exposure_us)).map(Duration::from_micros);
            drop(probe);
            let Some(window) = window.upgrade() else { return };
            let mut state = window.state.lock().expect("viewer handoff poisoned");
            if let Some(delay) = delay {
                state.latency.record(now, id, delay);
            }
            if let Some(end) = end {
                if let Err(error) = trace.record(id, "gpu_publisher_clock", end) {
                    state.error = Some(error);
                }
            }
        });
    }
}

/// Shows the latest picture, keeping its original aspect ratio as the window resizes.
struct Viewer {
    window: Arc<Window>,
    picture: Option<Picture>,
    poller: Arc<GpuCompletionPoller>,
    overlay: Option<RateOverlay>,
    interval: Option<Duration>,
    trace: Trace,
}

/// Samples cumulative display and receive counters once per second of wall time.
struct RateOverlay {
    sampled_at: Instant,
    painted: u64,
    received_bytes: u64,
    label: String,
}

impl RateOverlay {
    /// Starts sampling when the first picture reaches the window.
    fn new(now: Instant, painted: u64, received_bytes: u64) -> Self {
        Self { sampled_at: now, painted, received_bytes, label: "— FPS * — kbps".into() }
    }

    /// Updates the label when due and returns the time until the next sample.
    fn update(&mut self, now: Instant, painted: u64, received_bytes: u64) -> Duration {
        let elapsed = now.duration_since(self.sampled_at);
        let interval = Duration::from_secs(1);
        if elapsed < interval {
            return interval - elapsed;
        }
        let seconds = elapsed.as_secs_f64();
        let fps = (painted - self.painted) as f64 / seconds;
        let kbps = (received_bytes - self.received_bytes) as f64 * 8.0 / 1000.0 / seconds;
        self.label = format!("{fps:.1} FPS * {kbps:.1} kbps");
        self.sampled_at = now;
        self.painted = painted;
        self.received_bytes = received_bytes;
        interval
    }

    /// Paints the label inside the upper-left corner of the fitted video.
    fn paint(
        &self,
        ui: &mut egui::Ui,
        video: egui::Rect,
        delay: Option<Duration>,
        rtt: Option<Duration>,
    ) {
        let painter = ui.painter().with_clip_rect(video);
        let text = painter.layout_no_wrap(
            self.caption(delay),
            egui::FontId::monospace(12.0),
            egui::Color32::WHITE,
        );
        let padding = egui::vec2(6.0, 3.0);
        let badge = egui::Rect::from_min_size(
            video.min + egui::vec2(6.0, 6.0),
            text.size() + 2.0 * padding,
        );
        painter.rect_filled(badge, 3, egui::Color32::from_black_alpha(185));
        painter.galley(badge.min + padding, text, egui::Color32::WHITE);
        ui.interact(badge, ui.id().with("latency-overlay"), egui::Sense::hover()).on_hover_text(
            format!(
                "Camera exposure to GPU completion (1 s average)\nPublisher round trip: {} ms",
                milliseconds(rtt)
            ),
        );
    }

    /// Formats the rolling latency alongside the sampled display and receive rates.
    fn caption(&self, delay: Option<Duration>) -> String {
        format!("{} * {} ms g2g", self.label, milliseconds(delay))
    }
}

/// Formats a duration with a tenth of a millisecond or marks it unavailable.
fn milliseconds(value: Option<Duration>) -> String {
    value.map_or_else(|| "—".into(), |d| format!("{:.1}", d.as_secs_f64() * 1000.0))
}

/// Builds one draw mesh for all faces in display coordinates.
fn draw_triangles(painter: &egui::Painter, rect: egui::Rect, triangles: &[DisplayTriangle]) {
    let mut mesh = egui::Mesh::default();
    mesh.vertices.reserve(3 * triangles.len());
    mesh.indices.reserve(3 * triangles.len());
    for triangle in triangles {
        let first = mesh.vertices.len() as u32;
        let [r, g, b] = triangle.rgb;
        let color = egui::Color32::from_rgb(r, g, b);
        for [x, y] in triangle.points {
            mesh.colored_vertex(rect.min + egui::vec2(x * rect.width(), y * rect.height()), color);
        }
        mesh.add_triangle(first, first + 1, first + 2);
    }
    painter.add(egui::Shape::mesh(mesh));
}

impl eframe::App for Viewer {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let (picture, finished) = {
            let mut state = self.window.state.lock().expect("viewer handoff poisoned");
            let now = Instant::now();
            if !state.finished && state.next_refresh.is_some_and(|deadline| deadline > now) {
                if state.pending.is_some() {
                    ui.ctx().request_repaint_after(state.next_refresh.unwrap_or(now) - now);
                }
                (None, false)
            } else {
                let picture = state.pending.take();
                if picture.is_some() {
                    state.next_refresh = self.interval.map(|interval| now + interval);
                }
                (picture, state.finished)
            }
        };
        let id = picture.as_ref().map(|p| p.id);
        let exposure = picture.as_ref().and_then(|p| p.exposure_us);
        if let Some(picture) = picture {
            self.picture = Some(picture);
        }
        if let Some(picture) = &self.picture {
            let bounds = ui.max_rect();
            let size = egui::vec2(picture.canvas.width as f32, picture.canvas.height as f32);
            let scale = (bounds.width() / size.x).min(bounds.height() / size.y);
            let rect = egui::Rect::from_center_size(bounds.center(), size * scale);
            draw_triangles(ui.painter(), rect, &picture.triangles);
            let now = Instant::now();
            let (painted, received_bytes, clock, (delay, latency_refresh), rtt) = {
                let mut state = self.window.state.lock().expect("viewer handoff poisoned");
                state.painted += u64::from(id.is_some());
                let clock = state.clock.filter(|s| s.publisher_time(now).is_some());
                (
                    state.painted,
                    state.received_bytes,
                    clock,
                    state.latency.read(now, clock.is_some()),
                    clock.and(state.last_rtt),
                )
            };
            if let (Some(id), Some(exposure_us), Some(clock)) = (id, exposure, clock) {
                ui.painter().add(egui_wgpu::Callback::new_paint_callback(
                    rect,
                    FrameCompletion {
                        id,
                        exposure_us,
                        clock,
                        window: Arc::downgrade(&self.window),
                        poller: Arc::downgrade(&self.poller),
                        trace: self.trace.clone(),
                    },
                ));
            }
            let overlay =
                self.overlay.get_or_insert_with(|| RateOverlay::new(now, painted, received_bytes));
            let rate_refresh = overlay.update(now, painted, received_bytes);
            ui.ctx().request_repaint_after(rate_refresh.min(latency_refresh));
            overlay.paint(ui, rect, delay, rtt);
        } else {
            ui.centered_and_justified(|ui| ui.label("Waiting for video…"));
        }
        if let Some(id) = id {
            if let Err(error) = self.trace.record(id, "paint", now_us()) {
                self.window.state.lock().expect("viewer handoff poisoned").error = Some(error);
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            }
            if finished {
                ui.ctx().request_repaint();
            }
        } else if finished {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}

/// Runs graphics on the main thread and cancels and joins the receiver when the window closes.
pub(crate) async fn run(args: Subscriber, trace: Trace) -> Result<()> {
    let interval = args.max_display_fps.map(|hz| Duration::from_secs_f64(1.0 / f64::from(hz)));
    let window = Arc::new(Window::default());
    let receiver_window = Arc::clone(&window);
    let worker_trace = trace.clone();
    let (cancel, cancelled) = tokio::sync::oneshot::channel::<()>();
    let receiver = tokio::spawn(async move {
        let result =
            transport::subscribe(&args, Output::Window(&receiver_window), &worker_trace, async {
                let _ = cancelled.await;
            })
            .await;
        receiver_window.finish();
        result
    });
    let view = Arc::clone(&window);
    let mut options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([768.0, 432.0]),
        persist_window: false,
        ..Default::default()
    };
    options.wgpu_options.surface.desired_maximum_frame_latency = Some(1);

    let display = eframe::run_native(
        "Vector video",
        options,
        Box::new(move |cc| {
            view.state.lock().expect("viewer handoff poisoned").context = Some(cc.egui_ctx.clone());
            let device = &cc.wgpu_render_state.as_ref().ok_or("Missing wgpu render state")?.device;
            let poller = Arc::new(GpuCompletionPoller::new(device.clone()));
            Ok(Box::new(Viewer {
                window: view,
                poller,
                picture: None,
                overlay: None,
                interval,
                trace,
            }))
        }),
    )
    .map_err(|error| anyhow!("Could not run live viewer: {error}"));
    let _ = cancel.send(());
    let result = receiver.await?;
    let mut state = window.state.lock().expect("viewer handoff poisoned");
    state.context = None;
    if let Some(error) = state.error.take() {
        return Err(error);
    }
    display.and(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Irregular, out-of-order completions contribute once and UI repaints hold the 2 Hz value.
    #[test]
    fn latency_averages_overlapping_seconds_and_expires_on_stall() {
        let start = Instant::now();
        let ms = Duration::from_millis;
        let mut latency = LatencyOverlay::default();
        assert_eq!(latency.read(start, true), (None, ms(500)));
        latency.record(start + ms(100), 1, ms(10));
        latency.record(start + ms(400), 3, ms(70));
        latency.record(start + ms(410), 2, ms(40));
        latency.record(start + ms(420), 3, ms(90));
        assert_eq!(latency.read(start + ms(499), true), (None, ms(1)));
        assert_eq!(latency.read(start + ms(500), true), (Some(ms(40)), ms(500)));
        latency.record(start + ms(750), 4, ms(80));
        assert_eq!(latency.read(start + ms(900), true), (Some(ms(40)), ms(100)));
        assert_eq!(latency.read(start + ms(1000), true).0, Some(ms(50)));
        assert_eq!(latency.read(start + ms(1500), true).0, Some(ms(80)));
        assert_eq!(latency.read(start + ms(2000), true).0, None);
        latency.record(start + ms(2100), 5, ms(30));
        assert_eq!(latency.read(start + ms(2500), false).0, None);
    }

    /// Sampling uses elapsed time, excludes UI-only repaints, and falls to zero on a stall.
    #[test]
    fn overlay_samples_displayed_pictures_and_payload_bytes() {
        let start = Instant::now();
        let mut overlay = RateOverlay::new(start, 1, 180);
        let half_second = Duration::from_millis(500);
        assert_eq!(overlay.update(start + half_second, 7, 900), half_second);
        let next = start + Duration::from_secs(2);
        overlay.update(next, 26, 3180);
        assert_eq!(overlay.label, "12.5 FPS * 12.0 kbps");
        assert_eq!(
            overlay.caption(Some(Duration::from_micros(123_460))),
            "12.5 FPS * 12.0 kbps * 123.5 ms g2g"
        );
        assert_eq!(overlay.caption(None), "12.5 FPS * 12.0 kbps * — ms g2g");
        overlay.update(next + Duration::from_secs(1), 26, 3180);
        assert_eq!(overlay.label, "0.0 FPS * 0.0 kbps");
    }

    /// Stalled repainting retains only the newest pixels and releases overwritten frames.
    #[test]
    fn a_stalled_window_releases_old_pictures() {
        let window = Window::default();
        let pixels = Arc::new(Vec::new());
        let old = Arc::downgrade(&pixels);
        window.offer(0, Some(10), Canvas::DEFAULT, pixels);
        window.offer(1, Some(12), Canvas::DEFAULT, Arc::new(Vec::new()));
        assert!(old.upgrade().is_none());
        window.finish();
        let state = window.state.lock().unwrap();
        assert!(state.finished);
        assert_eq!(state.superseded, 1);
        assert_eq!(state.pending.as_ref().unwrap().exposure_us, Some(12));
        assert_eq!(state.pending.as_ref().unwrap().id, 1);
    }
}
