use eframe::egui;

const DEFAULT_ASPECT: f32 = 16.0 / 9.0;
const INITIAL_LONG_EDGE: f32 = 960.0;
const MIN_LONG_EDGE: f32 = 320.0;
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
    let aspect = initial_aspect.filter(|aspect| valid_aspect(*aspect)).unwrap_or(DEFAULT_ASPECT);
    eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size(initial_window_size(Some(aspect)))
            .with_min_inner_size(minimum_window_size(aspect)),
        persist_window: false,
        vsync: false,
        wgpu_options: wgpu_options(),
        ..Default::default()
    }
}

fn wgpu_options() -> egui_wgpu::WgpuConfiguration {
    let mut options = egui_wgpu::WgpuConfiguration::default();

    if cfg!(target_os = "linux") && std::env::var_os("WGPU_BACKEND").is_none() {
        if let egui_wgpu::WgpuSetup::CreateNew(create_new) = &mut options.wgpu_setup {
            create_new.instance_descriptor.backends = egui_wgpu::wgpu::Backends::VULKAN;
        }
    }

    options
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
    aspect_size(INITIAL_LONG_EDGE, aspect.filter(|a| valid_aspect(*a)).unwrap_or(DEFAULT_ASPECT))
}

fn minimum_window_size(aspect: f32) -> egui::Vec2 {
    aspect_size(MIN_LONG_EDGE, aspect)
}
