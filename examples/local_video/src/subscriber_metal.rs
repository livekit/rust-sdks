#![allow(unexpected_cfgs)]

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("subscriber_metal is only supported on macOS");
}

#[cfg(target_os = "macos")]
mod macos {
    extern crate objc;

    use std::{
        env,
        ffi::c_void,
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        },
        thread,
        time::{Duration, Instant, SystemTime, UNIX_EPOCH},
    };

    use anyhow::{anyhow, Result};
    use block::ConcreteBlock;
    use chrono::{DateTime, Utc};
    use clap::Parser;
    use core_foundation::base::{CFRelease, CFTypeRef};
    use core_graphics_types::geometry::CGSize;
    use core_video::{
        image_buffer::CVImageBufferRef,
        pixel_buffer::{
            kCVPixelFormatType_420YpCbCr8BiPlanarFullRange as CV_PIXEL_FORMAT_NV12_FULL_RANGE,
            kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange as CV_PIXEL_FORMAT_NV12_VIDEO_RANGE,
            CVPixelBufferGetHeightOfPlane, CVPixelBufferGetPixelFormatType,
            CVPixelBufferGetWidthOfPlane, CVPixelBufferRef,
        },
    };
    use futures::StreamExt;
    use livekit::e2ee::{key_provider::*, E2eeOptions, EncryptionType};
    use livekit::prelude::*;
    use livekit::webrtc::{
        video_frame::{native::RetainedCvPixelBuffer, BoxVideoFrame},
        video_stream::native::NativeVideoStream,
    };
    use livekit_api::access_token;
    use log::{debug, info, warn};
    use metal::foreign_types::ForeignTypeRef;
    use metal::*;
    use objc::{msg_send, rc::autoreleasepool, runtime::YES, sel, sel_impl};
    use parking_lot::Mutex;
    use winit::{
        application::ApplicationHandler,
        dpi::LogicalSize,
        event::WindowEvent,
        event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy},
        raw_window_handle::{HasWindowHandle, RawWindowHandle},
        window::{Window, WindowAttributes, WindowId},
    };

    #[allow(non_camel_case_types)]
    type cocoa_id = *mut objc::runtime::Object;

    type CVMetalTextureCacheRef = *mut c_void;
    type CVMetalTextureRef = *mut c_void;

    extern "C" {
        fn CVMetalTextureCacheCreate(
            allocator: *const c_void,
            cache_attributes: *const c_void,
            metal_device: *mut c_void,
            texture_attributes: *const c_void,
            cache_out: *mut CVMetalTextureCacheRef,
        ) -> i32;
        fn CVMetalTextureCacheCreateTextureFromImage(
            allocator: *const c_void,
            texture_cache: CVMetalTextureCacheRef,
            source_image: CVImageBufferRef,
            texture_attributes: *const c_void,
            pixel_format: MTLPixelFormat,
            width: usize,
            height: usize,
            plane_index: usize,
            texture_out: *mut CVMetalTextureRef,
        ) -> i32;
        fn CVMetalTextureGetTexture(image: CVMetalTextureRef) -> *mut MTLTexture;
    }

    struct CvMetalTextureCache {
        raw: CVMetalTextureCacheRef,
    }

    impl CvMetalTextureCache {
        fn new(device: &DeviceRef) -> Result<Self> {
            let mut raw = std::ptr::null_mut();
            let status = unsafe {
                CVMetalTextureCacheCreate(
                    std::ptr::null(),
                    std::ptr::null(),
                    device.as_ptr().cast(),
                    std::ptr::null(),
                    &mut raw,
                )
            };
            if status == 0 && !raw.is_null() {
                Ok(Self { raw })
            } else {
                Err(anyhow!("CVMetalTextureCacheCreate failed: {status}"))
            }
        }

        fn create_texture_from_image(
            &self,
            source_image: CVImageBufferRef,
            pixel_format: MTLPixelFormat,
            width: usize,
            height: usize,
            plane_index: usize,
        ) -> Result<CvMetalTexture> {
            let mut raw = std::ptr::null_mut();
            let status = unsafe {
                CVMetalTextureCacheCreateTextureFromImage(
                    std::ptr::null(),
                    self.raw,
                    source_image,
                    std::ptr::null(),
                    pixel_format,
                    width,
                    height,
                    plane_index,
                    &mut raw,
                )
            };
            if status == 0 && !raw.is_null() {
                Ok(CvMetalTexture { raw })
            } else {
                Err(anyhow!("CVMetalTextureCacheCreateTextureFromImage failed: {status}"))
            }
        }
    }

    impl Drop for CvMetalTextureCache {
        fn drop(&mut self) {
            if !self.raw.is_null() {
                unsafe {
                    CFRelease(self.raw as CFTypeRef);
                }
            }
        }
    }

    struct CvMetalTexture {
        raw: CVMetalTextureRef,
    }

    impl CvMetalTexture {
        fn get_texture(&self) -> Option<&TextureRef> {
            let texture = unsafe { CVMetalTextureGetTexture(self.raw) };
            if texture.is_null() {
                None
            } else {
                unsafe { Some(TextureRef::from_ptr(texture)) }
            }
        }
    }

    impl Drop for CvMetalTexture {
        fn drop(&mut self) {
            if !self.raw.is_null() {
                unsafe {
                    CFRelease(self.raw as CFTypeRef);
                }
            }
        }
    }

    #[derive(Parser, Debug, Clone)]
    #[command(author, version, about, long_about = None)]
    struct Args {
        /// LiveKit participant identity
        #[arg(long, default_value = "rust-video-subscriber-metal")]
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

        /// Only subscribe to video from this participant identity
        #[arg(long)]
        participant: Option<String>,

        /// Track timestamp/frame metadata in logs and the window title
        #[arg(long)]
        display_timestamp: bool,

        /// Shared encryption key for E2EE
        #[arg(long)]
        e2ee_key: Option<String>,
    }

    #[derive(Clone, Copy, Debug)]
    enum AppEvent {
        FrameReady,
        Shutdown,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum RenderPath {
        ZeroCopy,
        Upload,
        Unsupported,
    }

    impl RenderPath {
        fn as_str(self) -> &'static str {
            match self {
                Self::ZeroCopy => "zero_copy",
                Self::Upload => "upload",
                Self::Unsupported => "unsupported",
            }
        }
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum PlaneFormat {
        I420,
        Nv12,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct YuvParams {
        format: u32,
        full_range: u32,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct OverlayParams {
        origin: [f32; 2],
        size: [f32; 2],
        drawable_size: [f32; 2],
    }

    #[derive(Clone, Copy)]
    struct OverlaySnapshot {
        frame_id: Option<u32>,
        publish_us: Option<u64>,
        latency_us: u64,
    }

    struct SharedState {
        pending: Option<PendingFrame>,
        app_replaced: u64,
        sdk_dropped: u64,
        last_frame_id: Option<u32>,
        last_publish_at_us: Option<u64>,
        last_receive_latency_ms: Option<f64>,
        last_render_path: RenderPath,
        last_received_at_us: Option<u64>,
        last_render_start_us: Option<u64>,
        last_commit_us: Option<u64>,
        last_width: u32,
        last_height: u32,
    }

    impl Default for SharedState {
        fn default() -> Self {
            Self {
                pending: None,
                app_replaced: 0,
                sdk_dropped: 0,
                last_frame_id: None,
                last_publish_at_us: None,
                last_receive_latency_ms: None,
                last_render_path: RenderPath::Unsupported,
                last_received_at_us: None,
                last_render_start_us: None,
                last_commit_us: None,
                last_width: 0,
                last_height: 0,
            }
        }
    }

    struct PendingFrame {
        storage: PendingStorage,
        width: u32,
        height: u32,
        received_at_us: u64,
        frame_id: Option<u32>,
        publish_us: Option<u64>,
    }

    enum PendingStorage {
        Native { pixel_buffer: RetainedCvPixelBuffer, fallback: BoxVideoFrame },
        Upload(BoxVideoFrame),
    }

    struct MetalApp {
        shared: Arc<Mutex<SharedState>>,
        shutdown: Arc<AtomicBool>,
        wake_in_flight: Arc<AtomicBool>,
        window: Option<Window>,
        renderer: Option<MetalRenderer>,
        last_title_update: Instant,
        display_timestamp: bool,
    }

    impl MetalApp {
        fn new(
            shared: Arc<Mutex<SharedState>>,
            shutdown: Arc<AtomicBool>,
            wake_in_flight: Arc<AtomicBool>,
            display_timestamp: bool,
        ) -> Self {
            Self {
                shared,
                shutdown,
                wake_in_flight,
                window: None,
                renderer: None,
                last_title_update: Instant::now(),
                display_timestamp,
            }
        }

        fn ensure_window(&mut self, event_loop: &ActiveEventLoop) {
            if self.window.is_some() {
                return;
            }

            let attrs = WindowAttributes::default()
                .with_title("LiveKit Metal Video Subscriber")
                .with_inner_size(LogicalSize::new(960.0, 540.0));
            let window = match event_loop.create_window(attrs) {
                Ok(window) => window,
                Err(err) => {
                    log::error!("failed to create window: {err:?}");
                    event_loop.exit();
                    return;
                }
            };

            match MetalRenderer::new(&window) {
                Ok(renderer) => {
                    self.renderer = Some(renderer);
                    self.window = Some(window);
                }
                Err(err) => {
                    log::error!("failed to initialize Metal renderer: {err:?}");
                    event_loop.exit();
                }
            }
        }

        fn render_latest(&mut self) {
            let Some(renderer) = self.renderer.as_mut() else {
                return;
            };

            let pending = self.shared.lock().pending.take();
            let Some(frame) = pending else {
                return;
            };

            let render_start_us = current_timestamp_us();
            let receive_to_render_ms =
                render_start_us.saturating_sub(frame.received_at_us) as f64 / 1000.0;
            let overlay = if self.display_timestamp {
                let snapshot = OverlaySnapshot {
                    frame_id: frame.frame_id,
                    publish_us: frame.publish_us,
                    latency_us: current_timestamp_us(),
                };
                if snapshot.frame_id.is_none() && snapshot.publish_us.is_none() {
                    debug!(
                        "timestamp overlay unavailable for frame received_at_us={}: missing packet trailer metadata",
                        frame.received_at_us
                    );
                }
                Some(snapshot)
            } else {
                None
            };

            match renderer.render(frame, overlay) {
                Ok(result) => {
                    let commit_us = current_timestamp_us();
                    let mut shared = self.shared.lock();
                    shared.last_render_path = result.path;
                    shared.last_render_start_us = Some(render_start_us);
                    shared.last_commit_us = Some(commit_us);
                    shared.last_width = result.width;
                    shared.last_height = result.height;
                    drop(shared);
                    debug!(
                        "rendered frame path={} receive_to_render={receive_to_render_ms:.2}ms",
                        result.path.as_str()
                    );
                    self.update_title();
                }
                Err(err) => {
                    warn!("render failed: {err:?}");
                }
            }
        }

        fn update_title(&mut self) {
            if self.last_title_update.elapsed() < Duration::from_millis(250) {
                return;
            }
            let Some(window) = self.window.as_ref() else {
                return;
            };
            self.last_title_update = Instant::now();

            let shared = self.shared.lock();
            let frame_id =
                shared.last_frame_id.map(|id| id.to_string()).unwrap_or_else(|| "N/A".to_string());
            let recv = shared
                .last_receive_latency_ms
                .map(|lat| format!("{lat:.1}ms"))
                .unwrap_or_else(|| "N/A".to_string());
            let timing = if self.display_timestamp {
                format!(" frame={frame_id} recv={recv}")
            } else {
                String::new()
            };
            window.set_title(&format!(
                "LiveKit Metal Subscriber {}x{} path={}{} dropped={} replaced={}",
                shared.last_width,
                shared.last_height,
                shared.last_render_path.as_str(),
                timing,
                shared.sdk_dropped,
                shared.app_replaced
            ));
        }
    }

    impl ApplicationHandler<AppEvent> for MetalApp {
        fn resumed(&mut self, event_loop: &ActiveEventLoop) {
            event_loop.set_control_flow(ControlFlow::Wait);
            self.ensure_window(event_loop);
        }

        fn user_event(&mut self, event_loop: &ActiveEventLoop, event: AppEvent) {
            event_loop.set_control_flow(ControlFlow::Wait);
            match event {
                AppEvent::FrameReady => {
                    self.wake_in_flight.store(false, Ordering::Release);
                    self.render_latest();
                }
                AppEvent::Shutdown => {
                    self.shutdown.store(true, Ordering::Release);
                    event_loop.exit();
                }
            }
        }

        fn window_event(
            &mut self,
            event_loop: &ActiveEventLoop,
            _window_id: WindowId,
            event: WindowEvent,
        ) {
            event_loop.set_control_flow(ControlFlow::Wait);
            match event {
                WindowEvent::CloseRequested => {
                    self.shutdown.store(true, Ordering::Release);
                    event_loop.exit();
                }
                WindowEvent::Resized(size) => {
                    if let Some(renderer) = self.renderer.as_mut() {
                        renderer.resize(size.width, size.height);
                    }
                }
                WindowEvent::RedrawRequested => self.render_latest(),
                _ => {}
            }
        }
    }

    struct RenderResult {
        path: RenderPath,
        width: u32,
        height: u32,
    }

    enum RenderTextures {
        Native {
            cv_y: CvMetalTexture,
            cv_uv: CvMetalTexture,
            full_range: bool,
            width: u32,
            height: u32,
        },
        Upload {
            y: Texture,
            u: Texture,
            v: Option<Texture>,
            format: PlaneFormat,
            full_range: bool,
            width: u32,
            height: u32,
        },
    }

    impl RenderTextures {
        fn y_texture(&self) -> Result<&TextureRef> {
            match self {
                Self::Native { cv_y, .. } => {
                    cv_y.get_texture().ok_or_else(|| anyhow!("missing Y MTLTexture"))
                }
                Self::Upload { y, .. } => Ok(y),
            }
        }

        fn u_texture(&self) -> Result<&TextureRef> {
            match self {
                Self::Native { cv_uv, .. } => {
                    cv_uv.get_texture().ok_or_else(|| anyhow!("missing UV MTLTexture"))
                }
                Self::Upload { u, .. } => Ok(u),
            }
        }

        fn v_texture<'a>(&'a self, dummy_v: &'a TextureRef) -> Option<&'a TextureRef> {
            match self {
                Self::Native { .. } => Some(dummy_v),
                Self::Upload { v, .. } => v.as_ref().map(|texture| &**texture).or(Some(dummy_v)),
            }
        }

        fn format(&self) -> PlaneFormat {
            match self {
                Self::Native { .. } => PlaneFormat::Nv12,
                Self::Upload { format, .. } => *format,
            }
        }

        fn full_range(&self) -> bool {
            match self {
                Self::Native { full_range, .. } | Self::Upload { full_range, .. } => *full_range,
            }
        }

        fn width(&self) -> u32 {
            match self {
                Self::Native { width, .. } | Self::Upload { width, .. } => *width,
            }
        }

        fn height(&self) -> u32 {
            match self {
                Self::Native { height, .. } | Self::Upload { height, .. } => *height,
            }
        }
    }

    struct UploadTextures {
        y: Texture,
        u: Texture,
        v: Texture,
        dims: (u32, u32),
        format: PlaneFormat,
    }

    struct OverlayTexture {
        texture: Texture,
        width: u32,
        height: u32,
    }

    struct OverlayRenderer {
        textures: Vec<OverlayTexture>,
        active_texture: Option<usize>,
        next_texture: usize,
        last_text: String,
        last_latency_text: String,
        last_latency_refresh: Option<Instant>,
    }

    struct MetalRenderer {
        device: Device,
        layer: MetalLayer,
        command_queue: CommandQueue,
        pipeline: RenderPipelineState,
        overlay_pipeline: RenderPipelineState,
        sampler: SamplerState,
        texture_cache: CvMetalTextureCache,
        upload: Option<UploadTextures>,
        overlay: OverlayRenderer,
        dummy_v: Texture,
    }

    impl MetalRenderer {
        fn new(window: &Window) -> Result<Self> {
            let device =
                Device::system_default().ok_or_else(|| anyhow!("no Metal device found"))?;
            let mut layer = MetalLayer::new();
            layer.set_device(&device);
            layer.set_pixel_format(MTLPixelFormat::BGRA8Unorm);
            layer.set_presents_with_transaction(false);
            layer.set_display_sync_enabled(false);
            layer.set_maximum_drawable_count(2);
            layer.remove_all_animations();

            attach_layer(window, &mut layer)?;
            let size = window.inner_size();
            let scale = window.scale_factor();
            layer.set_contents_scale(scale);
            layer.set_drawable_size(CGSize::new(size.width as f64, size.height as f64));

            let command_queue = device.new_command_queue();
            let library = compile_yuv_library(&device)?;
            let pipeline = create_pipeline(&device, &library)?;
            let overlay_pipeline = create_overlay_pipeline(&device, &library)?;
            let sampler = create_sampler(&device);
            let texture_cache = CvMetalTextureCache::new(&device)?;
            let dummy_v = create_texture(&device, 1, 1, MTLPixelFormat::R8Unorm);

            Ok(Self {
                device,
                layer,
                command_queue,
                pipeline,
                overlay_pipeline,
                sampler,
                texture_cache,
                upload: None,
                overlay: OverlayRenderer::default(),
                dummy_v,
            })
        }

        fn resize(&mut self, width: u32, height: u32) {
            self.layer.set_drawable_size(CGSize::new(width.max(1) as f64, height.max(1) as f64));
        }

        fn render(
            &mut self,
            frame: PendingFrame,
            overlay: Option<OverlaySnapshot>,
        ) -> Result<RenderResult> {
            autoreleasepool(|| self.render_inner(frame, overlay))
        }

        fn render_inner(
            &mut self,
            frame: PendingFrame,
            overlay: Option<OverlaySnapshot>,
        ) -> Result<RenderResult> {
            let frame_width = frame.width;
            let frame_height = frame.height;
            let (textures, path, completion_guard) = match frame.storage {
                PendingStorage::Native { pixel_buffer, fallback } => {
                    match self.native_textures(pixel_buffer, frame_width, frame_height) {
                        Ok((textures, guard)) => (textures, RenderPath::ZeroCopy, Some(guard)),
                        Err(err) => {
                            warn!(
                                "native CVPixelBuffer render path unavailable, using upload fallback: {err:?}"
                            );
                            let textures = self.upload_textures(&fallback)?;
                            (textures, RenderPath::Upload, None)
                        }
                    }
                }
                PendingStorage::Upload(frame) => {
                    let textures = self.upload_textures(&frame)?;
                    (textures, RenderPath::Upload, None)
                }
            };

            let Some(drawable) = self.layer.next_drawable() else {
                return Ok(RenderResult { path, width: frame_width, height: frame_height });
            };
            let drawable_size = self.layer.drawable_size();
            let overlay_texture = overlay
                .as_ref()
                .and_then(|snapshot| self.overlay.prepare(&self.device, *snapshot, path));
            let render_pass_descriptor = RenderPassDescriptor::new();
            let color_attachment = render_pass_descriptor.color_attachments().object_at(0).unwrap();
            color_attachment.set_texture(Some(drawable.texture()));
            color_attachment.set_load_action(MTLLoadAction::Clear);
            color_attachment.set_clear_color(MTLClearColor::new(0.0, 0.0, 0.0, 1.0));
            color_attachment.set_store_action(MTLStoreAction::Store);

            let command_buffer = self.command_queue.new_command_buffer();
            let encoder = command_buffer.new_render_command_encoder(render_pass_descriptor);
            encoder.set_render_pipeline_state(&self.pipeline);
            encoder.set_fragment_texture(0, Some(textures.y_texture()?));
            encoder.set_fragment_texture(1, Some(textures.u_texture()?));
            encoder.set_fragment_texture(2, textures.v_texture(&self.dummy_v));
            encoder.set_fragment_sampler_state(0, Some(&self.sampler));
            let params = YuvParams {
                format: match textures.format() {
                    PlaneFormat::I420 => 0,
                    PlaneFormat::Nv12 => 1,
                },
                full_range: u32::from(textures.full_range()),
            };
            encoder.set_fragment_bytes(
                0,
                std::mem::size_of::<YuvParams>() as u64,
                (&params as *const YuvParams).cast::<c_void>(),
            );
            encoder.set_viewport(fit_viewport(drawable_size, textures.width(), textures.height()));
            encoder.draw_primitives(MTLPrimitiveType::Triangle, 0, 3);

            if let Some(overlay) = overlay_texture {
                encode_overlay(
                    &encoder,
                    &self.overlay_pipeline,
                    &self.sampler,
                    overlay,
                    drawable_size,
                );
            }

            encoder.end_encoding();

            if let Some(guard) = completion_guard {
                let block = ConcreteBlock::new(move |_cmd: &CommandBufferRef| {
                    let _ = &guard;
                })
                .copy();
                command_buffer.add_completed_handler(&block);
            }

            command_buffer.present_drawable(drawable);
            command_buffer.commit();

            Ok(RenderResult { path, width: frame_width, height: frame_height })
        }

        fn native_textures(
            &self,
            pixel_buffer: RetainedCvPixelBuffer,
            fallback_width: u32,
            fallback_height: u32,
        ) -> Result<(RenderTextures, RetainedCvPixelBuffer)> {
            let ptr = pixel_buffer.as_ptr() as CVPixelBufferRef;
            if ptr.is_null() {
                return Err(anyhow!("null CVPixelBuffer"));
            }

            let pixel_format = unsafe { CVPixelBufferGetPixelFormatType(ptr) };
            let full_range = match pixel_format {
                CV_PIXEL_FORMAT_NV12_VIDEO_RANGE => false,
                CV_PIXEL_FORMAT_NV12_FULL_RANGE => true,
                other => return Err(anyhow!("unsupported CVPixelBuffer pixel format: {other:#x}")),
            };
            let y_w = unsafe { CVPixelBufferGetWidthOfPlane(ptr, 0) as u32 }.max(fallback_width);
            let y_h = unsafe { CVPixelBufferGetHeightOfPlane(ptr, 0) as u32 }.max(fallback_height);
            let uv_w = unsafe { CVPixelBufferGetWidthOfPlane(ptr, 1) as u32 }.max(1);
            let uv_h = unsafe { CVPixelBufferGetHeightOfPlane(ptr, 1) as u32 }.max(1);
            let source = ptr as CVImageBufferRef;

            let cv_y = self
                .texture_cache
                .create_texture_from_image(
                    source,
                    MTLPixelFormat::R8Unorm,
                    y_w as usize,
                    y_h as usize,
                    0,
                )
                .map_err(|status| anyhow!("create Y CVMetalTexture failed: {status:?}"))?;
            let cv_uv = self
                .texture_cache
                .create_texture_from_image(
                    source,
                    MTLPixelFormat::RG8Unorm,
                    uv_w as usize,
                    uv_h as usize,
                    1,
                )
                .map_err(|status| anyhow!("create UV CVMetalTexture failed: {status:?}"))?;
            cv_y.get_texture().ok_or_else(|| anyhow!("missing Y MTLTexture"))?;
            cv_uv.get_texture().ok_or_else(|| anyhow!("missing UV MTLTexture"))?;

            Ok((
                RenderTextures::Native { cv_y, cv_uv, full_range, width: y_w, height: y_h },
                pixel_buffer,
            ))
        }

        fn upload_textures(&mut self, frame: &BoxVideoFrame) -> Result<RenderTextures> {
            let width = frame.buffer.width();
            let height = frame.buffer.height();
            if let Some(nv12) = frame.buffer.as_nv12() {
                self.ensure_upload_textures(width, height, PlaneFormat::Nv12);
                let upload = self.upload.as_ref().unwrap();
                let uv_w = width.div_ceil(2);
                let uv_h = height.div_ceil(2);
                let (stride_y, stride_uv) = nv12.strides();
                let (data_y, data_uv) = nv12.data();
                replace_plane(&upload.y, data_y, stride_y, width, height);
                replace_plane(&upload.u, data_uv, stride_uv, uv_w, uv_h);
                return Ok(RenderTextures::Upload {
                    y: upload.y.clone(),
                    u: upload.u.clone(),
                    v: None,
                    format: PlaneFormat::Nv12,
                    full_range: false,
                    width,
                    height,
                });
            }

            self.ensure_upload_textures(width, height, PlaneFormat::I420);
            let converted;
            let i420 = if let Some(i420) = frame.buffer.as_i420() {
                i420
            } else {
                converted = frame.buffer.to_i420();
                &converted
            };
            let upload = self.upload.as_ref().unwrap();
            let uv_w = width.div_ceil(2);
            let uv_h = height.div_ceil(2);
            let (stride_y, stride_u, stride_v) = i420.strides();
            let (data_y, data_u, data_v) = i420.data();
            replace_plane(&upload.y, data_y, stride_y, width, height);
            replace_plane(&upload.u, data_u, stride_u, uv_w, uv_h);
            replace_plane(&upload.v, data_v, stride_v, uv_w, uv_h);
            Ok(RenderTextures::Upload {
                y: upload.y.clone(),
                u: upload.u.clone(),
                v: Some(upload.v.clone()),
                format: PlaneFormat::I420,
                full_range: false,
                width,
                height,
            })
        }

        fn ensure_upload_textures(&mut self, width: u32, height: u32, format: PlaneFormat) {
            let dims = (width, height);
            if self
                .upload
                .as_ref()
                .is_some_and(|upload| upload.dims == dims && upload.format == format)
            {
                return;
            }

            let uv_w = width.div_ceil(2).max(1);
            let uv_h = height.div_ceil(2).max(1);
            let y =
                create_texture(&self.device, width.max(1), height.max(1), MTLPixelFormat::R8Unorm);
            let u_format = match format {
                PlaneFormat::I420 => MTLPixelFormat::R8Unorm,
                PlaneFormat::Nv12 => MTLPixelFormat::RG8Unorm,
            };
            let u = create_texture(&self.device, uv_w, uv_h, u_format);
            let v = create_texture(&self.device, uv_w, uv_h, MTLPixelFormat::R8Unorm);
            self.upload = Some(UploadTextures { y, u, v, dims, format });
        }
    }

    impl Default for OverlayRenderer {
        fn default() -> Self {
            Self {
                textures: Vec::new(),
                active_texture: None,
                next_texture: 0,
                last_text: String::new(),
                last_latency_text: String::new(),
                last_latency_refresh: None,
            }
        }
    }

    impl OverlayRenderer {
        fn prepare(
            &mut self,
            device: &DeviceRef,
            snapshot: OverlaySnapshot,
            path: RenderPath,
        ) -> Option<&OverlayTexture> {
            if snapshot.frame_id.is_none() && snapshot.publish_us.is_none() {
                return None;
            }

            let text = self.overlay_text(snapshot, path);
            if text != self.last_text {
                let bitmap = rasterize_overlay(&text);
                if self.textures.len() < 3 {
                    self.textures.push(create_overlay_texture(device, bitmap.width, bitmap.height));
                }
                let index = self.next_texture % self.textures.len();
                let needs_texture = self.textures[index].width != bitmap.width
                    || self.textures[index].height != bitmap.height;
                if needs_texture {
                    self.textures[index] =
                        create_overlay_texture(device, bitmap.width, bitmap.height);
                }
                let texture = &self.textures[index];
                replace_rgba_texture(&texture.texture, &bitmap.pixels, bitmap.width, bitmap.height);
                self.active_texture = Some(index);
                self.next_texture = (index + 1) % 3;
                self.last_text = text;
            }

            self.active_texture.and_then(|index| self.textures.get(index))
        }

        fn overlay_text(&mut self, snapshot: OverlaySnapshot, _path: RenderPath) -> String {
            let frame_id =
                snapshot.frame_id.map(|id| id.to_string()).unwrap_or_else(|| "N/A".to_string());

            if let Some(publish_us) = snapshot.publish_us {
                let latency = self.latency_text(publish_us, snapshot.latency_us);
                format!(
                    "Frame ID:   {frame_id}\nPublish:    {}\nRender:     {}\nLatency:    {latency}",
                    format_optional_timestamp_us(Some(publish_us)),
                    format_optional_timestamp_us(Some(snapshot.latency_us)),
                )
            } else {
                format!("Frame ID:   {frame_id}")
            }
        }

        fn latency_text(&mut self, publish_us: u64, latency_us: u64) -> String {
            let should_refresh = self.last_latency_text.is_empty()
                || self
                    .last_latency_refresh
                    .is_none_or(|last| last.elapsed() >= Duration::from_millis(500));
            if should_refresh {
                self.last_latency_text =
                    format!("{:.1}ms", latency_us.saturating_sub(publish_us) as f64 / 1000.0);
                self.last_latency_refresh = Some(Instant::now());
            }

            self.last_latency_text.clone()
        }
    }

    struct OverlayBitmap {
        pixels: Vec<u8>,
        width: u32,
        height: u32,
    }

    fn create_overlay_texture(device: &DeviceRef, width: u32, height: u32) -> OverlayTexture {
        OverlayTexture {
            texture: create_texture(device, width, height, MTLPixelFormat::RGBA8Unorm),
            width,
            height,
        }
    }

    fn rasterize_overlay(text: &str) -> OverlayBitmap {
        const FONT_W: usize = 5;
        const FONT_H: usize = 7;
        const SCALE_NUM: usize = 3;
        const SCALE_DEN: usize = 2;
        const PAD: usize = 6;
        const CELL_W: usize = 9;
        const LINE_H: usize = 14;

        let lines: Vec<&str> = text.lines().collect();
        let max_cols = lines.iter().map(|line| line.chars().count()).max().unwrap_or(1);
        let glyph_w = scaled_ceil(FONT_W, SCALE_NUM, SCALE_DEN);
        let glyph_h = scaled_ceil(FONT_H, SCALE_NUM, SCALE_DEN);
        let width = (PAD * 2 + max_cols.saturating_sub(1) * CELL_W + glyph_w).max(1);
        let height = (PAD * 2 + lines.len().saturating_sub(1) * LINE_H + glyph_h).max(1);
        let mut pixels = vec![0u8; width * height * 4];

        for px in pixels.chunks_exact_mut(4) {
            px[0] = 0;
            px[1] = 0;
            px[2] = 0;
            px[3] = 150;
        }

        for (row, line) in lines.iter().enumerate() {
            let y = PAD + row * LINE_H;
            for (col, ch) in line.chars().enumerate() {
                draw_glyph(&mut pixels, width, height, PAD + col * CELL_W, y, ch);
            }
        }

        OverlayBitmap { pixels, width: width as u32, height: height as u32 }
    }

    fn draw_glyph(pixels: &mut [u8], width: usize, height: usize, x: usize, y: usize, ch: char) {
        const SCALE_NUM: usize = 3;
        const SCALE_DEN: usize = 2;
        let rows = glyph(ch.to_ascii_uppercase());
        for (gy, row) in rows.iter().enumerate() {
            for gx in 0..5 {
                if row & (1 << (4 - gx)) == 0 {
                    continue;
                }
                let x0 = x + scaled_floor(gx, SCALE_NUM, SCALE_DEN);
                let x1 = x + scaled_ceil(gx + 1, SCALE_NUM, SCALE_DEN);
                let y0 = y + scaled_floor(gy, SCALE_NUM, SCALE_DEN);
                let y1 = y + scaled_ceil(gy + 1, SCALE_NUM, SCALE_DEN);
                for py in y0..y1 {
                    for px in x0..x1 {
                        if px >= width || py >= height {
                            continue;
                        }
                        let idx = (py * width + px) * 4;
                        pixels[idx] = 255;
                        pixels[idx + 1] = 255;
                        pixels[idx + 2] = 255;
                        pixels[idx + 3] = 255;
                    }
                }
            }
        }
    }

    fn scaled_floor(value: usize, num: usize, den: usize) -> usize {
        value * num / den
    }

    fn scaled_ceil(value: usize, num: usize, den: usize) -> usize {
        (value * num).div_ceil(den)
    }

    fn glyph(ch: char) -> [u8; 7] {
        match ch {
            'A' => [0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001],
            'B' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110],
            'C' => [0b01110, 0b10001, 0b10000, 0b10000, 0b10000, 0b10001, 0b01110],
            'D' => [0b11110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b11110],
            'E' => [0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111],
            'F' => [0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000],
            'G' => [0b01110, 0b10001, 0b10000, 0b10111, 0b10001, 0b10001, 0b01110],
            'H' => [0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001],
            'I' => [0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b11111],
            'J' => [0b00111, 0b00010, 0b00010, 0b00010, 0b10010, 0b10010, 0b01100],
            'K' => [0b10001, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001],
            'L' => [0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111],
            'M' => [0b10001, 0b11011, 0b10101, 0b10101, 0b10001, 0b10001, 0b10001],
            'N' => [0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001, 0b10001],
            'O' => [0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110],
            'P' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000],
            'Q' => [0b01110, 0b10001, 0b10001, 0b10001, 0b10101, 0b10010, 0b01101],
            'R' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001],
            'S' => [0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110],
            'T' => [0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100],
            'U' => [0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110],
            'V' => [0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01010, 0b00100],
            'W' => [0b10001, 0b10001, 0b10001, 0b10101, 0b10101, 0b10101, 0b01010],
            'X' => [0b10001, 0b10001, 0b01010, 0b00100, 0b01010, 0b10001, 0b10001],
            'Y' => [0b10001, 0b10001, 0b01010, 0b00100, 0b00100, 0b00100, 0b00100],
            'Z' => [0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b11111],
            '0' => [0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110],
            '1' => [0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110],
            '2' => [0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111],
            '3' => [0b11110, 0b00001, 0b00001, 0b01110, 0b00001, 0b00001, 0b11110],
            '4' => [0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010],
            '5' => [0b11111, 0b10000, 0b10000, 0b11110, 0b00001, 0b00001, 0b11110],
            '6' => [0b01110, 0b10000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110],
            '7' => [0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000],
            '8' => [0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110],
            '9' => [0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00001, 0b01110],
            ':' => [0b00000, 0b00100, 0b00100, 0b00000, 0b00100, 0b00100, 0b00000],
            '.' => [0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b00100, 0b00100],
            '-' => [0b00000, 0b00000, 0b00000, 0b11111, 0b00000, 0b00000, 0b00000],
            '/' => [0b00001, 0b00010, 0b00010, 0b00100, 0b01000, 0b01000, 0b10000],
            '_' => [0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b11111],
            ' ' => [0; 7],
            _ => [0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b00000, 0b00100],
        }
    }

    fn attach_layer(window: &Window, layer: &mut MetalLayer) -> Result<()> {
        let raw = window.window_handle()?.as_raw();
        unsafe {
            let RawWindowHandle::AppKit(rw) = raw else {
                return Err(anyhow!("expected AppKit window handle"));
            };
            let view = rw.ns_view.as_ptr() as cocoa_id;
            let _: () = msg_send![view, setWantsLayer: YES];
            let layer_ptr = layer.as_mut() as *mut _ as cocoa_id;
            let _: () = msg_send![view, setLayer: layer_ptr];
        }
        Ok(())
    }

    fn compile_yuv_library(device: &DeviceRef) -> Result<Library> {
        let source = include_str!("metal_yuv.metal");
        let options = CompileOptions::new();
        device
            .new_library_with_source(source, &options)
            .map_err(|err| anyhow!("failed to compile Metal shader: {err}"))
    }

    fn create_pipeline(device: &DeviceRef, library: &LibraryRef) -> Result<RenderPipelineState> {
        let vertex = library
            .get_function("yuv_vertex", None)
            .map_err(|err| anyhow!("missing yuv_vertex: {err}"))?;
        let fragment = library
            .get_function("yuv_fragment", None)
            .map_err(|err| anyhow!("missing yuv_fragment: {err}"))?;
        let descriptor = RenderPipelineDescriptor::new();
        descriptor.set_vertex_function(Some(&vertex));
        descriptor.set_fragment_function(Some(&fragment));
        let attachment = descriptor.color_attachments().object_at(0).unwrap();
        attachment.set_pixel_format(MTLPixelFormat::BGRA8Unorm);
        device
            .new_render_pipeline_state(&descriptor)
            .map_err(|err| anyhow!("failed to create render pipeline: {err}"))
    }

    fn create_overlay_pipeline(
        device: &DeviceRef,
        library: &LibraryRef,
    ) -> Result<RenderPipelineState> {
        let vertex = library
            .get_function("overlay_vertex", None)
            .map_err(|err| anyhow!("missing overlay_vertex: {err}"))?;
        let fragment = library
            .get_function("overlay_fragment", None)
            .map_err(|err| anyhow!("missing overlay_fragment: {err}"))?;
        let descriptor = RenderPipelineDescriptor::new();
        descriptor.set_vertex_function(Some(&vertex));
        descriptor.set_fragment_function(Some(&fragment));
        let attachment = descriptor.color_attachments().object_at(0).unwrap();
        attachment.set_pixel_format(MTLPixelFormat::BGRA8Unorm);
        attachment.set_blending_enabled(true);
        attachment.set_source_rgb_blend_factor(MTLBlendFactor::SourceAlpha);
        attachment.set_destination_rgb_blend_factor(MTLBlendFactor::OneMinusSourceAlpha);
        attachment.set_rgb_blend_operation(MTLBlendOperation::Add);
        attachment.set_source_alpha_blend_factor(MTLBlendFactor::One);
        attachment.set_destination_alpha_blend_factor(MTLBlendFactor::OneMinusSourceAlpha);
        attachment.set_alpha_blend_operation(MTLBlendOperation::Add);
        device
            .new_render_pipeline_state(&descriptor)
            .map_err(|err| anyhow!("failed to create overlay render pipeline: {err}"))
    }

    fn encode_overlay(
        encoder: &RenderCommandEncoderRef,
        pipeline: &RenderPipelineStateRef,
        sampler: &SamplerStateRef,
        overlay: &OverlayTexture,
        drawable_size: CGSize,
    ) {
        let drawable_w = drawable_size.width.max(1.0) as f32;
        let drawable_h = drawable_size.height.max(1.0) as f32;
        let params = OverlayParams {
            origin: [10.0, 10.0],
            size: [overlay.width as f32, overlay.height as f32],
            drawable_size: [drawable_w, drawable_h],
        };
        encoder.set_render_pipeline_state(pipeline);
        encoder.set_vertex_bytes(
            0,
            std::mem::size_of::<OverlayParams>() as u64,
            (&params as *const OverlayParams).cast::<c_void>(),
        );
        encoder.set_fragment_texture(0, Some(&overlay.texture));
        encoder.set_fragment_sampler_state(0, Some(sampler));
        encoder.draw_primitives(MTLPrimitiveType::TriangleStrip, 0, 4);
    }

    fn create_sampler(device: &DeviceRef) -> SamplerState {
        let descriptor = SamplerDescriptor::new();
        descriptor.set_min_filter(MTLSamplerMinMagFilter::Linear);
        descriptor.set_mag_filter(MTLSamplerMinMagFilter::Linear);
        descriptor.set_address_mode_s(MTLSamplerAddressMode::ClampToEdge);
        descriptor.set_address_mode_t(MTLSamplerAddressMode::ClampToEdge);
        device.new_sampler(&descriptor)
    }

    fn create_texture(
        device: &DeviceRef,
        width: u32,
        height: u32,
        pixel_format: MTLPixelFormat,
    ) -> Texture {
        let descriptor = TextureDescriptor::new();
        descriptor.set_texture_type(MTLTextureType::D2);
        descriptor.set_pixel_format(pixel_format);
        descriptor.set_width(width as u64);
        descriptor.set_height(height as u64);
        descriptor.set_mipmap_level_count(1);
        descriptor.set_usage(MTLTextureUsage::ShaderRead);
        descriptor.set_storage_mode(MTLStorageMode::Managed);
        device.new_texture(&descriptor)
    }

    fn replace_rgba_texture(texture: &TextureRef, data: &[u8], width: u32, height: u32) {
        if width == 0 || height == 0 || data.is_empty() {
            return;
        }
        let region = MTLRegion {
            origin: MTLOrigin { x: 0, y: 0, z: 0 },
            size: MTLSize { width: width as u64, height: height as u64, depth: 1 },
        };
        texture.replace_region(region, 0, data.as_ptr().cast(), width as u64 * 4);
    }

    fn replace_plane(texture: &TextureRef, data: &[u8], stride: u32, width: u32, height: u32) {
        if width == 0 || height == 0 || data.is_empty() {
            return;
        }
        let region = MTLRegion {
            origin: MTLOrigin { x: 0, y: 0, z: 0 },
            size: MTLSize { width: width as u64, height: height as u64, depth: 1 },
        };
        texture.replace_region(region, 0, data.as_ptr().cast(), stride as u64);
    }

    fn fit_viewport(drawable_size: CGSize, width: u32, height: u32) -> MTLViewport {
        let dw = drawable_size.width.max(1.0);
        let dh = drawable_size.height.max(1.0);
        let aspect = width.max(1) as f64 / height.max(1) as f64;
        let mut vw = dw;
        let mut vh = vw / aspect;
        if vh > dh {
            vh = dh;
            vw = vh * aspect;
        }
        MTLViewport {
            originX: (dw - vw) * 0.5,
            originY: (dh - vh) * 0.5,
            width: vw,
            height: vh,
            znear: 0.0,
            zfar: 1.0,
        }
    }

    fn current_timestamp_us() -> u64 {
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_micros() as u64
    }

    fn format_timestamp_us(ts_us: u64) -> String {
        DateTime::<Utc>::from_timestamp_micros(ts_us as i64)
            .map(|dt| {
                dt.format("%Y-%m-%d %H:%M:%S:").to_string()
                    + &format!("{:03}", dt.timestamp_subsec_millis())
            })
            .unwrap_or_else(|| format!("<invalid timestamp {ts_us}>"))
    }

    fn format_optional_timestamp_us(ts_us: Option<u64>) -> String {
        ts_us.map(format_timestamp_us).unwrap_or_else(|| "N/A".to_string())
    }

    fn codec_label(mime: &str) -> String {
        let base = mime.split(';').next().unwrap_or(mime).trim();
        let last = base.rsplit('/').next().unwrap_or(base).trim();
        last.to_ascii_uppercase()
    }

    async fn wait_for_shutdown(flag: Arc<AtomicBool>) {
        while !flag.load(Ordering::Acquire) {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    fn start_livekit_thread(
        args: Args,
        shared: Arc<Mutex<SharedState>>,
        proxy: EventLoopProxy<AppEvent>,
        shutdown: Arc<AtomicBool>,
        wake_in_flight: Arc<AtomicBool>,
    ) -> thread::JoinHandle<()> {
        thread::spawn(move || {
            let runtime = match tokio::runtime::Builder::new_multi_thread().enable_all().build() {
                Ok(runtime) => runtime,
                Err(err) => {
                    log::error!("failed to create Tokio runtime: {err:?}");
                    let _ = proxy.send_event(AppEvent::Shutdown);
                    return;
                }
            };
            if let Err(err) =
                runtime.block_on(run_livekit(args, shared, proxy.clone(), shutdown, wake_in_flight))
            {
                log::error!("subscriber_metal LiveKit task failed: {err:?}");
                let _ = proxy.send_event(AppEvent::Shutdown);
            }
        })
    }

    async fn run_livekit(
        args: Args,
        shared: Arc<Mutex<SharedState>>,
        proxy: EventLoopProxy<AppEvent>,
        shutdown: Arc<AtomicBool>,
        wake_in_flight: Arc<AtomicBool>,
    ) -> Result<()> {
        tokio::spawn({
            let shutdown = shutdown.clone();
            let proxy = proxy.clone();
            async move {
                let _ = tokio::signal::ctrl_c().await;
                shutdown.store(true, Ordering::Release);
                let _ = proxy.send_event(AppEvent::Shutdown);
            }
        });

        let url = args.url.or_else(|| env::var("LIVEKIT_URL").ok()).expect(
            "LiveKit URL must be provided via --url argument or LIVEKIT_URL environment variable",
        );
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

        let mut room_options = RoomOptions::default();
        room_options.auto_subscribe = true;
        room_options.dynacast = false;
        room_options.adaptive_stream = false;

        if let Some(ref e2ee_key) = args.e2ee_key {
            let key_provider = KeyProvider::with_shared_key(
                KeyProviderOptions::default(),
                e2ee_key.as_bytes().to_vec(),
            );
            room_options.encryption =
                Some(E2eeOptions { encryption_type: EncryptionType::Gcm, key_provider });
            info!("E2EE enabled with AES-GCM encryption");
        }

        info!(
            "Connecting to LiveKit room '{}' as '{}' with Metal low-latency subscriber...",
            args.room_name, args.identity
        );
        let (room, _) = Room::connect(&url, &token, room_options).await?;
        let room = Arc::new(room);
        info!("Connected: {} - {}", room.name(), room.sid().await);

        if args.e2ee_key.is_some() {
            room.e2ee_manager().set_enabled(true);
            info!("End-to-end encryption activated");
        }

        let allowed_identity = args.participant.clone();
        let active_sid = Arc::new(Mutex::new(None::<TrackSid>));
        let mut events = room.subscribe();
        info!("Subscribed to room events");

        while !shutdown.load(Ordering::Acquire) {
            tokio::select! {
                _ = wait_for_shutdown(shutdown.clone()) => break,
                maybe_evt = events.recv() => {
                    let Some(evt) = maybe_evt else { break };
                    match evt {
                        RoomEvent::TrackSubscribed { track, publication, participant } => {
                            handle_track_subscribed(
                                track,
                                publication,
                                participant,
                                &allowed_identity,
                                &shared,
                                &active_sid,
                                proxy.clone(),
                                shutdown.clone(),
                                wake_in_flight.clone(),
                                args.display_timestamp,
                            ).await;
                        }
                        RoomEvent::TrackUnsubscribed { publication, .. }
                        | RoomEvent::TrackUnpublished { publication, .. } => {
                            let sid = publication.sid().clone();
                            let mut active = active_sid.lock();
                            if active.as_ref() == Some(&sid) {
                                info!("Video track removed ({}), clearing active sink", sid);
                                *active = None;
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        Ok(())
    }

    async fn handle_track_subscribed(
        track: livekit::track::RemoteTrack,
        publication: RemoteTrackPublication,
        participant: RemoteParticipant,
        allowed_identity: &Option<String>,
        shared: &Arc<Mutex<SharedState>>,
        active_sid: &Arc<Mutex<Option<TrackSid>>>,
        proxy: EventLoopProxy<AppEvent>,
        shutdown: Arc<AtomicBool>,
        wake_in_flight: Arc<AtomicBool>,
        display_timestamp: bool,
    ) {
        if let Some(ref allow) = allowed_identity {
            if participant.identity().as_str() != allow {
                debug!(
                    "Skipping track from '{}' (filter set to '{}')",
                    participant.identity(),
                    allow
                );
                return;
            }
        }

        let livekit::track::RemoteTrack::Video(video_track) = track else {
            return;
        };

        let sid = publication.sid().clone();
        {
            let mut active = active_sid.lock();
            if active.as_ref() == Some(&sid) {
                return;
            }
            if active.is_some() {
                debug!(
                    "A video track is already active ({}), ignoring new subscribe {}",
                    active.as_ref().unwrap(),
                    sid
                );
                return;
            }
            *active = Some(sid.clone());
        }

        info!(
            "Subscribed to video track: {} (sid {}) from {} - codec: {}, label: {}, simulcast: {}, dimension: {}x{}",
            publication.name(),
            publication.sid(),
            participant.identity(),
            publication.mime_type(),
            codec_label(&publication.mime_type()),
            publication.simulcasted(),
            publication.dimension().0,
            publication.dimension().1,
        );

        let rtc_track = video_track.rtc_track();
        let shared = shared.clone();
        let active_sid = active_sid.clone();
        tokio::spawn(async move {
            let mut sink = NativeVideoStream::latest(rtc_track);
            let mut frames: u64 = 0;
            let mut last_log = Instant::now();
            let mut logged_first = false;

            loop {
                if shutdown.load(Ordering::Acquire) {
                    break;
                }
                let next = tokio::select! {
                    _ = wait_for_shutdown(shutdown.clone()) => None,
                    frame = sink.next() => frame,
                };
                let Some(frame) = next else { break };
                let received_at_us = current_timestamp_us();
                let width = frame.buffer.width();
                let height = frame.buffer.height();
                let metadata = frame.frame_metadata;
                let frame_id = metadata.and_then(|m| m.frame_id);
                let publish_us = metadata.and_then(|m| m.user_timestamp);
                let receive_latency_ms = metadata
                    .and_then(|m| m.user_timestamp)
                    .map(|published| received_at_us.saturating_sub(published) as f64 / 1000.0);

                if !logged_first {
                    debug!(
                        "First Metal subscriber frame: {}x{}, type {:?}",
                        width,
                        height,
                        frame.buffer.buffer_type()
                    );
                    logged_first = true;
                }

                let storage = if let Some(native) = frame.buffer.as_native() {
                    if let Some(pixel_buffer) = native.retained_cv_pixel_buffer() {
                        PendingStorage::Native { pixel_buffer, fallback: frame }
                    } else {
                        PendingStorage::Upload(frame)
                    }
                } else {
                    PendingStorage::Upload(frame)
                };

                {
                    let mut state = shared.lock();
                    if state
                        .pending
                        .replace(PendingFrame {
                            storage,
                            width,
                            height,
                            received_at_us,
                            frame_id,
                            publish_us,
                        })
                        .is_some()
                    {
                        state.app_replaced += 1;
                    }
                    state.sdk_dropped = sink.dropped_frames();
                    state.last_frame_id = frame_id;
                    state.last_publish_at_us = publish_us;
                    state.last_receive_latency_ms = receive_latency_ms;
                    state.last_received_at_us = Some(received_at_us);
                }
                if !wake_in_flight.swap(true, Ordering::AcqRel) {
                    if proxy.send_event(AppEvent::FrameReady).is_err() {
                        wake_in_flight.store(false, Ordering::Release);
                    }
                }

                frames += 1;
                let elapsed = last_log.elapsed();
                if elapsed >= Duration::from_secs(2) {
                    let state = shared.lock();
                    let fps = frames as f64 / elapsed.as_secs_f64();
                    let recv = state
                        .last_receive_latency_ms
                        .map(|lat| format!("{lat:.1}ms"))
                        .unwrap_or_else(|| "N/A".to_string());
                    info!(
                        "metal subscriber: {}x{}, ~{:.1} fps, path={}, frame_id={:?}, recv={}, sdk_dropped={}, app_replaced={}",
                        state.last_width.max(width),
                        state.last_height.max(height),
                        fps,
                        state.last_render_path.as_str(),
                        state.last_frame_id,
                        recv,
                        state.sdk_dropped,
                        state.app_replaced
                    );
                    frames = 0;
                    last_log = Instant::now();
                }
            }

            info!("Video stream ended for {}", sid);
            let mut active = active_sid.lock();
            if active.as_ref() == Some(&sid) {
                *active = None;
            }
        });

        if display_timestamp
            && !publication
                .packet_trailer_features()
                .contains(&PacketTrailerFeature::PtfUserTimestamp)
        {
            warn!("publisher did not advertise PTF_USER_TIMESTAMP; receive latency title/log fields will be N/A");
        }
    }

    pub fn main() -> Result<()> {
        env_logger::init();
        let args = Args::parse();
        let event_loop = EventLoop::<AppEvent>::with_user_event().build()?;
        event_loop.set_control_flow(ControlFlow::Wait);
        let proxy = event_loop.create_proxy();
        let shared = Arc::new(Mutex::new(SharedState::default()));
        let shutdown = Arc::new(AtomicBool::new(false));
        let wake_in_flight = Arc::new(AtomicBool::new(false));
        let _livekit_thread = start_livekit_thread(
            args.clone(),
            shared.clone(),
            proxy,
            shutdown.clone(),
            wake_in_flight.clone(),
        );
        let mut app = MetalApp::new(shared, shutdown, wake_in_flight, args.display_timestamp);
        event_loop.run_app(&mut app)?;
        Ok(())
    }
}

#[cfg(target_os = "macos")]
fn main() -> anyhow::Result<()> {
    macos::main()
}
