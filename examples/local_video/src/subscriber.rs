use anyhow::Result;
use clap::Parser;
use eframe::egui;
use egui_wgpu as egui_wgpu_backend;
use egui_wgpu_backend::CallbackTrait;
use eframe::wgpu::{self, util::DeviceExt};
use futures::StreamExt;
use livekit::prelude::*;
use livekit::webrtc::video_stream::native::NativeVideoStream;
use livekit_api::access_token;
use log::{debug, info};
use parking_lot::Mutex;
use std::{
    collections::HashMap,
    env,
    sync::Arc,
    time::{Duration, Instant},
};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// LiveKit participant identity
    #[arg(long, default_value = "rust-video-subscriber")] 
    identity: String,

    /// LiveKit room name
    #[arg(long, default_value = "video-room")] 
    room_name: String,

    /// LiveKit server URL
    #[arg(long)]
    url: Option<String>,

    /// LiveKit API key (can also be set via LIVEKIT_API_KEY environment variable)
    #[arg(long)]
    api_key: Option<String>,

    /// LiveKit API secret (can also be set via LIVEKIT_API_SECRET environment variable)
    #[arg(long)]
    api_secret: Option<String>,
}

struct SharedYuv {
    width: u32,
    height: u32,
    stride_y: u32,
    stride_u: u32,
    stride_v: u32,
    y: Vec<u8>,
    u: Vec<u8>,
    v: Vec<u8>,
    dirty: bool,
}

struct VideoApp {
    shared: Arc<Mutex<SharedYuv>>,
}

impl eframe::App for VideoApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            let available = ui.available_size();
            let rect = egui::Rect::from_min_size(ui.min_rect().min, available);

            // Ensure we keep repainting for smooth playback
            ui.ctx().request_repaint();

            // Add a custom wgpu paint callback that renders I420 directly
            let cb = egui_wgpu_backend::Callback::new_paint_callback(
                rect,
                YuvPaintCallback { shared: self.shared.clone() },
            );
            ui.painter().add(cb);
        });

        ctx.request_repaint_after(Duration::from_millis(16));
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    let args = Args::parse();

    // LiveKit connection details (prefer CLI args, fallback to env vars)
    let url = args
        .url
        .or_else(|| env::var("LIVEKIT_URL").ok())
        .expect("LiveKit URL must be provided via --url argument or LIVEKIT_URL environment variable");
    let api_key = args
        .api_key
        .or_else(|| env::var("LIVEKIT_API_KEY").ok())
        .expect("LiveKit API key must be provided via --api-key argument or LIVEKIT_API_KEY environment variable");
    let api_secret = args
        .api_secret
        .or_else(|| env::var("LIVEKIT_API_SECRET").ok())
        .expect("LiveKit API secret must be provided via --api-secret argument or LIVEKIT_API_SECRET environment variable");

    let token = access_token::AccessToken::with_api_key(&api_key, &api_secret)
        .with_identity(&args.identity)
        .with_name(&args.identity)
        .with_grants(access_token::VideoGrants {
            room_join: true,
            room: args.room_name.clone(),
            can_subscribe: true,
            ..Default::default()
        })
        .to_jwt()?;

    info!("Connecting to LiveKit room '{}' as '{}'...", args.room_name, args.identity);
    let mut room_options = RoomOptions::default();
    room_options.auto_subscribe = true;
    let (room, _) = Room::connect(&url, &token, room_options).await?;
    let room = Arc::new(room);
    info!("Connected: {} - {}", room.name(), room.sid().await);

    // Shared YUV buffer for UI/GPU
    let shared = Arc::new(Mutex::new(SharedYuv {
        width: 0,
        height: 0,
        stride_y: 0,
        stride_u: 0,
        stride_v: 0,
        y: Vec::new(),
        u: Vec::new(),
        v: Vec::new(),
        dirty: false,
    }));

    // Subscribe to room events: on first video track, start sink task
    let shared_clone = shared.clone();
    let rt = tokio::runtime::Handle::current();
    tokio::spawn(async move {
        let mut events = room.subscribe();
        info!("Subscribed to room events");
        while let Some(evt) = events.recv().await {
            debug!("Room event: {:?}", evt);
            if let RoomEvent::TrackSubscribed { track, publication, participant } = evt {
                if let livekit::track::RemoteTrack::Video(video_track) = track {
                    info!(
                        "Subscribed to video track: {} (sid {}) from {} - codec: {}, simulcast: {}, dimension: {}x{}",
                        publication.name(),
                        publication.sid(),
                        participant.identity(),
                        publication.mime_type(),
                        publication.simulcasted(),
                        publication.dimension().0,
                        publication.dimension().1
                    );

                    // Try to fetch inbound RTP/codec stats for more details
                    match video_track.get_stats().await {
                        Ok(stats) => {
                            let mut codec_by_id: HashMap<String, (String, String)> = HashMap::new();
                            let mut inbound: Option<livekit::webrtc::stats::InboundRtpStats> = None;
                            for s in stats.iter() {
                                match s {
                                    livekit::webrtc::stats::RtcStats::Codec(c) => {
                                        codec_by_id.insert(
                                            c.rtc.id.clone(),
                                            (c.codec.mime_type.clone(), c.codec.sdp_fmtp_line.clone()),
                                        );
                                    }
                                    livekit::webrtc::stats::RtcStats::InboundRtp(i) => {
                                        if i.stream.kind == "video" {
                                            inbound = Some(i.clone());
                                        }
                                    }
                                    _ => {}
                                }
                            }

                            if let Some(i) = inbound {
                                if let Some((mime, fmtp)) = codec_by_id.get(&i.stream.codec_id) {
                                    info!("Inbound codec: {} (fmtp: {})", mime, fmtp);
                                } else {
                                    info!("Inbound codec id: {}", i.stream.codec_id);
                                }
                                info!(
                                    "Inbound current layer: {}x{} ~{:.1} fps, decoder: {}, power_efficient: {}",
                                    i.inbound.frame_width,
                                    i.inbound.frame_height,
                                    i.inbound.frames_per_second,
                                    i.inbound.decoder_implementation,
                                    i.inbound.power_efficient_decoder
                                );
                            }
                        }
                        Err(e) => debug!("Failed to get stats for video track: {:?}", e),
                    }
                    // Start background sink thread
                    let shared2 = shared_clone.clone();
                    std::thread::spawn(move || {
                        let mut sink = NativeVideoStream::new(video_track.rtc_track());
                        let mut frames: u64 = 0;
                        let mut last_log = Instant::now();
                        let mut logged_first = false;
                        // YUV buffers reused to avoid per-frame allocations
                        let mut y_buf: Vec<u8> = Vec::new();
                        let mut u_buf: Vec<u8> = Vec::new();
                        let mut v_buf: Vec<u8> = Vec::new();
                        while let Some(frame) = rt.block_on(sink.next()) {
                            let w = frame.buffer.width();
                            let h = frame.buffer.height();

                            if !logged_first {
                                debug!(
                                    "First frame: {}x{}, type {:?}",
                                    w, h, frame.buffer.buffer_type()
                                );
                                logged_first = true;
                            }

                            // Convert to I420 on CPU, but keep planes separate for GPU sampling
                            let i420 = frame.buffer.to_i420();
                            let (sy, su, sv) = i420.strides();
                            let (dy, du, dv) = i420.data();

                            let ch = (h + 1) / 2;

                            // Ensure capacity and copy full plane slices
                            let y_size = (sy * h) as usize;
                            let u_size = (su * ch) as usize;
                            let v_size = (sv * ch) as usize;
                            if y_buf.len() != y_size { y_buf.resize(y_size, 0); }
                            if u_buf.len() != u_size { u_buf.resize(u_size, 0); }
                            if v_buf.len() != v_size { v_buf.resize(v_size, 0); }
                            y_buf.copy_from_slice(dy);
                            u_buf.copy_from_slice(du);
                            v_buf.copy_from_slice(dv);

                            // Swap buffers into shared state
                            let mut s = shared2.lock();
                            s.width = w as u32;
                            s.height = h as u32;
                            s.stride_y = sy as u32;
                            s.stride_u = su as u32;
                            s.stride_v = sv as u32;
                            std::mem::swap(&mut s.y, &mut y_buf);
                            std::mem::swap(&mut s.u, &mut u_buf);
                            std::mem::swap(&mut s.v, &mut v_buf);
                            s.dirty = true;

                            frames += 1;
                            let elapsed = last_log.elapsed();
                            if elapsed >= Duration::from_secs(2) {
                                let fps = frames as f64 / elapsed.as_secs_f64();
                                info!("Receiving video: {}x{}, ~{:.1} fps", w, h, fps);
                                frames = 0;
                                last_log = Instant::now();
                            }
                        }
                        info!("Video stream ended");
                    });
                    break;
                }
            }
        }
    });

    // Start UI
    let app = VideoApp { shared };
    let native_options = eframe::NativeOptions::default();
    eframe::run_native("LiveKit Video Subscriber", native_options, Box::new(|_| Ok::<Box<dyn eframe::App>, _>(Box::new(app))))?;

    Ok(())
}


// ===== WGPU I420 renderer =====

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
}

impl YuvGpuState {
    fn create_textures(device: &wgpu::Device, _width: u32, height: u32, y_pad_w: u32, uv_pad_w: u32) -> (wgpu::Texture, wgpu::Texture, wgpu::Texture, wgpu::TextureView, wgpu::TextureView, wgpu::TextureView) {
        let y_size = wgpu::Extent3d { width: y_pad_w, height, depth_or_array_layers: 1 };
        let uv_size = wgpu::Extent3d { width: uv_pad_w, height: (height + 1) / 2, depth_or_array_layers: 1 };
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

fn align_up(value: u32, alignment: u32) -> u32 {
    ((value + alignment - 1) / alignment) * alignment
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
    fn prepare(&self, device: &wgpu::Device, queue: &wgpu::Queue, _screen_desc: &egui_wgpu_backend::ScreenDescriptor, _encoder: &mut wgpu::CommandEncoder, resources: &mut egui_wgpu_backend::CallbackResources) -> Vec<wgpu::CommandBuffer> {
        // Initialize or update GPU state lazily based on current frame
        let mut shared = self.shared.lock();

        // Nothing to draw yet
        if shared.width == 0 || shared.height == 0 {
            return Vec::new();
        }

        // Fetch or create our GPU state
        if resources.get::<YuvGpuState>().is_none() {
            // Build pipeline and initial small textures; will be recreated on first upload
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
                    wgpu::BindGroupLayoutEntry { binding: 2, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }, count: None },
                    wgpu::BindGroupLayoutEntry { binding: 3, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }, count: None },
                    wgpu::BindGroupLayoutEntry {
                        binding: 4,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: Some(std::num::NonZeroU64::new(std::mem::size_of::<ParamsUniform>() as u64).unwrap()),
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
                vertex: wgpu::VertexState { module: &shader, entry_point: Some("vs_main"), buffers: &[], compilation_options: wgpu::PipelineCompilationOptions::default() },
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
                primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, strip_index_format: None, front_face: wgpu::FrontFace::Ccw, cull_mode: None, unclipped_depth: false, polygon_mode: wgpu::PolygonMode::Fill, conservative: false },
                depth_stencil: None,
                multisample: wgpu::MultisampleState { count: 1, mask: !0, alpha_to_coverage_enabled: false },
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
                contents: bytemuck::bytes_of(&ParamsUniform { src_w: 1, src_h: 1, y_tex_w: 1, uv_tex_w: 1 }),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });

            // Initial tiny textures
            let (y_tex, u_tex, v_tex, y_view, u_view, v_view) = YuvGpuState::create_textures(device, 1, 1, 256, 256);
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("yuv_bind_group"),
                layout: &bind_layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::Sampler(&sampler) },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&y_view) },
                    wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&u_view) },
                    wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(&v_view) },
                    wgpu::BindGroupEntry { binding: 4, resource: params_buf.as_entire_binding() },
                ],
            });

            let new_state = YuvGpuState {
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
            };
            resources.insert(new_state);
        }
        let state = resources.get_mut::<YuvGpuState>().unwrap();

        // Upload planes when marked dirty
        // Recreate textures/bind group on size change
        if state.dims != (shared.width, shared.height) {
            let y_pad_w = align_up(shared.width, 256);
            let uv_w = (shared.width + 1) / 2;
            let uv_pad_w = align_up(uv_w, 256);
            let (y_tex, u_tex, v_tex, y_view, u_view, v_view) = YuvGpuState::create_textures(device, shared.width, shared.height, y_pad_w, uv_pad_w);
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("yuv_bind_group"),
                layout: &state.bind_layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::Sampler(&state.sampler) },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&y_view) },
                    wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&u_view) },
                    wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(&v_view) },
                    wgpu::BindGroupEntry { binding: 4, resource: state.params_buf.as_entire_binding() },
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
            state.dims = (shared.width, shared.height);
        }

        if shared.dirty {
            let y_bytes_per_row = align_up(shared.width, 256);
            let uv_w = (shared.width + 1) / 2;
            let uv_h = (shared.height + 1) / 2;
            let uv_bytes_per_row = align_up(uv_w, 256);

            // Pack and upload Y
            if shared.stride_y >= shared.width {
                let mut packed = vec![0u8; (y_bytes_per_row * shared.height) as usize];
                for row in 0..shared.height {
                    let src = &shared.y[(row * shared.stride_y) as usize..][..shared.width as usize];
                    let dst_off = (row * y_bytes_per_row) as usize;
                    packed[dst_off..dst_off + shared.width as usize].copy_from_slice(src);
                }
                queue.write_texture(
                    wgpu::ImageCopyTexture {
                        texture: &state.y_tex,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    &packed,
                    wgpu::ImageDataLayout {
                        offset: 0,
                        bytes_per_row: Some(y_bytes_per_row),
                        rows_per_image: Some(shared.height),
                    },
                    wgpu::Extent3d { width: state.y_pad_w, height: shared.height, depth_or_array_layers: 1 },
                );
            }

            // Pack and upload U,V
            if shared.stride_u >= uv_w && shared.stride_v >= uv_w {
                let mut packed_u = vec![0u8; (uv_bytes_per_row * uv_h) as usize];
                let mut packed_v = vec![0u8; (uv_bytes_per_row * uv_h) as usize];
                for row in 0..uv_h {
                    let src_u = &shared.u[(row * shared.stride_u) as usize..][..uv_w as usize];
                    let src_v = &shared.v[(row * shared.stride_v) as usize..][..uv_w as usize];
                    let dst_off = (row * uv_bytes_per_row) as usize;
                    packed_u[dst_off..dst_off + uv_w as usize].copy_from_slice(src_u);
                    packed_v[dst_off..dst_off + uv_w as usize].copy_from_slice(src_v);
                }
                queue.write_texture(
                    wgpu::ImageCopyTexture { texture: &state.u_tex, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
                    &packed_u,
                    wgpu::ImageDataLayout { offset: 0, bytes_per_row: Some(uv_bytes_per_row), rows_per_image: Some(uv_h) },
                    wgpu::Extent3d { width: state.uv_pad_w, height: uv_h, depth_or_array_layers: 1 },
                );
                queue.write_texture(
                    wgpu::ImageCopyTexture { texture: &state.v_tex, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
                    &packed_v,
                    wgpu::ImageDataLayout { offset: 0, bytes_per_row: Some(uv_bytes_per_row), rows_per_image: Some(uv_h) },
                    wgpu::Extent3d { width: state.uv_pad_w, height: uv_h, depth_or_array_layers: 1 },
                );
            }

            // Update params uniform
            let params = ParamsUniform { src_w: shared.width, src_h: shared.height, y_tex_w: state.y_pad_w, uv_tex_w: state.uv_pad_w };
            queue.write_buffer(&state.params_buf, 0, bytemuck::bytes_of(&params));

            shared.dirty = false;
        }

        Vec::new()
    }

    fn paint(&self, _info: egui::PaintCallbackInfo, render_pass: &mut wgpu::RenderPass<'static>, resources: &egui_wgpu_backend::CallbackResources) {
        // Acquire device/queue via screen_descriptor? Not available; use resources to fetch our state
        let shared = self.shared.lock();
        if shared.width == 0 || shared.height == 0 {
            return;
        }

        // Build pipeline and textures on first paint or on resize
        let state_entry = resources.get::<YuvGpuState>().expect("YuvGpuState should be initialized in prepare");
        // We cannot mutate resources here; assume created already with correct dims
        let state = state_entry;

        if state.dims != (shared.width, shared.height) {
            // We cannot rebuild here (no device access); skip drawing until next frame where prepare will rebuild
            return;
        }

        render_pass.set_pipeline(&state.pipeline);
        render_pass.set_bind_group(0, &state.bind_group, &[]);
        // Fullscreen triangle without vertex buffer
        render_pass.draw(0..3, 0..1);
    }
}

// Build or rebuild GPU state. This helper is intended to be called from prepare, but we lack device there in current API constraints.
// Note: eframe/egui-wgpu provides device in paint via RenderPass context; however, to keep this example concise, we set up the state once externally.

