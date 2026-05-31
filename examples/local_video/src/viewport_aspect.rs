use eframe::{egui, wgpu, Renderer};
use egui_wgpu as egui_wgpu_backend;

const DEFAULT_ASPECT: f32 = 16.0 / 9.0;
pub(crate) const DEFAULT_INITIAL_LONG_EDGE: f32 = 960.0;
pub(crate) const MIN_LONG_EDGE: f32 = 320.0;
const ASPECT_EPSILON: f32 = 0.001;

pub(crate) struct AspectConstrainedViewport {
    aspect: Option<f32>,
}

impl AspectConstrainedViewport {
    pub(crate) fn new(initial_aspect: Option<f32>) -> Self {
        Self { aspect: initial_aspect.filter(|aspect| valid_aspect(*aspect)) }
    }

    pub(crate) fn aspect(&self) -> Option<f32> {
        self.aspect
    }

    pub(crate) fn set_video_size(&mut self, ctx: &egui::Context, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }

        let aspect = width as f32 / height as f32;
        if aspect_changed(self.aspect, aspect) {
            self.aspect = Some(aspect);
            ctx.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(minimum_window_size(aspect)));
        }
    }
}

pub(crate) fn native_options(initial_aspect: Option<f32>) -> eframe::NativeOptions {
    native_options_with_initial_long_edge(initial_aspect, DEFAULT_INITIAL_LONG_EDGE)
}

pub(crate) fn native_options_with_initial_long_edge(
    initial_aspect: Option<f32>,
    initial_long_edge: f32,
) -> eframe::NativeOptions {
    let aspect = initial_aspect.filter(|aspect| valid_aspect(*aspect)).unwrap_or(DEFAULT_ASPECT);
    let initial_long_edge = initial_long_edge.max(MIN_LONG_EDGE);
    let mut wgpu_options = egui_wgpu_backend::WgpuConfiguration::default();
    #[cfg(target_os = "macos")]
    {
        wgpu_options.present_mode = wgpu::PresentMode::Immediate;
    }
    #[cfg(not(target_os = "macos"))]
    {
        wgpu_options.present_mode = wgpu::PresentMode::AutoNoVsync;
    }
    wgpu_options.desired_maximum_frame_latency = Some(1);

    eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size(initial_window_size(Some(aspect), initial_long_edge))
            .with_min_inner_size(minimum_window_size(aspect)),
        persist_window: false,
        vsync: false,
        multisampling: 0,
        depth_buffer: 0,
        stencil_buffer: 0,
        renderer: Renderer::Wgpu,
        wgpu_options,
        dithering: false,
        ..Default::default()
    }
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

fn initial_window_size(aspect: Option<f32>, initial_long_edge: f32) -> egui::Vec2 {
    aspect_size(initial_long_edge, aspect.filter(|a| valid_aspect(*a)).unwrap_or(DEFAULT_ASPECT))
}

fn minimum_window_size(aspect: f32) -> egui::Vec2 {
    aspect_size(MIN_LONG_EDGE, aspect)
}
