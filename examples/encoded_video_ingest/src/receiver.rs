// Copyright 2026 LiveKit, Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Encoded ingest receiver with an in-process WGPU visualizer.
//!
//! Subscribes to a LiveKit room and renders the first incoming video track
//! directly in an `egui`/`wgpu` window.
//!
//! NOTE: the current SDK only exposes *decoded* frames on the receive side
//! (via `NativeVideoStream`). WebRTC's internal decoder runs in-process
//! before we hand the frame to the application. Encoded-frame receive is
//! a future enhancement — see README.md.

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::{anyhow, Result};
use clap::Parser;
use eframe::Renderer;
use futures::StreamExt;
use livekit::{
    prelude::*,
    webrtc::{
        native::yuv_helper,
        prelude::{RtcVideoTrack, VideoBuffer},
        video_stream::native::{NativeVideoStream, NativeVideoStreamOptions},
    },
};
use livekit_api::access_token;
use log::{info, warn};
use parking_lot::Mutex;
use tokio::sync::mpsc;

#[derive(Parser, Debug, Clone)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// LiveKit server URL (or set LIVEKIT_URL env var)
    #[arg(long, env = "LIVEKIT_URL")]
    url: String,

    /// LiveKit API key (or set LIVEKIT_API_KEY env var)
    #[arg(long, env = "LIVEKIT_API_KEY")]
    api_key: String,

    /// LiveKit API secret (or set LIVEKIT_API_SECRET env var)
    #[arg(long, env = "LIVEKIT_API_SECRET")]
    api_secret: String,

    /// Room name to join
    #[arg(long, default_value = "encoded-video-demo")]
    room: String,

    /// Participant identity
    #[arg(long, default_value = "encoded-receiver")]
    identity: String,

    /// Only subscribe to the track from this participant identity
    #[arg(long)]
    from: Option<String>,

    /// Enable vsync for smoother display at the cost of extra render latency
    #[arg(long, default_value_t = false)]
    vsync: bool,
}

fn main() -> Result<()> {
    env_logger::init();
    let args = Args::parse();
    let present_mode = if args.vsync {
        eframe::wgpu::PresentMode::AutoVsync
    } else {
        eframe::wgpu::PresentMode::AutoNoVsync
    };

    eframe::run_native(
        "LiveKit Encoded Video Receiver",
        eframe::NativeOptions {
            centered: true,
            renderer: Renderer::Wgpu,
            vsync: args.vsync,
            wgpu_options: egui_wgpu::WgpuConfiguration {
                present_mode,
                desired_maximum_frame_latency: Some(1),
                ..Default::default()
            },
            ..Default::default()
        },
        Box::new(|cc| Ok(Box::new(ReceiverApp::new(cc, args)))),
    )
    .map_err(|err| anyhow!("receiver UI failed: {err}"))?;

    Ok(())
}

enum UiEvent {
    Connected { room: Arc<Room>, sid: RoomSid },
    ConnectFailed { error: String },
    RoomEvent { event: RoomEvent },
}

struct ReceiverApp {
    async_runtime: tokio::runtime::Runtime,
    ui_rx: mpsc::UnboundedReceiver<UiEvent>,
    room: Option<Arc<Room>>,
    render_state: egui_wgpu::RenderState,
    renderer: Option<VideoRenderer>,
    active_sid: Option<TrackSid>,
    active_label: Option<String>,
    from: Option<String>,
    status: String,
}

impl ReceiverApp {
    fn new(cc: &eframe::CreationContext<'_>, args: Args) -> Self {
        let async_runtime =
            tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap();
        let (ui_tx, ui_rx) = mpsc::unbounded_channel();
        async_runtime.spawn(connect_task(args.clone(), ui_tx));

        Self {
            async_runtime,
            ui_rx,
            room: None,
            render_state: cc.wgpu_render_state.clone().unwrap(),
            renderer: None,
            active_sid: None,
            active_label: None,
            from: args.from,
            status: format!("Connecting to room '{}' as '{}'...", args.room, args.identity),
        }
    }

    fn event(&mut self, event: UiEvent) {
        match event {
            UiEvent::Connected { room, sid } => {
                self.status = format!("Connected to room '{}' (sid {})", room.name(), sid);
                self.room = Some(room);
            }
            UiEvent::ConnectFailed { error } => {
                self.status = format!("Connection failed: {error}");
            }
            UiEvent::RoomEvent { event } => self.room_event(event),
        }
    }

    fn room_event(&mut self, event: RoomEvent) {
        match event {
            RoomEvent::TrackSubscribed { track, publication, participant } => {
                if let Some(from) = &self.from {
                    if participant.identity().as_str() != from {
                        return;
                    }
                }

                let RemoteTrack::Video(video) = track else {
                    return;
                };

                if self.active_sid.is_some() {
                    info!(
                        "Ignoring extra video track {} (already have one active)",
                        publication.sid()
                    );
                    return;
                }

                let sid = publication.sid();
                let label = format!(
                    "{} from '{}': codec={}, {}x{}",
                    sid,
                    participant.identity(),
                    publication.mime_type(),
                    publication.dimension().0,
                    publication.dimension().1,
                );

                info!("Subscribed to {label}");
                self.renderer = Some(VideoRenderer::new(
                    self.async_runtime.handle(),
                    self.render_state.clone(),
                    video.rtc_track(),
                ));
                self.active_sid = Some(sid);
                self.active_label = Some(label.clone());
                self.status = format!("Rendering {label}");
            }
            RoomEvent::TrackUnsubscribed { publication, .. }
            | RoomEvent::TrackUnpublished { publication, .. } => {
                if self.active_sid.as_ref() == Some(&publication.sid()) {
                    info!("Track {} ended", publication.sid());
                    self.renderer = None;
                    self.active_sid = None;
                    self.active_label = None;
                    self.status = "Waiting for a video track...".to_string();
                }
            }
            RoomEvent::Disconnected { reason } => {
                self.renderer = None;
                self.active_sid = None;
                self.active_label = None;
                self.room = None;
                self.status = format!("Disconnected: {reason:?}");
            }
            _ => {}
        }
    }

    fn draw_video(&self, ui: &mut egui::Ui) {
        let rect = ui.available_rect_before_wrap();
        let response = ui.allocate_rect(rect, egui::Sense::hover());
        let rect = response.rect;

        ui.painter().rect_filled(rect, egui::CornerRadius::default(), egui::Color32::BLACK);

        let Some(renderer) = &self.renderer else {
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                &self.status,
                egui::FontId::proportional(18.0),
                egui::Color32::WHITE,
            );
            return;
        };

        let resolution = renderer.resolution();
        if let Some(texture_id) = renderer.texture_id() {
            let image_rect = fit_rect(rect, resolution.0, resolution.1);
            ui.painter().image(
                texture_id,
                image_rect,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
        }

        ui.painter().text(
            egui::pos2(rect.min.x + 8.0, rect.max.y - 8.0),
            egui::Align2::LEFT_BOTTOM,
            format!(
                "{}x{} {}",
                resolution.0,
                resolution.1,
                self.active_label.as_deref().unwrap_or("")
            ),
            egui::FontId::default(),
            egui::Color32::WHITE,
        );
    }
}

impl eframe::App for ReceiverApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        while let Ok(event) = self.ui_rx.try_recv() {
            self.event(event);
        }

        egui::TopBottomPanel::top("status_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(&self.status);
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            self.draw_video(ui);
        });

        ctx.request_repaint();
    }
}

impl Drop for ReceiverApp {
    fn drop(&mut self) {
        if let Some(room) = self.room.take() {
            if let Err(err) = self.async_runtime.block_on(room.close()) {
                warn!("room.close: {err}");
            }
        }
    }
}

async fn connect_task(args: Args, ui_tx: mpsc::UnboundedSender<UiEvent>) {
    let token = match access_token::AccessToken::with_api_key(&args.api_key, &args.api_secret)
        .with_identity(&args.identity)
        .with_name(&args.identity)
        .with_grants(access_token::VideoGrants {
            room_join: true,
            room: args.room.clone(),
            can_subscribe: true,
            ..Default::default()
        })
        .to_jwt()
    {
        Ok(token) => token,
        Err(err) => {
            let _ = ui_tx.send(UiEvent::ConnectFailed { error: err.to_string() });
            return;
        }
    };

    info!("Connecting to LiveKit room '{}' as '{}'...", args.room, args.identity);
    let mut room_options = RoomOptions::default();
    room_options.auto_subscribe = true;
    room_options.adaptive_stream = false;

    match Room::connect(&args.url, &token, room_options).await {
        Ok((room, events)) => {
            let sid = room.sid().await;
            let room = Arc::new(room);
            info!("Connected: {} (sid {})", room.name(), sid);
            let _ = ui_tx.send(UiEvent::Connected { room, sid });
            tokio::spawn(room_event_task(events, ui_tx));
        }
        Err(err) => {
            let _ = ui_tx.send(UiEvent::ConnectFailed { error: err.to_string() });
        }
    }
}

async fn room_event_task(
    mut events: mpsc::UnboundedReceiver<RoomEvent>,
    ui_tx: mpsc::UnboundedSender<UiEvent>,
) {
    while let Some(event) = events.recv().await {
        let _ = ui_tx.send(UiEvent::RoomEvent { event });
    }
}

fn fit_rect(container: egui::Rect, width: u32, height: u32) -> egui::Rect {
    if width == 0 || height == 0 {
        return container;
    }

    let source_aspect = width as f32 / height as f32;
    let container_aspect = container.width() / container.height();
    let size = if container_aspect > source_aspect {
        egui::vec2(container.height() * source_aspect, container.height())
    } else {
        egui::vec2(container.width(), container.width() / source_aspect)
    };

    egui::Rect::from_center_size(container.center(), size)
}

struct VideoRenderer {
    internal: Arc<Mutex<RendererInternal>>,

    #[allow(dead_code)]
    rtc_track: RtcVideoTrack,
}

struct RendererInternal {
    render_state: egui_wgpu::RenderState,
    width: u32,
    height: u32,
    rgba_data: Vec<u8>,
    texture: Option<eframe::wgpu::Texture>,
    texture_view: Option<eframe::wgpu::TextureView>,
    egui_texture: Option<egui::TextureId>,
}

impl VideoRenderer {
    fn new(
        async_handle: &tokio::runtime::Handle,
        render_state: egui_wgpu::RenderState,
        rtc_track: RtcVideoTrack,
    ) -> Self {
        let internal = Arc::new(Mutex::new(RendererInternal {
            render_state,
            width: 0,
            height: 0,
            rgba_data: Vec::default(),
            texture: None,
            texture_view: None,
            egui_texture: None,
        }));

        let mut video_sink = NativeVideoStream::with_options(
            rtc_track.clone(),
            NativeVideoStreamOptions { queue_size_frames: Some(1) },
        );
        std::thread::spawn({
            let async_handle = async_handle.clone();
            let internal = internal.clone();
            move || {
                let mut frames: u64 = 0;
                let mut last_log = Instant::now();
                while let Some(frame) = async_handle.block_on(video_sink.next()) {
                    let mut internal = internal.lock();
                    let buffer = frame.buffer.as_ref();
                    let width = buffer.width();
                    let height = buffer.height();

                    internal.ensure_texture_size(width, height);
                    convert_to_abgr(buffer, &mut internal.rgba_data);

                    internal.render_state.queue.write_texture(
                        eframe::wgpu::TexelCopyTextureInfo {
                            texture: internal.texture.as_ref().unwrap(),
                            mip_level: 0,
                            origin: eframe::wgpu::Origin3d::default(),
                            aspect: eframe::wgpu::TextureAspect::default(),
                        },
                        &internal.rgba_data,
                        eframe::wgpu::TexelCopyBufferLayout {
                            bytes_per_row: Some(width * 4),
                            ..Default::default()
                        },
                        eframe::wgpu::Extent3d { width, height, ..Default::default() },
                    );

                    frames += 1;
                    if last_log.elapsed() >= Duration::from_secs(2) {
                        info!(
                            "recv: {}x{}, ~{:.1} fps",
                            width,
                            height,
                            frames as f64 / last_log.elapsed().as_secs_f64()
                        );
                        frames = 0;
                        last_log = Instant::now();
                    }
                }
                info!("frame renderer ended");
            }
        });

        Self { rtc_track, internal }
    }

    fn resolution(&self) -> (u32, u32) {
        let internal = self.internal.lock();
        (internal.width, internal.height)
    }

    fn texture_id(&self) -> Option<egui::TextureId> {
        self.internal.lock().egui_texture
    }
}

fn convert_to_abgr(buffer: &dyn VideoBuffer, dst: &mut [u8]) {
    let width = buffer.width();
    let height = buffer.height();
    let stride = width * 4;

    if let Some(buffer) = buffer.as_i420() {
        let (stride_y, stride_u, stride_v) = buffer.strides();
        let (data_y, data_u, data_v) = buffer.data();
        yuv_helper::i420_to_abgr(
            data_y,
            stride_y,
            data_u,
            stride_u,
            data_v,
            stride_v,
            dst,
            stride,
            width as i32,
            height as i32,
        );
        return;
    }

    if let Some(buffer) = buffer.as_nv12() {
        let (stride_y, stride_uv) = buffer.strides();
        let (data_y, data_uv) = buffer.data();
        yuv_helper::nv12_to_abgr(
            data_y,
            stride_y,
            data_uv,
            stride_uv,
            dst,
            stride,
            width as i32,
            height as i32,
        );
        return;
    }

    if let Some(buffer) = buffer.as_i422() {
        let (stride_y, stride_u, stride_v) = buffer.strides();
        let (data_y, data_u, data_v) = buffer.data();
        yuv_helper::i422_to_abgr(
            data_y,
            stride_y,
            data_u,
            stride_u,
            data_v,
            stride_v,
            dst,
            stride,
            width as i32,
            height as i32,
        );
        return;
    }

    if let Some(buffer) = buffer.as_i444() {
        let (stride_y, stride_u, stride_v) = buffer.strides();
        let (data_y, data_u, data_v) = buffer.data();
        yuv_helper::i444_to_abgr(
            data_y,
            stride_y,
            data_u,
            stride_u,
            data_v,
            stride_v,
            dst,
            stride,
            width as i32,
            height as i32,
        );
        return;
    }

    if let Some(buffer) = buffer.as_i010() {
        let (stride_y, stride_u, stride_v) = buffer.strides();
        let (data_y, data_u, data_v) = buffer.data();
        yuv_helper::i010_to_abgr(
            data_y,
            stride_y,
            data_u,
            stride_u,
            data_v,
            stride_v,
            dst,
            stride,
            width as i32,
            height as i32,
        );
        return;
    }

    let buffer = buffer.to_i420();
    let (stride_y, stride_u, stride_v) = buffer.strides();
    let (data_y, data_u, data_v) = buffer.data();
    yuv_helper::i420_to_abgr(
        data_y,
        stride_y,
        data_u,
        stride_u,
        data_v,
        stride_v,
        dst,
        stride,
        width as i32,
        height as i32,
    );
}

impl RendererInternal {
    fn ensure_texture_size(&mut self, width: u32, height: u32) {
        if self.width == width && self.height == height {
            return;
        }

        self.width = width;
        self.height = height;
        self.rgba_data.resize((width * height * 4) as usize, 0);

        self.texture =
            Some(self.render_state.device.create_texture(&eframe::wgpu::TextureDescriptor {
                label: Some("lk-receiver-texture"),
                usage: eframe::wgpu::TextureUsages::TEXTURE_BINDING
                    | eframe::wgpu::TextureUsages::COPY_DST,
                dimension: eframe::wgpu::TextureDimension::D2,
                size: eframe::wgpu::Extent3d { width, height, ..Default::default() },
                sample_count: 1,
                mip_level_count: 1,
                format: eframe::wgpu::TextureFormat::Rgba8Unorm,
                view_formats: &[eframe::wgpu::TextureFormat::Rgba8Unorm],
            }));

        self.texture_view = Some(self.texture.as_mut().unwrap().create_view(
            &eframe::wgpu::TextureViewDescriptor {
                label: Some("lk-receiver-texture-view"),
                format: Some(eframe::wgpu::TextureFormat::Rgba8Unorm),
                dimension: Some(eframe::wgpu::TextureViewDimension::D2),
                mip_level_count: Some(1),
                array_layer_count: Some(1),
                ..Default::default()
            },
        ));

        if let Some(texture_id) = self.egui_texture {
            self.render_state.renderer.write().update_egui_texture_from_wgpu_texture(
                &self.render_state.device,
                self.texture_view.as_ref().unwrap(),
                eframe::wgpu::FilterMode::Linear,
                texture_id,
            );
        } else {
            self.egui_texture = Some(self.render_state.renderer.write().register_native_texture(
                &self.render_state.device,
                self.texture_view.as_ref().unwrap(),
                eframe::wgpu::FilterMode::Linear,
            ));
        }
    }
}
