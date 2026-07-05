use anyhow::Result;
use chrono::{Local, Timelike};
use clap::Parser;
use eframe::egui;
use eframe::wgpu::{self, util::DeviceExt};
use eframe::Renderer;
use egui_wgpu as egui_wgpu_backend;
use egui_wgpu_backend::CallbackTrait;
use std::num::NonZeroU64;

const CLOCK_CHAR_COUNT: usize = 12;
const COLON: u32 = 10;
const DOT: u32 = 11;

#[derive(Parser, Debug)]
#[command(
    author,
    version,
    about = "Display a low-latency millisecond clock with a millisecond grid",
    long_about = None
)]
struct Args {
    /// Start in borderless fullscreen
    #[arg(long, default_value_t = false)]
    fullscreen: bool,

    /// Keep the clock above normal windows
    #[arg(long, default_value_t = false)]
    always_on_top: bool,

    /// Disable vsync and render as fast as the display backend accepts frames
    #[arg(long, default_value_t = false)]
    no_vsync: bool,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct ClockUniform {
    viewport_size: [f32; 2],
    _pad0: [u32; 2],
    chars0: [u32; 4],
    chars1: [u32; 4],
    chars2: [u32; 4],
}

impl ClockUniform {
    fn new(viewport_size: [u32; 2], chars: [u32; CLOCK_CHAR_COUNT]) -> Self {
        Self {
            viewport_size: [viewport_size[0] as f32, viewport_size[1] as f32],
            _pad0: [0; 2],
            chars0: [chars[0], chars[1], chars[2], chars[3]],
            chars1: [chars[4], chars[5], chars[6], chars[7]],
            chars2: [chars[8], chars[9], chars[10], chars[11]],
        }
    }
}

struct ClockApp;

impl eframe::App for ClockApp {
    fn ui(&mut self, root_ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = root_ui.ctx().clone();
        ctx.request_repaint();

        egui::CentralPanel::default().frame(egui::Frame::NONE).show(root_ui, |ui| {
            ui.painter().rect_filled(ui.max_rect(), 0, egui::Color32::BLACK);

            let (rect, _) = ui.allocate_exact_size(ui.available_size(), egui::Sense::hover());
            let cb = egui_wgpu_backend::Callback::new_paint_callback(rect, ClockPaintCallback);
            ui.painter().add(cb);
        });
    }
}

struct ClockPaintCallback;

struct ClockGpuState {
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    uniform_buf: wgpu::Buffer,
}

impl CallbackTrait for ClockPaintCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        screen_descriptor: &egui_wgpu_backend::ScreenDescriptor,
        _encoder: &mut wgpu::CommandEncoder,
        resources: &mut egui_wgpu_backend::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        if resources.get::<ClockGpuState>().is_none() {
            resources.insert(ClockGpuState::new(device));
        }

        let state =
            resources.get::<ClockGpuState>().expect("clock GPU state should exist after insertion");
        let uniform = ClockUniform::new(screen_descriptor.size_in_pixels, current_clock_chars());
        queue.write_buffer(&state.uniform_buf, 0, bytemuck::bytes_of(&uniform));

        Vec::new()
    }

    fn paint(
        &self,
        _info: egui::PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'static>,
        resources: &egui_wgpu_backend::CallbackResources,
    ) {
        let Some(state) = resources.get::<ClockGpuState>() else {
            return;
        };

        render_pass.set_pipeline(&state.pipeline);
        render_pass.set_bind_group(0, &state.bind_group, &[]);
        render_pass.draw(0..3, 0..1);
    }
}

impl ClockGpuState {
    fn new(device: &wgpu::Device) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("clock_shader"),
            source: wgpu::ShaderSource::Wgsl(CLOCK_SHADER.into()),
        });

        let bind_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("clock_bind_layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: Some(
                        NonZeroU64::new(std::mem::size_of::<ClockUniform>() as u64)
                            .expect("clock uniform size should be non-zero"),
                    ),
                },
                count: None,
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("clock_pipeline_layout"),
            bind_group_layouts: &[Some(&bind_layout)],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("clock_pipeline"),
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
            multiview_mask: None,
            cache: None,
        });

        let uniform_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("clock_uniform"),
            contents: bytemuck::bytes_of(&ClockUniform::new([1, 1], clock_chars(0, 0, 0, 0))),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("clock_bind_group"),
            layout: &bind_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buf.as_entire_binding(),
            }],
        });

        Self { pipeline, bind_group, uniform_buf }
    }
}

fn current_clock_chars() -> [u32; CLOCK_CHAR_COUNT] {
    let now = Local::now();
    clock_chars(now.hour(), now.minute(), now.second(), now.nanosecond() / 1_000_000)
}

fn clock_chars(hour: u32, minute: u32, second: u32, millisecond: u32) -> [u32; CLOCK_CHAR_COUNT] {
    [
        (hour / 10) % 10,
        hour % 10,
        COLON,
        (minute / 10) % 10,
        minute % 10,
        COLON,
        (second / 10) % 10,
        second % 10,
        DOT,
        (millisecond / 100) % 10,
        (millisecond / 10) % 10,
        millisecond % 10,
    ]
}

fn native_options(args: &Args) -> eframe::NativeOptions {
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("LiveKit Clock")
        .with_inner_size([960.0, 420.0])
        .with_min_inner_size([480.0, 210.0])
        .with_fullscreen(args.fullscreen);

    if args.always_on_top {
        viewport = viewport.with_always_on_top();
    }

    let mut wgpu_options = egui_wgpu_backend::WgpuConfiguration::default();
    let vsync = !args.no_vsync;
    wgpu_options.surface.present_mode =
        if vsync { wgpu::PresentMode::AutoVsync } else { wgpu::PresentMode::AutoNoVsync };
    wgpu_options.surface.desired_maximum_frame_latency = Some(1);

    eframe::NativeOptions {
        viewport,
        multisampling: 0,
        depth_buffer: 0,
        stencil_buffer: 0,
        renderer: Renderer::Wgpu,
        wgpu_options,
        persist_window: false,
        dithering: false,
        ..Default::default()
    }
}

fn main() -> Result<()> {
    env_logger::init();
    let args = Args::parse();
    eframe::run_native(
        "LiveKit Clock",
        native_options(&args),
        Box::new(|_| Ok(Box::new(ClockApp))),
    )?;
    Ok(())
}

const CLOCK_SHADER: &str = r#"
struct ClockUniform {
    viewport_size: vec2<f32>,
    _pad0: vec2<u32>,
    chars0: vec4<u32>,
    chars1: vec4<u32>,
    chars2: vec4<u32>,
};

@group(0) @binding(0)
var<uniform> clock: ClockUniform;

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

const CHAR_COUNT: u32 = 12u;
const DIGIT_HEIGHT: f32 = 1.85;
const CELL_WIDTH: f32 = 1.0;
const SEGMENT_THICKNESS: f32 = 0.16;
const COLON_WIDTH: f32 = 0.34;
const DOT_WIDTH: f32 = 0.24;
const GAP: f32 = 0.14;
const TOTAL_WIDTH: f32 = 9.0 * CELL_WIDTH + 2.0 * COLON_WIDTH + DOT_WIDTH + 11.0 * GAP;
const GRID_COLUMNS: u32 = 9u;
const GRID_ROWS: u32 = 3u;
const GRID_CELL: f32 = 0.72;
const GRID_COLUMN_GAP: f32 = 0.30;
const GRID_ROW_GAP: f32 = 0.30;
const GRID_TOP_GAP: f32 = 0.22;
const GRID_WIDTH: f32 = 9.0 * GRID_CELL + 8.0 * GRID_COLUMN_GAP;
const GRID_HEIGHT: f32 = 3.0 * GRID_CELL + 2.0 * GRID_ROW_GAP;
const GROUP_HEIGHT: f32 = DIGIT_HEIGHT + GRID_TOP_GAP + GRID_HEIGHT;
const COLON_CODE: u32 = 10u;
const DOT_CODE: u32 = 11u;
const FEATHER: f32 = 0.007;
const DIGIT_MASKS: array<u32, 10> = array<u32, 10>(
    0x3fu,
    0x06u,
    0x5bu,
    0x4fu,
    0x66u,
    0x6du,
    0x7du,
    0x07u,
    0x7fu,
    0x6fu,
);

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOut {
    let positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    let position = positions[vertex_index];

    var out: VertexOut;
    out.position = vec4<f32>(position, 0.0, 1.0);
    out.uv = position * vec2<f32>(0.5, -0.5) + vec2<f32>(0.5, 0.5);
    return out;
}

fn char_at(index: u32) -> u32 {
    if (index < 4u) {
        return clock.chars0[index];
    }
    if (index < 8u) {
        return clock.chars1[index - 4u];
    }
    return clock.chars2[index - 8u];
}

fn char_width(code: u32) -> f32 {
    if (code < 10u) {
        return CELL_WIDTH;
    }
    if (code == COLON_CODE) {
        return COLON_WIDTH;
    }
    return DOT_WIDTH;
}

fn segment_rect(segment: u32) -> vec4<f32> {
    let t = SEGMENT_THICKNESS;
    let mid = DIGIT_HEIGHT * 0.5;

    switch segment {
        case 0u: { return vec4<f32>(t, 0.0, CELL_WIDTH - t, t); }
        case 1u: { return vec4<f32>(CELL_WIDTH - t, t, CELL_WIDTH, mid); }
        case 2u: { return vec4<f32>(CELL_WIDTH - t, mid, CELL_WIDTH, DIGIT_HEIGHT - t); }
        case 3u: { return vec4<f32>(t, DIGIT_HEIGHT - t, CELL_WIDTH - t, DIGIT_HEIGHT); }
        case 4u: { return vec4<f32>(0.0, mid, t, DIGIT_HEIGHT - t); }
        case 5u: { return vec4<f32>(0.0, t, t, mid); }
        case 6u: { return vec4<f32>(t, mid - t * 0.5, CELL_WIDTH - t, mid + t * 0.5); }
        default: { return vec4<f32>(0.0); }
    }
}

fn rect_alpha(p: vec2<f32>, min_p: vec2<f32>, max_p: vec2<f32>) -> f32 {
    let center = (min_p + max_p) * 0.5;
    let half_size = (max_p - min_p) * 0.5;
    let d = abs(p - center) - half_size;
    let outside = length(max(d, vec2<f32>(0.0)));
    let inside = min(max(d.x, d.y), 0.0);
    let signed_distance = outside + inside;
    return 1.0 - smoothstep(0.0, FEATHER, signed_distance);
}

fn circle_alpha(p: vec2<f32>, center: vec2<f32>, radius: f32) -> f32 {
    return 1.0 - smoothstep(0.0, FEATHER, length(p - center) - radius);
}

fn digit_alpha(p: vec2<f32>, origin: vec2<f32>, digit: u32) -> f32 {
    if (digit > 9u) {
        return 0.0;
    }

    let local = p - origin;
    let mask = DIGIT_MASKS[digit];
    var alpha = 0.0;

    for (var segment = 0u; segment < 7u; segment = segment + 1u) {
        if ((mask & (1u << segment)) != 0u) {
            let r = segment_rect(segment);
            alpha = max(alpha, rect_alpha(local, r.xy, r.zw));
        }
    }

    return alpha;
}

fn separator_alpha(p: vec2<f32>, origin: vec2<f32>, code: u32) -> f32 {
    let local = p - origin;
    let center_x = char_width(code) * 0.5;

    if (code == COLON_CODE) {
        let r = 0.095;
        let top = circle_alpha(local, vec2<f32>(center_x, DIGIT_HEIGHT * 0.38), r);
        let bottom = circle_alpha(local, vec2<f32>(center_x, DIGIT_HEIGHT * 0.62), r);
        return max(top, bottom);
    }

    if (code == DOT_CODE) {
        return circle_alpha(local, vec2<f32>(center_x, DIGIT_HEIGHT - 0.095), 0.08);
    }

    return 0.0;
}

fn grid_alpha(p: vec2<f32>, origin: vec2<f32>) -> vec2<f32> {
    let grid_origin = origin + vec2<f32>((TOTAL_WIDTH - GRID_WIDTH) * 0.5, DIGIT_HEIGHT + GRID_TOP_GAP);
    var filled = 0.0;
    var unfilled = 0.0;

    for (var row = 0u; row < GRID_ROWS; row = row + 1u) {
        let row_digit = char_at(9u + row);
        for (var column = 0u; column < GRID_COLUMNS; column = column + 1u) {
            let cell_origin = grid_origin + vec2<f32>(
                f32(column) * (GRID_CELL + GRID_COLUMN_GAP),
                f32(row) * (GRID_CELL + GRID_ROW_GAP),
            );
            let cell_alpha = rect_alpha(p, cell_origin, cell_origin + vec2<f32>(GRID_CELL));
            if (column < row_digit) {
                filled = max(filled, cell_alpha);
            } else {
                unfilled = max(unfilled, cell_alpha);
            }
        }
    }

    return vec2<f32>(filled, unfilled);
}

fn clock_alpha(p: vec2<f32>, aspect: f32) -> f32 {
    let scale = min((aspect * 0.94) / TOTAL_WIDTH, 0.82 / GROUP_HEIGHT);
    let scaled_size = vec2<f32>(TOTAL_WIDTH, GROUP_HEIGHT) * scale;
    let origin = vec2<f32>((aspect - scaled_size.x) * 0.5, (1.0 - scaled_size.y) * 0.5);
    let local_p = (p - origin) / scale;

    var cursor = 0.0;
    var alpha = 0.0;

    for (var index = 0u; index < CHAR_COUNT; index = index + 1u) {
        let code = char_at(index);
        let char_origin = vec2<f32>(cursor, 0.0);

        if (code < 10u) {
            alpha = max(alpha, digit_alpha(local_p, char_origin, code));
        } else {
            alpha = max(alpha, separator_alpha(local_p, char_origin, code));
        }

        cursor = cursor + char_width(code);
        if (index + 1u < CHAR_COUNT) {
            cursor = cursor + GAP;
        }
    }

    return max(alpha, grid_alpha(local_p, vec2<f32>(0.0)).x);
}

fn grid_unfilled_alpha(p: vec2<f32>, aspect: f32) -> f32 {
    let scale = min((aspect * 0.94) / TOTAL_WIDTH, 0.82 / GROUP_HEIGHT);
    let scaled_size = vec2<f32>(TOTAL_WIDTH, GROUP_HEIGHT) * scale;
    let origin = vec2<f32>((aspect - scaled_size.x) * 0.5, (1.0 - scaled_size.y) * 0.5);
    let local_p = (p - origin) / scale;
    return grid_alpha(local_p, vec2<f32>(0.0)).y;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    let height = max(clock.viewport_size.y, 1.0);
    let aspect = max(clock.viewport_size.x / height, 0.1);
    let p = vec2<f32>(in.uv.x * aspect, in.uv.y);
    let alpha = clock_alpha(p, aspect);
    let grid_empty = grid_unfilled_alpha(p, aspect);
    let foreground = vec3<f32>(1.0, 0.98, 0.92);
    let empty_grid = vec3<f32>(0.14, 0.14, 0.14) * grid_empty;
    let color = max(empty_grid, foreground * alpha);
    return vec4<f32>(color, 1.0);
}
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_chars_render_three_millisecond_digits() {
        assert_eq!(clock_chars(12, 34, 56, 789), [1, 2, COLON, 3, 4, COLON, 5, 6, DOT, 7, 8, 9]);
    }
}
