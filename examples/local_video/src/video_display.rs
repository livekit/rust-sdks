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
use std::time::Duration;

use crate::timestamp_burn::{format_timestamp_us, TextBurner, METRICS_OVERLAY_SCALE};
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
    pub(crate) fps: f32,
    pub(crate) dirty: bool,
    pub(crate) timing_sample: Option<PublisherTimingSample>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct PublisherTimingSample {
    pub(crate) frame_id: Option<u32>,
    pub(crate) capture_timestamp_us: u64,
    pub(crate) read_timestamp_us: u64,
    pub(crate) sent_timestamp_us: u64,
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
    publish_latency_display: Option<&str>,
) {
    let uv_w = (width + 1) / 2;
    let uv_h = (height + 1) / 2;
    let y_bytes_per_row = align_up(width, 256);
    let uv_bytes_per_row = align_up(uv_w, 256);

    let mut s = shared.lock();
    let mut y_buf = Vec::new();
    let mut u_buf = Vec::new();
    let mut v_buf = Vec::new();
    std::mem::swap(&mut y_buf, &mut s.y);
    std::mem::swap(&mut u_buf, &mut s.u);
    std::mem::swap(&mut v_buf, &mut s.v);

    pack_plane(y, y_stride, width, height, y_bytes_per_row, &mut y_buf);
    pack_plane(u, u_stride, uv_w, uv_h, uv_bytes_per_row, &mut u_buf);
    pack_plane(v, v_stride, uv_w, uv_h, uv_bytes_per_row, &mut v_buf);

    if let Some(sample) = timing_sample {
        let fallback_latency_display;
        let latency_display = if let Some(latency_display) = publish_latency_display {
            latency_display
        } else {
            fallback_latency_display =
                format_us_delta_ms(sample.sent_timestamp_us, sample.capture_timestamp_us);
            fallback_latency_display.as_str()
        };
        burn_publisher_timing_sample(
            sample,
            latency_display,
            width,
            height,
            y_bytes_per_row,
            &mut y_buf,
        );
    }

    s.width = width;
    s.height = height;
    s.y_bytes_per_row = y_bytes_per_row;
    s.uv_bytes_per_row = uv_bytes_per_row;
    std::mem::swap(&mut s.y, &mut y_buf);
    std::mem::swap(&mut s.u, &mut u_buf);
    std::mem::swap(&mut s.v, &mut v_buf);
    s.timing_sample = timing_sample;
    s.dirty = true;
}

fn format_us_delta_ms(later_us: u64, earlier_us: u64) -> String {
    let delta_us = later_us.saturating_sub(earlier_us);
    format!("{:.1}ms", delta_us as f64 / 1_000.0)
}

fn frame_id_label(frame_id: Option<u32>) -> String {
    frame_id.map(|id| id.to_string()).unwrap_or_else(|| "NA".to_string())
}

fn burned_stats_line(label: &str, value: impl std::fmt::Display) -> String {
    format!("{label:<17}{value}")
}

fn build_publisher_timing_lines(
    sample: PublisherTimingSample,
    publish_latency_display: &str,
) -> Vec<String> {
    vec![
        burned_stats_line("FRAME ID:", frame_id_label(sample.frame_id)),
        burned_stats_line("CAPT TIMESTAMP:", format_timestamp_us(sample.capture_timestamp_us)),
        burned_stats_line("READ TIMESTAMP:", format_timestamp_us(sample.read_timestamp_us)),
        burned_stats_line("SENT TIMESTAMP:", format_timestamp_us(sample.sent_timestamp_us)),
        burned_stats_line("PUBLISH LATENCY:", publish_latency_display),
    ]
}

fn burn_publisher_timing_sample(
    sample: PublisherTimingSample,
    publish_latency_display: &str,
    width: u32,
    height: u32,
    y_bytes_per_row: u32,
    y_buf: &mut [u8],
) {
    let burner = TextBurner::new_top_left(width, height, METRICS_OVERLAY_SCALE);
    let lines = build_publisher_timing_lines(sample, publish_latency_display);
    let line_refs = lines.iter().map(String::as_str).collect::<Vec<_>>();
    burner.draw_lines(y_buf, y_bytes_per_row as usize, &line_refs);
}

fn video_size(shared: &Arc<Mutex<SharedYuv>>) -> Option<(u32, u32)> {
    let s = shared.lock();
    if s.width > 0 && s.height > 0 {
        Some((s.width, s.height))
    } else {
        None
    }
}

struct VideoApp {
    shared: Arc<Mutex<SharedYuv>>,
    ctrl_c_received: Arc<AtomicBool>,
    viewport: AspectConstrainedViewport,
}

impl eframe::App for VideoApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.ctrl_c_received.load(Ordering::Acquire) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }

        let mut aspect_just_changed = false;
        if let Some((width, height)) = video_size(&self.shared) {
            aspect_just_changed = self.viewport.set_video_size(ctx, width, height);
        }
        self.viewport.constrain(ctx, aspect_just_changed);

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

        egui::Area::new("video_hud".into())
            .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-10.0, 10.0))
            .interactable(false)
            .show(ctx, |ui| {
                let s = self.shared.lock();
                if s.width == 0 || s.height == 0 || s.fps <= 0.0 || s.codec.is_empty() {
                    return;
                }
                let text = format!("{} {}x{} {:.1}fps", s.codec, s.width, s.height, s.fps);
                egui::Frame::NONE
                    .fill(egui::Color32::from_black_alpha(140))
                    .corner_radius(egui::CornerRadius::same(4))
                    .inner_margin(egui::Margin::same(6))
                    .show(ui, |ui| {
                        ui.add(
                            egui::Label::new(egui::RichText::new(text).color(egui::Color32::WHITE))
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
