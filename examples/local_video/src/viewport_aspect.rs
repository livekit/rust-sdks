use eframe::{egui, wgpu, Renderer};
use egui_wgpu as egui_wgpu_backend;
use std::time::Duration;

const DEFAULT_ASPECT: f32 = 16.0 / 9.0;
const DEFAULT_INITIAL_LONG_EDGE: f32 = 960.0;
const MIN_LONG_EDGE: f32 = 320.0;
/// Repaint cadence used while a local video window is visible.
pub(crate) const VIDEO_REPAINT_INTERVAL: Duration = Duration::from_millis(8);
const ASPECT_EPSILON: f32 = 0.001;

pub(crate) struct AspectConstrainedViewport {
    aspect: Option<f32>,
    fit_window_to_first_video_size: bool,
}

impl AspectConstrainedViewport {
    /// Creates viewport aspect state for a window using the default startup size.
    pub(crate) fn new(initial_aspect: Option<f32>) -> Self {
        let aspect = initial_aspect.filter(|aspect| valid_aspect(*aspect));
        Self { aspect, fit_window_to_first_video_size: aspect.is_none() }
    }

    /// Returns the active video aspect ratio when it is known.
    pub(crate) fn aspect(&self) -> Option<f32> {
        self.aspect
    }

    /// Updates the video size used for viewport constraints.
    pub(crate) fn set_video_size(&mut self, ctx: &egui::Context, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }

        let aspect = width as f32 / height as f32;
        if aspect_changed(self.aspect, aspect) {
            let fit_window_to_video = self.fit_window_to_first_video_size && self.aspect.is_none();
            self.aspect = Some(aspect);
            ctx.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(minimum_window_size(aspect)));
            if fit_window_to_video {
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(initial_window_size(Some(
                    aspect,
                ))));
            }
        }
        self.fit_window_to_first_video_size = false;
    }
}

/// Returns native window options for a video display.
pub(crate) fn native_options(initial_aspect: Option<f32>) -> eframe::NativeOptions {
    let aspect = initial_aspect.filter(|aspect| valid_aspect(*aspect)).unwrap_or(DEFAULT_ASPECT);
    let mut wgpu_options = egui_wgpu_backend::WgpuConfiguration::default();
    #[cfg(target_os = "macos")]
    {
        wgpu_options.surface.present_mode = wgpu::PresentMode::Immediate;
    }
    #[cfg(not(target_os = "macos"))]
    {
        wgpu_options.surface.present_mode = wgpu::PresentMode::AutoNoVsync;
    }
    #[cfg(target_os = "linux")]
    if std::env::var_os("WGPU_BACKEND").is_none() {
        if let egui_wgpu_backend::WgpuSetup::CreateNew(create_new) = &mut wgpu_options.wgpu_setup {
            create_new.instance_descriptor.backends = wgpu::Backends::VULKAN;
        }
    }
    wgpu_options.surface.desired_maximum_frame_latency = Some(1);

    eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size(initial_window_size(Some(aspect)))
            .with_min_inner_size(minimum_window_size(aspect)),
        persist_window: false,
        multisampling: 0,
        depth_buffer: 0,
        stencil_buffer: 0,
        renderer: Renderer::Wgpu,
        wgpu_options,
        dithering: false,
        ..Default::default()
    }
}

/// Returns the largest video size that fits the available UI space.
pub(crate) fn fitted_video_size(available: egui::Vec2, aspect: Option<f32>) -> egui::Vec2 {
    let available = egui::vec2(available.x.max(1.0), available.y.max(1.0));
    let Some(aspect) = aspect.filter(|aspect| valid_aspect(*aspect)) else {
        return available;
    };

    let mut width = available.x;
    let mut height = (width / aspect).max(1.0);
    if height > available.y {
        height = available.y;
        width = (height * aspect).max(1.0);
    }
    egui::vec2(width, height)
}

fn valid_aspect(aspect: f32) -> bool {
    aspect.is_finite() && aspect > 0.0
}

fn aspect_changed(current: Option<f32>, next: f32) -> bool {
    match current {
        Some(current) => (current - next).abs() > ASPECT_EPSILON,
        None => true,
    }
}

fn aspect_size(long_edge: f32, aspect: f32) -> egui::Vec2 {
    if aspect >= 1.0 {
        egui::vec2(long_edge, long_edge / aspect)
    } else {
        egui::vec2(long_edge * aspect, long_edge)
    }
}

fn initial_window_size(aspect: Option<f32>) -> egui::Vec2 {
    aspect_size(
        DEFAULT_INITIAL_LONG_EDGE,
        aspect.filter(|a| valid_aspect(*a)).unwrap_or(DEFAULT_ASPECT),
    )
}

fn minimum_window_size(aspect: f32) -> egui::Vec2 {
    aspect_size(MIN_LONG_EDGE, aspect)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_vec2_near(actual: egui::Vec2, expected: egui::Vec2) {
        assert!((actual.x - expected.x).abs() < 0.001, "x: {} != {}", actual.x, expected.x);
        assert!((actual.y - expected.y).abs() < 0.001, "y: {} != {}", actual.y, expected.y);
    }

    #[test]
    fn fitted_video_size_uses_available_size_without_aspect() {
        assert_eq!(fitted_video_size(egui::vec2(800.0, 600.0), None), egui::vec2(800.0, 600.0));
    }

    #[test]
    fn fitted_video_size_letterboxes_wide_video_by_height() {
        assert_vec2_near(
            fitted_video_size(egui::vec2(800.0, 300.0), Some(16.0 / 9.0)),
            egui::vec2(533.3334, 300.0),
        );
    }

    #[test]
    fn fitted_video_size_letterboxes_tall_video_by_width() {
        assert_vec2_near(
            fitted_video_size(egui::vec2(300.0, 800.0), Some(9.0 / 16.0)),
            egui::vec2(300.0, 533.3333),
        );
    }
}
