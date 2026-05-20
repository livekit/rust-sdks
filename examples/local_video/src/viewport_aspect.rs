use eframe::egui;

const DEFAULT_ASPECT: f32 = 16.0 / 9.0;
const INITIAL_LONG_EDGE: f32 = 960.0;
const MIN_LONG_EDGE: f32 = 320.0;
const ASPECT_EPSILON: f32 = 0.001;
const SIZE_EPSILON: f32 = 1.0;

pub(crate) struct AspectConstrainedViewport {
    aspect: Option<f32>,
    last_size: Option<egui::Vec2>,
}

impl AspectConstrainedViewport {
    pub(crate) fn new(initial_aspect: Option<f32>) -> Self {
        Self { aspect: initial_aspect.filter(|aspect| valid_aspect(*aspect)), last_size: None }
    }

    pub(crate) fn aspect(&self) -> Option<f32> {
        self.aspect
    }

    pub(crate) fn set_video_size(&mut self, ctx: &egui::Context, width: u32, height: u32) -> bool {
        if width == 0 || height == 0 {
            return false;
        }

        let aspect = width as f32 / height as f32;
        if aspect_changed(self.aspect, aspect) {
            self.aspect = Some(aspect);
            ctx.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(minimum_window_size(aspect)));
            true
        } else {
            false
        }
    }

    pub(crate) fn constrain(&mut self, ctx: &egui::Context, aspect_just_changed: bool) {
        let Some(aspect) = self.aspect else {
            return;
        };
        let current_size = viewport_size(ctx);
        if current_size.x < 1.0 || current_size.y < 1.0 {
            return;
        }

        let current_aspect = current_size.x / current_size.y;
        if (current_aspect - aspect).abs() <= ASPECT_EPSILON {
            self.last_size = Some(current_size);
            return;
        }

        let target_size = if aspect_just_changed {
            fit_inside(current_size, aspect)
        } else {
            resize_from_changed_axis(current_size, self.last_size, aspect)
        };

        if !sizes_close(current_size, target_size) {
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(target_size));
        }
        self.last_size = Some(current_size);
    }
}

#[allow(dead_code)] // used by other binaries in this crate (e.g. `publisher`)
pub(crate) fn native_options(initial_aspect: Option<f32>) -> eframe::NativeOptions {
    let aspect = initial_aspect.filter(|aspect| valid_aspect(*aspect)).unwrap_or(DEFAULT_ASPECT);
    eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size(initial_window_size(Some(aspect)))
            .with_min_inner_size(minimum_window_size(aspect)),
        persist_window: false,
        vsync: false,
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

#[allow(dead_code)] // used via `native_options` from other binaries in this crate
fn initial_window_size(aspect: Option<f32>) -> egui::Vec2 {
    aspect_size(INITIAL_LONG_EDGE, aspect.filter(|a| valid_aspect(*a)).unwrap_or(DEFAULT_ASPECT))
}

fn minimum_window_size(aspect: f32) -> egui::Vec2 {
    aspect_size(MIN_LONG_EDGE, aspect)
}

fn viewport_size(ctx: &egui::Context) -> egui::Vec2 {
    ctx.input(|i| i.viewport().inner_rect.map(|rect| rect.size()))
        .unwrap_or_else(|| ctx.viewport_rect().size())
}

fn fit_inside(size: egui::Vec2, aspect: f32) -> egui::Vec2 {
    let width = size.x.max(1.0);
    let height = size.y.max(1.0);
    let mut target_width = width;
    let mut target_height = target_width / aspect;
    if target_height > height {
        target_height = height;
        target_width = target_height * aspect;
    }
    egui::vec2(target_width.max(1.0), target_height.max(1.0))
}

fn resize_from_changed_axis(
    size: egui::Vec2,
    previous_size: Option<egui::Vec2>,
    aspect: f32,
) -> egui::Vec2 {
    let width = size.x.max(1.0);
    let height = size.y.max(1.0);
    let Some(previous_size) = previous_size else {
        return fit_inside(size, aspect);
    };

    let width_delta = (width - previous_size.x).abs();
    let height_delta = (height - previous_size.y).abs();
    if width_delta >= height_delta {
        egui::vec2(width, (width / aspect).max(1.0))
    } else {
        egui::vec2((height * aspect).max(1.0), height)
    }
}

fn sizes_close(left: egui::Vec2, right: egui::Vec2) -> bool {
    (left.x - right.x).abs() <= SIZE_EPSILON && (left.y - right.y).abs() <= SIZE_EPSILON
}
