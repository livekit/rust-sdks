use anyhow::Result;
use clap::Parser;
use eframe::egui;
use eframe::wgpu::{self, util::DeviceExt};
use egui_wgpu as egui_wgpu_backend;
use egui_wgpu_backend::CallbackTrait;
use futures::StreamExt;
use livekit::e2ee::{key_provider::*, E2eeOptions, EncryptionType};
use livekit::prelude::*;
use livekit::webrtc::video_frame::BoxVideoFrame;
use livekit::webrtc::video_stream::native::NativeVideoStream;
use livekit_api::access_token;
use log::{debug, info};
use parking_lot::Mutex;
use std::{
    collections::HashMap,
    env,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

mod timestamp_burn;
mod viewport_aspect;

use timestamp_burn::{format_timestamp_us, LatencyDisplay};
use viewport_aspect::AspectConstrainedViewport;

#[cfg(target_os = "macos")]
mod macos_native_video {
    use std::ffi::c_void;

    use anyhow::{anyhow, Result};
    use eframe::wgpu;
    use metal::{foreign_types::ForeignType, MTLPixelFormat, MTLTextureType};

    use livekit::webrtc::video_frame::BoxVideoFrame;

    type CVReturn = i32;
    type OSType = u32;
    type CVPixelBufferRef = *mut c_void;
    type CVImageBufferRef = *mut c_void;
    type CVMetalTextureCacheRef = *mut c_void;
    type CVMetalTextureRef = *mut c_void;
    type CFAllocatorRef = *const c_void;
    type CFDictionaryRef = *const c_void;
    type CFTypeRef = *const c_void;
    type Id = *mut c_void;

    const K_CV_RETURN_SUCCESS: CVReturn = 0;
    const K_CV_PIXEL_FORMAT_TYPE_420YPCBCR8_BIPLANAR_VIDEO_RANGE: OSType = 0x3432_3076;
    const K_CV_PIXEL_FORMAT_TYPE_420YPCBCR8_BIPLANAR_FULL_RANGE: OSType = 0x3432_3066;

    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFRelease(cf: CFTypeRef);
    }

    #[link(name = "CoreVideo", kind = "framework")]
    unsafe extern "C" {
        fn CVMetalTextureCacheCreate(
            allocator: CFAllocatorRef,
            cache_attributes: CFDictionaryRef,
            metal_device: Id,
            texture_attributes: CFDictionaryRef,
            cache_out: *mut CVMetalTextureCacheRef,
        ) -> CVReturn;
        fn CVMetalTextureCacheCreateTextureFromImage(
            allocator: CFAllocatorRef,
            texture_cache: CVMetalTextureCacheRef,
            source_image: CVImageBufferRef,
            texture_attributes: CFDictionaryRef,
            pixel_format: MTLPixelFormat,
            width: usize,
            height: usize,
            plane_index: usize,
            texture_out: *mut CVMetalTextureRef,
        ) -> CVReturn;
        fn CVMetalTextureGetTexture(image: CVMetalTextureRef) -> Id;
        fn CVPixelBufferGetPixelFormatType(pixel_buffer: CVPixelBufferRef) -> OSType;
        fn CVPixelBufferGetPlaneCount(pixel_buffer: CVPixelBufferRef) -> usize;
        fn CVPixelBufferGetWidthOfPlane(
            pixel_buffer: CVPixelBufferRef,
            plane_index: usize,
        ) -> usize;
        fn CVPixelBufferGetHeightOfPlane(
            pixel_buffer: CVPixelBufferRef,
            plane_index: usize,
        ) -> usize;
    }

    #[link(name = "objc")]
    unsafe extern "C" {
        fn objc_retain(obj: Id) -> Id;
    }

    pub(crate) struct CvMetalTextureCache {
        raw: CVMetalTextureCacheRef,
    }

    // SAFETY: The cache object is immutable after creation and CoreVideo objects are ref-counted.
    unsafe impl Send for CvMetalTextureCache {}
    // SAFETY: Calls through the cache are synchronized by CoreVideo/Metal; we never mutate Rust state through it.
    unsafe impl Sync for CvMetalTextureCache {}

    impl CvMetalTextureCache {
        pub(crate) fn new(device: &wgpu::Device) -> Result<Self> {
            let raw_device = unsafe {
                // SAFETY: We only inspect the backend device and copy out the retained MTLDevice
                // pointer for CoreVideo cache creation.
                let hal_device = device
                    .as_hal::<wgpu::hal::api::Metal>()
                    .ok_or_else(|| anyhow!("wgpu is not using the Metal backend"))?;
                let raw_device = hal_device.raw_device().lock().as_ptr() as Id;
                raw_device
            };

            let mut cache = std::ptr::null_mut();
            let status = unsafe {
                // SAFETY: CoreVideo writes a retained cache object into `cache` when it succeeds.
                CVMetalTextureCacheCreate(
                    std::ptr::null(),
                    std::ptr::null(),
                    raw_device,
                    std::ptr::null(),
                    &mut cache,
                )
            };
            if status != K_CV_RETURN_SUCCESS || cache.is_null() {
                return Err(anyhow!("CVMetalTextureCacheCreate failed with status {status}"));
            }

            Ok(Self { raw: cache })
        }
    }

    impl Drop for CvMetalTextureCache {
        fn drop(&mut self) {
            if !self.raw.is_null() {
                unsafe {
                    // SAFETY: `raw` is a non-null CoreFoundation object returned retained by CoreVideo.
                    CFRelease(self.raw as CFTypeRef)
                };
            }
        }
    }

    pub(crate) struct NativeFrameResources {
        _y_cv_texture: CvMetalTexture,
        _uv_cv_texture: CvMetalTexture,
        _frame: BoxVideoFrame,
    }

    // SAFETY: The contained native handles are ref-counted and only kept alive for rendering.
    unsafe impl Send for NativeFrameResources {}
    // SAFETY: The struct is used as lifetime storage; it does not provide interior mutation.
    unsafe impl Sync for NativeFrameResources {}

    struct CvMetalTexture {
        raw: CVMetalTextureRef,
    }

    impl Drop for CvMetalTexture {
        fn drop(&mut self) {
            if !self.raw.is_null() {
                unsafe {
                    // SAFETY: `raw` is a non-null CoreFoundation object returned retained by CoreVideo.
                    CFRelease(self.raw as CFTypeRef)
                };
            }
        }
    }

    pub(crate) struct ImportedNativeFrame {
        pub(crate) y_tex: wgpu::Texture,
        pub(crate) uv_tex: wgpu::Texture,
        pub(crate) y_view: wgpu::TextureView,
        pub(crate) uv_view: wgpu::TextureView,
        pub(crate) resources: NativeFrameResources,
        pub(crate) full_size: (u32, u32),
        pub(crate) uv_size: (u32, u32),
    }

    pub(crate) fn import_nv12_frame(
        device: &wgpu::Device,
        cache: &CvMetalTextureCache,
        frame: BoxVideoFrame,
    ) -> Result<ImportedNativeFrame> {
        let native = frame
            .buffer
            .as_native()
            .ok_or_else(|| anyhow!("frame is not backed by a native buffer"))?;
        let pixel_buffer = native.get_cv_pixel_buffer() as CVPixelBufferRef;
        if pixel_buffer.is_null() {
            return Err(anyhow!("native buffer is not backed by a CVPixelBuffer"));
        }

        let pixel_format = unsafe {
            // SAFETY: `pixel_buffer` was returned by the native frame and checked for null.
            CVPixelBufferGetPixelFormatType(pixel_buffer)
        };
        if pixel_format != K_CV_PIXEL_FORMAT_TYPE_420YPCBCR8_BIPLANAR_VIDEO_RANGE
            && pixel_format != K_CV_PIXEL_FORMAT_TYPE_420YPCBCR8_BIPLANAR_FULL_RANGE
        {
            return Err(anyhow!("unsupported CVPixelBuffer pixel format 0x{pixel_format:08x}"));
        }

        let plane_count = unsafe {
            // SAFETY: `pixel_buffer` was returned by the native frame and checked for null.
            CVPixelBufferGetPlaneCount(pixel_buffer)
        };
        if plane_count != 2 {
            return Err(anyhow!("expected 2-plane NV12 CVPixelBuffer, got {plane_count} planes"));
        }

        let y_w = unsafe {
            // SAFETY: The pixel buffer reported exactly two planes, so plane 0 is valid.
            CVPixelBufferGetWidthOfPlane(pixel_buffer, 0)
        };
        let y_h = unsafe {
            // SAFETY: The pixel buffer reported exactly two planes, so plane 0 is valid.
            CVPixelBufferGetHeightOfPlane(pixel_buffer, 0)
        };
        let uv_w = unsafe {
            // SAFETY: The pixel buffer reported exactly two planes, so plane 1 is valid.
            CVPixelBufferGetWidthOfPlane(pixel_buffer, 1)
        };
        let uv_h = unsafe {
            // SAFETY: The pixel buffer reported exactly two planes, so plane 1 is valid.
            CVPixelBufferGetHeightOfPlane(pixel_buffer, 1)
        };
        if y_w == 0 || y_h == 0 || uv_w == 0 || uv_h == 0 {
            return Err(anyhow!("CVPixelBuffer has an empty plane"));
        }

        let y_cv_texture =
            create_cv_metal_texture(cache, pixel_buffer, MTLPixelFormat::R8Unorm, y_w, y_h, 0)?;
        let uv_cv_texture =
            create_cv_metal_texture(cache, pixel_buffer, MTLPixelFormat::RG8Unorm, uv_w, uv_h, 1)?;

        let y_mtl = retained_metal_texture(y_cv_texture.raw)?;
        let uv_mtl = retained_metal_texture(uv_cv_texture.raw)?;
        let y_tex = create_wgpu_texture_from_metal(
            device,
            y_mtl,
            wgpu::TextureFormat::R8Unorm,
            y_w as u32,
            y_h as u32,
            "cvpixelbuffer_y_plane",
        )?;
        let uv_tex = create_wgpu_texture_from_metal(
            device,
            uv_mtl,
            wgpu::TextureFormat::Rg8Unorm,
            uv_w as u32,
            uv_h as u32,
            "cvpixelbuffer_uv_plane",
        )?;
        let y_view = y_tex.create_view(&wgpu::TextureViewDescriptor::default());
        let uv_view = uv_tex.create_view(&wgpu::TextureViewDescriptor::default());

        Ok(ImportedNativeFrame {
            y_tex,
            uv_tex,
            y_view,
            uv_view,
            resources: NativeFrameResources {
                _y_cv_texture: y_cv_texture,
                _uv_cv_texture: uv_cv_texture,
                _frame: frame,
            },
            full_size: (y_w as u32, y_h as u32),
            uv_size: (uv_w as u32, uv_h as u32),
        })
    }

    fn create_cv_metal_texture(
        cache: &CvMetalTextureCache,
        pixel_buffer: CVPixelBufferRef,
        pixel_format: MTLPixelFormat,
        width: usize,
        height: usize,
        plane_index: usize,
    ) -> Result<CvMetalTexture> {
        let mut texture = std::ptr::null_mut();
        let status = unsafe {
            // SAFETY: The cache and pixel buffer are valid CoreVideo objects and `texture` is an
            // out-pointer for CoreVideo to fill with a retained CVMetalTexture.
            CVMetalTextureCacheCreateTextureFromImage(
                std::ptr::null(),
                cache.raw,
                pixel_buffer as CVImageBufferRef,
                std::ptr::null(),
                pixel_format,
                width,
                height,
                plane_index,
                &mut texture,
            )
        };
        if status != K_CV_RETURN_SUCCESS || texture.is_null() {
            return Err(anyhow!(
                "CVMetalTextureCacheCreateTextureFromImage failed for plane {plane_index} with status {status}"
            ));
        }
        Ok(CvMetalTexture { raw: texture })
    }

    fn retained_metal_texture(cv_texture: CVMetalTextureRef) -> Result<metal::Texture> {
        let raw_texture = unsafe {
            // SAFETY: `cv_texture` is a non-null CVMetalTexture returned by CoreVideo.
            CVMetalTextureGetTexture(cv_texture)
        };
        if raw_texture.is_null() {
            return Err(anyhow!("CVMetalTextureGetTexture returned null"));
        }
        let retained = unsafe {
            // SAFETY: `raw_texture` is a live Objective-C object. Retaining transfers ownership
            // to the `metal::Texture` wrapper below.
            objc_retain(raw_texture)
        };
        if retained.is_null() {
            return Err(anyhow!("objc_retain returned null for MTLTexture"));
        }
        Ok(unsafe {
            // SAFETY: The pointer was retained above and is an MTLTexture.
            metal::Texture::from_ptr(retained.cast())
        })
    }

    fn create_wgpu_texture_from_metal(
        device: &wgpu::Device,
        metal_texture: metal::Texture,
        format: wgpu::TextureFormat,
        width: u32,
        height: u32,
        label: &'static str,
    ) -> Result<wgpu::Texture> {
        let desc = wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        };

        let hal_texture = unsafe {
            // SAFETY: The raw MTLTexture is retained and the descriptor matches its plane format,
            // type, layer count, mip count, and copy extent.
            wgpu::hal::metal::Device::texture_from_raw(
                metal_texture,
                format,
                MTLTextureType::D2,
                1,
                1,
                wgpu::hal::CopyExtent { width, height, depth: 1 },
            )
        };

        Ok(unsafe {
            // SAFETY: The hal texture was created for this Metal-backed wgpu device with a
            // descriptor matching the wrapped native texture.
            device.create_texture_from_hal::<wgpu::hal::api::Metal>(hal_texture, &desc)
        })
    }
}

async fn wait_for_shutdown(flag: Arc<AtomicBool>) {
    while !flag.load(Ordering::Acquire) {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

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

    /// Only subscribe to video from this participant identity
    #[arg(long)]
    participant: Option<String>,

    /// Display frame timing and stats over the rendered video
    #[arg(long)]
    display_timestamp: bool,

    /// Shared encryption key for E2EE (enables AES-GCM end-to-end encryption when set; must match publisher's key)
    #[arg(long)]
    e2ee_key: Option<String>,
}

struct SharedYuv {
    width: u32,
    height: u32,
    frame: Option<BoxVideoFrame>,
    codec: String,
    fps: f32,
    dirty: bool,
    /// Time when the latest frame became available to the subscriber code.
    received_at_us: Option<u64>,
    /// Packet-trailer metadata from the most recent frame, if any.
    frame_metadata: Option<livekit::webrtc::video_frame::FrameMetadata>,
    /// Latest frame whose GPU submit has completed; lags CPU receive by ~1 display frame.
    gpu_done: Option<GpuDoneSample>,
}

#[derive(Clone, Copy, Debug)]
struct GpuDoneSample {
    frame_id: Option<u32>,
    capture_timestamp_us: Option<u64>,
    cpu_received_us: u64,
    gpu_done_us: u64,
}

/// Carried from upload into the wgpu submit callback to stamp `gpu_done_us`.
#[derive(Clone, Copy, Debug)]
struct PendingGpuSample {
    frame_id: Option<u32>,
    capture_timestamp_us: Option<u64>,
    cpu_received_us: u64,
}

#[derive(Clone)]
struct SimulcastState {
    available: bool,
    publication: Option<RemoteTrackPublication>,
    requested_quality: Option<livekit::track::VideoQuality>,
    active_quality: Option<livekit::track::VideoQuality>,
    full_dims: Option<(u32, u32)>,
}

impl Default for SimulcastState {
    fn default() -> Self {
        Self {
            available: false,
            publication: None,
            requested_quality: None,
            active_quality: None,
            full_dims: None,
        }
    }
}

fn codec_label(mime: &str) -> String {
    let base = mime.split(';').next().unwrap_or(mime).trim();
    let last = base.rsplit('/').next().unwrap_or(base).trim();
    last.to_ascii_uppercase()
}

fn infer_quality_from_dims(
    full_w: u32,
    _full_h: u32,
    cur_w: u32,
    _cur_h: u32,
) -> livekit::track::VideoQuality {
    if full_w == 0 {
        return livekit::track::VideoQuality::High;
    }
    let ratio = cur_w as f32 / full_w as f32;
    if ratio >= 0.75 {
        livekit::track::VideoQuality::High
    } else if ratio >= 0.45 {
        livekit::track::VideoQuality::Medium
    } else {
        livekit::track::VideoQuality::Low
    }
}

fn find_video_inbound_stats(
    stats: &[livekit::webrtc::stats::RtcStats],
) -> Option<livekit::webrtc::stats::InboundRtpStats> {
    stats.iter().find_map(|stat| match stat {
        livekit::webrtc::stats::RtcStats::InboundRtp(inbound) if inbound.stream.kind == "video" => {
            Some(inbound.clone())
        }
        _ => None,
    })
}

fn log_video_inbound_stats(stats: &[livekit::webrtc::stats::RtcStats]) {
    let mut codec_by_id: HashMap<String, (String, String)> = HashMap::new();
    for stat in stats {
        if let livekit::webrtc::stats::RtcStats::Codec(codec) = stat {
            codec_by_id.insert(
                codec.rtc.id.clone(),
                (codec.codec.mime_type.clone(), codec.codec.sdp_fmtp_line.clone()),
            );
        }
    }

    if let Some(inbound) = find_video_inbound_stats(stats) {
        if let Some((mime, fmtp)) = codec_by_id.get(&inbound.stream.codec_id) {
            info!("Inbound codec: {} (fmtp: {})", mime, fmtp);
        } else {
            info!("Inbound codec id: {}", inbound.stream.codec_id);
        }
        info!(
            "Inbound current layer: {}x{} ~{:.1} fps, decoder: {}, power_efficient: {}",
            inbound.inbound.frame_width,
            inbound.inbound.frame_height,
            inbound.inbound.frames_per_second,
            inbound.inbound.decoder_implementation,
            inbound.inbound.power_efficient_decoder
        );
    }
}

fn update_simulcast_quality_from_stats(
    stats: &[livekit::webrtc::stats::RtcStats],
    simulcast: &Arc<Mutex<SimulcastState>>,
) {
    let Some(inbound) = find_video_inbound_stats(stats) else {
        return;
    };
    let Some((fw, fh)) = simulcast_state_full_dims(simulcast) else {
        return;
    };

    let q = infer_quality_from_dims(
        fw,
        fh,
        inbound.inbound.frame_width as u32,
        inbound.inbound.frame_height as u32,
    );
    let mut sc = simulcast.lock();
    sc.active_quality = Some(q);
}

/// Returns the current wall-clock time as microseconds since Unix epoch.
fn current_timestamp_us() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_micros() as u64
}

fn format_optional_timestamp_us(ts_us: Option<u64>) -> String {
    ts_us.map(format_timestamp_us).unwrap_or_else(|| "NA".to_string())
}

/// Format the us delta as a millisecond string like `"12.3ms"`.
fn format_us_delta_ms(later_us: u64, earlier_us: u64) -> String {
    let delta_us = later_us.saturating_sub(earlier_us);
    format!("{:.1}MS", delta_us as f64 / 1_000.0)
}

fn simulcast_state_full_dims(state: &Arc<Mutex<SimulcastState>>) -> Option<(u32, u32)> {
    let sc = state.lock();
    sc.full_dims
}

fn video_quality_label(q: livekit::track::VideoQuality) -> &'static str {
    match q {
        livekit::track::VideoQuality::Low => "LOW",
        livekit::track::VideoQuality::Medium => "MED",
        livekit::track::VideoQuality::High => "HIGH",
    }
}

fn frame_id_label(frame_id: Option<u32>) -> String {
    frame_id.map(|id| id.to_string()).unwrap_or_else(|| "NA".to_string())
}

fn overlay_stats_line(label: &str, value: impl std::fmt::Display) -> String {
    format!("{label:<17}{value}")
}

fn build_timing_overlay_lines(
    frame_id: Option<u32>,
    publish_us: Option<u64>,
    receive_us: u64,
    prev_render: Option<GpuDoneSample>,
    prev_latency_display: &str,
) -> Vec<String> {
    let mut lines = vec![
        overlay_stats_line("FRAME ID:", frame_id_label(frame_id)),
        overlay_stats_line("CAPT TIMESTAMP:", format_optional_timestamp_us(publish_us)),
        overlay_stats_line("RECV TIMESTAMP:", format_timestamp_us(receive_us)),
    ];

    if let Some(sample) = prev_render {
        lines.push(overlay_stats_line("PREV FRAME ID:", frame_id_label(sample.frame_id)));
        lines.push(overlay_stats_line(
            "PREV CAPT:",
            format_optional_timestamp_us(sample.capture_timestamp_us),
        ));
        lines.push(overlay_stats_line("PREV RECV:", format_timestamp_us(sample.cpu_received_us)));
        lines.push(overlay_stats_line("PREV RENDER:", format_timestamp_us(sample.gpu_done_us)));
        lines.push(overlay_stats_line("PREV LATENCY:", prev_latency_display));
    } else {
        lines.push(overlay_stats_line("PREV FRAME ID:", "NA"));
        lines.push(overlay_stats_line("PREV CAPT:", "NA"));
        lines.push(overlay_stats_line("PREV RECV:", "NA"));
        lines.push(overlay_stats_line("PREV RENDER:", "NA"));
        lines.push(overlay_stats_line("PREV LATENCY:", prev_latency_display));
    }

    lines
}

fn video_size(shared: &Arc<Mutex<SharedYuv>>) -> Option<(u32, u32)> {
    let s = shared.lock();
    if s.width > 0 && s.height > 0 {
        Some((s.width, s.height))
    } else {
        None
    }
}

async fn handle_track_subscribed(
    track: livekit::track::RemoteTrack,
    publication: RemoteTrackPublication,
    participant: RemoteParticipant,
    allowed_identity: &Option<String>,
    shared: &Arc<Mutex<SharedYuv>>,
    active_sid: &Arc<Mutex<Option<TrackSid>>>,
    ctrl_c_received: &Arc<AtomicBool>,
    simulcast: &Arc<Mutex<SimulcastState>>,
    repaint_ctx: &Arc<Mutex<Option<egui::Context>>>,
) {
    // If a participant filter is set, skip others
    if let Some(ref allow) = allowed_identity {
        if participant.identity().as_str() != allow {
            debug!("Skipping track from '{}' (filter set to '{}')", participant.identity(), allow);
            return;
        }
    }

    let livekit::track::RemoteTrack::Video(video_track) = track else {
        return;
    };

    let sid = publication.sid().clone();
    let codec = codec_label(&publication.mime_type());
    // Only handle if we don't already have an active video track
    {
        let mut active = active_sid.lock();
        if active.as_ref() == Some(&sid) {
            debug!("Track {} already active, ignoring duplicate subscribe", sid);
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

    // Update HUD codec label and feature flags early (before first frame arrives)
    {
        let mut s = shared.lock();
        s.codec = codec;
    }

    info!(
        "Subscribed to video track: {} (sid {}) from {} - codec: {}, simulcast: {}, dimension: {}x{}, packet_trailer_features: {:?}",
        publication.name(),
        publication.sid(),
        participant.identity(),
        publication.mime_type(),
        publication.simulcasted(),
        publication.dimension().0,
        publication.dimension().1,
        publication.packet_trailer_features(),
    );

    let rtc_track = video_track.rtc_track();

    // Start background sink task immediately so stats lookup cannot delay first-frame handling.
    let shared2 = shared.clone();
    let active_sid2 = active_sid.clone();
    let my_sid = sid.clone();
    let ctrl_c_sink = ctrl_c_received.clone();
    let repaint_ctx_sink = repaint_ctx.clone();
    // Initialize simulcast state for this publication
    {
        let mut sc = simulcast.lock();
        sc.available = publication.simulcasted();
        let dim = publication.dimension();
        sc.full_dims = Some((dim.0, dim.1));
        sc.requested_quality = None;
        sc.active_quality = None;
        sc.publication = Some(publication.clone());
    }
    tokio::spawn(async move {
        let mut sink = NativeVideoStream::new(rtc_track);
        let mut frames: u64 = 0;
        let mut last_log = Instant::now();
        let mut logged_first = false;
        let mut fps_window_frames: u64 = 0;
        let mut fps_window_start = Instant::now();
        let mut fps_smoothed: f32 = 0.0;
        loop {
            if ctrl_c_sink.load(Ordering::Acquire) {
                break;
            }
            let next = tokio::select! {
                _ = wait_for_shutdown(ctrl_c_sink.clone()) => None,
                frame = sink.next() => frame,
            };
            let Some(frame) = next else { break };
            let received_at_us = current_timestamp_us();
            let w = frame.buffer.width();
            let h = frame.buffer.height();

            if !logged_first {
                debug!("First frame: {}x{}, type {:?}", w, h, frame.buffer.buffer_type());
                logged_first = true;
            }

            let mut s = shared2.lock();

            // Update smoothed FPS (~500ms window)
            fps_window_frames += 1;
            let win_elapsed = fps_window_start.elapsed();
            if win_elapsed >= Duration::from_millis(500) {
                let inst_fps = (fps_window_frames as f32) / (win_elapsed.as_secs_f32().max(0.001));
                fps_smoothed = if fps_smoothed <= 0.0 {
                    inst_fps
                } else {
                    // light EMA smoothing to reduce jitter
                    (fps_smoothed * 0.7) + (inst_fps * 0.3)
                };
                s.fps = fps_smoothed;
                fps_window_frames = 0;
                fps_window_start = Instant::now();
            }

            s.width = w;
            s.height = h;
            s.dirty = true;
            s.received_at_us = Some(received_at_us);
            s.frame_metadata = frame.frame_metadata;
            s.frame = Some(frame);
            drop(s);

            if let Some(ctx) = repaint_ctx_sink.lock().as_ref() {
                ctx.request_repaint();
            }

            frames += 1;
            let elapsed = last_log.elapsed();
            if elapsed >= Duration::from_secs(2) {
                let fps = frames as f64 / elapsed.as_secs_f64();
                info!("Receiving video: {}x{}, ~{:.1} fps", w, h, fps);
                frames = 0;
                last_log = Instant::now();
            }
        }
        info!("Video stream ended for {}", my_sid);
        // Clear active sid if still ours
        let mut active = active_sid2.lock();
        if active.as_ref() == Some(&my_sid) {
            *active = None;
        }
    });

    let ctrl_c_stats = ctrl_c_received.clone();
    let active_sid_stats = active_sid.clone();
    let my_sid_stats = sid.clone();
    let simulcast_stats = simulcast.clone();
    tokio::spawn(async move {
        let mut logged_initial = false;
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            if ctrl_c_stats.load(Ordering::Acquire) {
                break;
            }
            if active_sid_stats.lock().as_ref() != Some(&my_sid_stats) {
                break;
            }

            match video_track.get_stats().await {
                Ok(stats) => {
                    if !logged_initial {
                        log_video_inbound_stats(&stats);
                        logged_initial = true;
                    }
                    update_simulcast_quality_from_stats(&stats, &simulcast_stats);
                }
                Err(e) if !logged_initial => {
                    debug!("Failed to get stats for video track: {:?}", e);
                    logged_initial = true;
                }
                Err(_) => {}
            }

            interval.tick().await;
        }
    });
}

fn clear_hud_and_simulcast(shared: &Arc<Mutex<SharedYuv>>, simulcast: &Arc<Mutex<SimulcastState>>) {
    {
        let mut s = shared.lock();
        s.codec.clear();
        s.fps = 0.0;
        s.frame = None;
        s.dirty = false;
        s.received_at_us = None;
        s.frame_metadata = None;
        s.gpu_done = None;
    }
    let mut sc = simulcast.lock();
    *sc = SimulcastState::default();
}

fn timing_overlay_lines(
    shared: &Arc<Mutex<SharedYuv>>,
    latency_display: &mut LatencyDisplay,
) -> Option<Vec<String>> {
    let s = shared.lock();
    let receive_us = s.received_at_us?;
    let frame_id = s.frame_metadata.and_then(|m| m.frame_id);
    let publish_us = s.frame_metadata.and_then(|m| m.user_timestamp);
    let previous_gpu_done = s.gpu_done;
    drop(s);

    let prev_latency_display = latency_display
        .value(
            Instant::now(),
            previous_gpu_done.and_then(|sample| {
                sample
                    .capture_timestamp_us
                    .map(|capture_us| format_us_delta_ms(sample.gpu_done_us, capture_us))
            }),
        )
        .to_string();

    Some(build_timing_overlay_lines(
        frame_id,
        publish_us,
        receive_us,
        previous_gpu_done,
        &prev_latency_display,
    ))
}

fn paint_timing_overlay(ctx: &egui::Context, video_rect: egui::Rect, lines: &[String]) {
    egui::Area::new("timing_overlay".into())
        .fixed_pos(video_rect.left_top() + egui::vec2(10.0, 10.0))
        .interactable(false)
        .show(ctx, |ui| {
            egui::Frame::NONE
                .fill(egui::Color32::from_black_alpha(170))
                .corner_radius(egui::CornerRadius::same(4))
                .inner_margin(egui::Margin::same(6))
                .show(ui, |ui| {
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(lines.join("\n"))
                                .monospace()
                                .size(12.0)
                                .color(egui::Color32::WHITE),
                        )
                        .extend(),
                    );
                });
        });
}

fn handle_track_unsubscribed(
    publication: RemoteTrackPublication,
    shared: &Arc<Mutex<SharedYuv>>,
    active_sid: &Arc<Mutex<Option<TrackSid>>>,
    simulcast: &Arc<Mutex<SimulcastState>>,
) {
    let sid = publication.sid().clone();
    let mut active = active_sid.lock();
    if active.as_ref() == Some(&sid) {
        info!("Video track unsubscribed ({}), clearing active sink", sid);
        *active = None;
    }
    clear_hud_and_simulcast(shared, simulcast);
}

fn handle_track_unpublished(
    publication: RemoteTrackPublication,
    shared: &Arc<Mutex<SharedYuv>>,
    active_sid: &Arc<Mutex<Option<TrackSid>>>,
    simulcast: &Arc<Mutex<SimulcastState>>,
) {
    let sid = publication.sid().clone();
    let mut active = active_sid.lock();
    if active.as_ref() == Some(&sid) {
        info!("Video track unpublished ({}), clearing active sink", sid);
        *active = None;
    }
    clear_hud_and_simulcast(shared, simulcast);
}

struct VideoApp {
    shared: Arc<Mutex<SharedYuv>>,
    simulcast: Arc<Mutex<SimulcastState>>,
    repaint_ctx: Arc<Mutex<Option<egui::Context>>>,
    ctrl_c_received: Arc<AtomicBool>,
    viewport: AspectConstrainedViewport,
    display_timestamp: bool,
    latency_display: LatencyDisplay,
}

impl eframe::App for VideoApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        *self.repaint_ctx.lock() = Some(ctx.clone());
        if self.ctrl_c_received.load(Ordering::Acquire) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }

        let mut aspect_just_changed = false;
        if let Some((width, height)) = video_size(&self.shared) {
            aspect_just_changed = self.viewport.set_video_size(ctx, width, height);
        }
        self.viewport.constrain(ctx, aspect_just_changed);

        let timing_lines = self
            .display_timestamp
            .then(|| timing_overlay_lines(&self.shared, &mut self.latency_display))
            .flatten();

        egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx, |ui| {
            // Ensure we keep repainting for smooth playback.
            ui.ctx().request_repaint();

            // Render into a centered rect that matches the source aspect ratio. This keeps resize
            // smooth (no feedback loop) and avoids stretching/distortion while dragging.
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
                    if let Some(lines) = timing_lines.as_ref() {
                        paint_timing_overlay(ui.ctx(), rect, lines);
                    }
                },
            );
        });

        // Non-timing video stats stay in egui so they don't become part of the frame timing record.
        egui::Area::new("video_hud".into())
            .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-10.0, 10.0))
            .interactable(false)
            .show(ctx, |ui| {
                let s = self.shared.lock();
                if s.width == 0 || s.height == 0 || s.fps <= 0.0 || s.codec.is_empty() {
                    return;
                }
                let mut text = format!("{} {}x{} {:.1}fps", s.codec, s.width, s.height, s.fps);
                drop(s);

                let sc = self.simulcast.lock();
                if sc.available {
                    let layer = sc.active_quality.map(video_quality_label).unwrap_or("NA");
                    text.push_str(&format!("\nSimulcast: {}", layer));
                } else {
                    text.push_str("\nSimulcast: off");
                }
                drop(sc);

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

        // Simulcast layer controls: bottom-left overlay
        egui::Area::new("simulcast_controls".into())
            .anchor(egui::Align2::LEFT_BOTTOM, egui::vec2(10.0, -10.0))
            .interactable(true)
            .show(ctx, |ui| {
                let mut sc = self.simulcast.lock();
                if !sc.available {
                    return;
                }
                let selected = sc.requested_quality.or(sc.active_quality);
                ui.horizontal(|ui| {
                    let choices = [
                        (livekit::track::VideoQuality::Low, "Low"),
                        (livekit::track::VideoQuality::Medium, "Med"),
                        (livekit::track::VideoQuality::High, "High"),
                    ];
                    for (q, label) in choices {
                        let is_selected = selected.is_some_and(|s| s == q);
                        let resp = ui.selectable_label(is_selected, label);
                        if resp.clicked() {
                            if let Some(ref pub_remote) = sc.publication {
                                pub_remote.set_video_quality(q);
                                sc.requested_quality = Some(q);
                            }
                        }
                    }
                });
            });

        ctx.request_repaint_after(Duration::from_millis(16));
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    let args = Args::parse();

    let ctrl_c_received = Arc::new(AtomicBool::new(false));
    tokio::spawn({
        let ctrl_c_received = ctrl_c_received.clone();
        async move {
            let _ = tokio::signal::ctrl_c().await;
            ctrl_c_received.store(true, Ordering::Release);
            info!("Ctrl-C received, exiting...");
        }
    });

    run(args, ctrl_c_received).await
}

async fn run(args: Args, ctrl_c_received: Arc<AtomicBool>) -> Result<()> {
    // LiveKit connection details (prefer CLI args, fallback to env vars)
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

    info!("Connecting to LiveKit room '{}' as '{}'...", args.room_name, args.identity);
    let mut room_options = RoomOptions::default();
    room_options.auto_subscribe = true;
    room_options.dynacast = true;
    room_options.adaptive_stream = true;

    // Configure E2EE if an encryption key is provided
    if let Some(ref e2ee_key) = args.e2ee_key {
        let key_provider = KeyProvider::with_shared_key(
            KeyProviderOptions::default(),
            e2ee_key.as_bytes().to_vec(),
        );
        room_options.encryption =
            Some(E2eeOptions { encryption_type: EncryptionType::Gcm, key_provider });
        info!("E2EE enabled with AES-GCM encryption");
    }

    let (room, _) = Room::connect(&url, &token, room_options).await?;
    let room = Arc::new(room);
    info!("Connected: {} - {}", room.name(), room.sid().await);

    // Enable E2EE after connection
    if args.e2ee_key.is_some() {
        room.e2ee_manager().set_enabled(true);
        info!("End-to-end encryption activated");
    }

    // Shared YUV buffer for UI/GPU
    let shared = Arc::new(Mutex::new(SharedYuv {
        width: 0,
        height: 0,
        frame: None,
        codec: String::new(),
        fps: 0.0,
        dirty: false,
        received_at_us: None,
        frame_metadata: None,
        gpu_done: None,
    }));

    // Subscribe to room events: on first video track, start sink task
    let allowed_identity = args.participant.clone();
    let shared_clone = shared.clone();
    // Track currently active video track SID to handle unpublish/unsubscribe
    let active_sid = Arc::new(Mutex::new(None::<TrackSid>));
    // Shared simulcast UI/control state
    let simulcast = Arc::new(Mutex::new(SimulcastState::default()));
    let repaint_ctx = Arc::new(Mutex::new(None::<egui::Context>));
    let simulcast_events = simulcast.clone();
    let repaint_ctx_events = repaint_ctx.clone();
    let ctrl_c_events = ctrl_c_received.clone();
    tokio::spawn(async move {
        let active_sid = active_sid.clone();
        let simulcast = simulcast_events;
        let mut events = room.subscribe();
        info!("Subscribed to room events");
        while let Some(evt) = events.recv().await {
            debug!("Room event: {:?}", evt);
            match evt {
                RoomEvent::TrackSubscribed { track, publication, participant } => {
                    handle_track_subscribed(
                        track,
                        publication,
                        participant,
                        &allowed_identity,
                        &shared_clone,
                        &active_sid,
                        &ctrl_c_events,
                        &simulcast,
                        &repaint_ctx_events,
                    )
                    .await;
                }
                RoomEvent::TrackUnsubscribed { publication, .. } => {
                    handle_track_unsubscribed(publication, &shared_clone, &active_sid, &simulcast);
                }
                RoomEvent::TrackUnpublished { publication, .. } => {
                    handle_track_unpublished(publication, &shared_clone, &active_sid, &simulcast);
                }
                _ => {}
            }
        }
    });

    // Start UI
    let app = VideoApp {
        shared,
        simulcast,
        repaint_ctx,
        ctrl_c_received: ctrl_c_received.clone(),
        viewport: AspectConstrainedViewport::new(None),
        display_timestamp: args.display_timestamp,
        latency_display: LatencyDisplay::default(),
    };
    let native_options = viewport_aspect::native_options(None);
    eframe::run_native(
        "LiveKit Video Subscriber",
        native_options,
        Box::new(|_| Ok::<Box<dyn eframe::App>, _>(Box::new(app))),
    )?;

    // If the window was closed manually, still signal shutdown to background threads.
    ctrl_c_received.store(true, Ordering::Release);

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
    y_tex_w: u32,
    uv_tex_w: u32,
    dims: (u32, u32),
    yuv_layout: u32,
    cpu_upload_logged: bool,
    #[cfg(target_os = "macos")]
    native_resources: Option<macos_native_video::NativeFrameResources>,
    #[cfg(target_os = "macos")]
    native_cache: Option<macos_native_video::CvMetalTextureCache>,
    #[cfg(target_os = "macos")]
    native_import_logged: bool,
    #[cfg(target_os = "macos")]
    native_import_failed_logged: bool,
}

impl YuvGpuState {
    fn create_textures(
        device: &wgpu::Device,
        width: u32,
        height: u32,
    ) -> (
        wgpu::Texture,
        wgpu::Texture,
        wgpu::Texture,
        wgpu::TextureView,
        wgpu::TextureView,
        wgpu::TextureView,
    ) {
        let uv_w = (width + 1) / 2;
        let uv_h = (height + 1) / 2;
        let y_size = wgpu::Extent3d { width, height, depth_or_array_layers: 1 };
        let uv_size = wgpu::Extent3d { width: uv_w, height: uv_h, depth_or_array_layers: 1 };
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

    fn recreate_bind_group(&mut self, device: &wgpu::Device) {
        self.bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("yuv_bind_group"),
            layout: &self.bind_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&self.y_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&self.u_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&self.v_view),
                },
                wgpu::BindGroupEntry { binding: 4, resource: self.params_buf.as_entire_binding() },
            ],
        });
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct ParamsUniform {
    src_w: u32,
    src_h: u32,
    y_tex_w: u32,
    uv_tex_w: u32,
    yuv_layout: u32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
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
                    yuv_layout: 0,
                    _pad0: 0,
                    _pad1: 0,
                    _pad2: 0,
                }),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });

            // Initial tiny textures
            let (y_tex, u_tex, v_tex, y_view, u_view, v_view) =
                YuvGpuState::create_textures(device, 1, 1);
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
                y_tex_w: 1,
                uv_tex_w: 1,
                dims: (0, 0),
                yuv_layout: 0,
                cpu_upload_logged: false,
                #[cfg(target_os = "macos")]
                native_resources: None,
                #[cfg(target_os = "macos")]
                native_cache: None,
                #[cfg(target_os = "macos")]
                native_import_logged: false,
                #[cfg(target_os = "macos")]
                native_import_failed_logged: false,
            };
            resources.insert(new_state);
        }
        let state = resources.get_mut::<YuvGpuState>().unwrap();

        let dims = (shared.width, shared.height);
        let frame_for_upload = if shared.dirty {
            shared.dirty = false;
            shared.frame.take().map(|frame| {
                let frame_id = shared.frame_metadata.and_then(|m| m.frame_id);
                let capture_timestamp_us = shared.frame_metadata.and_then(|m| m.user_timestamp);
                let cpu_received_us = shared.received_at_us.unwrap_or_default();
                let sample = PendingGpuSample { frame_id, capture_timestamp_us, cpu_received_us };
                (frame, sample)
            })
        } else {
            None
        };
        drop(shared);

        // Recreate CPU-upload textures/bind group on size change.
        if state.dims != dims && state.yuv_layout == 0 {
            let (y_tex, u_tex, v_tex, y_view, u_view, v_view) =
                YuvGpuState::create_textures(device, dims.0, dims.1);
            state.y_tex = y_tex;
            state.u_tex = u_tex;
            state.v_tex = v_tex;
            state.y_view = y_view;
            state.u_view = u_view;
            state.v_view = v_view;
            state.y_tex_w = dims.0;
            state.uv_tex_w = (dims.0 + 1) / 2;
            state.dims = dims;
            state.recreate_bind_group(device);
        }

        let mut gpu_sample_in_flight: Option<PendingGpuSample> = None;
        let mut frame_for_cpu_upload = frame_for_upload;

        #[cfg(target_os = "macos")]
        if let Some((frame, sample)) = frame_for_cpu_upload.take() {
            if frame.buffer.as_native().is_some() {
                if state.native_cache.is_none() {
                    match macos_native_video::CvMetalTextureCache::new(device) {
                        Ok(cache) => state.native_cache = Some(cache),
                        Err(err) if !state.native_import_failed_logged => {
                            debug!("Unable to create CVMetalTextureCache, falling back to CPU upload: {err:?}");
                            state.native_import_failed_logged = true;
                        }
                        Err(_) => {}
                    }
                }

                if let Some(cache) = state.native_cache.as_ref() {
                    match macos_native_video::import_nv12_frame(device, cache, frame) {
                        Ok(imported) => {
                            let full_size = imported.full_size;
                            let uv_size = imported.uv_size;
                            let resources = imported.resources;
                            state.y_tex = imported.y_tex;
                            state.u_tex = imported.uv_tex.clone();
                            state.v_tex = imported.uv_tex;
                            state.y_view = imported.y_view;
                            state.u_view = imported.uv_view.clone();
                            state.v_view = imported.uv_view;
                            state.y_tex_w = full_size.0;
                            state.uv_tex_w = uv_size.0;
                            state.dims = full_size;
                            state.yuv_layout = 1;
                            state.native_resources = Some(resources);
                            if !state.native_import_logged {
                                info!(
                                    "Using native CVPixelBuffer to Metal texture render path \
                                     (no CPU frame upload)"
                                );
                                state.native_import_logged = true;
                            }
                            state.recreate_bind_group(device);
                            queue.write_buffer(
                                &state.params_buf,
                                0,
                                bytemuck::bytes_of(&ParamsUniform {
                                    src_w: full_size.0,
                                    src_h: full_size.1,
                                    y_tex_w: state.y_tex_w,
                                    uv_tex_w: state.uv_tex_w,
                                    yuv_layout: state.yuv_layout,
                                    _pad0: 0,
                                    _pad1: 0,
                                    _pad2: 0,
                                }),
                            );
                            gpu_sample_in_flight = Some(sample);
                        }
                        Err(err) => {
                            if !state.native_import_failed_logged {
                                debug!("Unable to import native video frame, falling back to CPU upload: {err:?}");
                                state.native_import_failed_logged = true;
                            }
                            // The failed import consumed the native frame. Continue with the
                            // next frame rather than forcing a CPU conversion from this one.
                        }
                    }
                } else {
                    frame_for_cpu_upload = Some((frame, sample));
                }
            } else {
                frame_for_cpu_upload = Some((frame, sample));
            }
        }

        if let Some((frame, sample)) = frame_for_cpu_upload {
            #[cfg(target_os = "macos")]
            {
                state.native_resources = None;
            }
            if !state.cpu_upload_logged {
                info!("Using CPU I420 upload render path");
                state.cpu_upload_logged = true;
            }
            if state.dims != dims || state.yuv_layout != 0 {
                let (y_tex, u_tex, v_tex, y_view, u_view, v_view) =
                    YuvGpuState::create_textures(device, dims.0, dims.1);
                state.y_tex = y_tex;
                state.u_tex = u_tex;
                state.v_tex = v_tex;
                state.y_view = y_view;
                state.u_view = u_view;
                state.v_view = v_view;
                state.y_tex_w = dims.0;
                state.uv_tex_w = (dims.0 + 1) / 2;
                state.dims = dims;
                state.yuv_layout = 0;
                state.recreate_bind_group(device);
            }

            let owned_i420;
            let i420 = match frame.buffer.as_i420() {
                Some(i420) => i420,
                None => {
                    owned_i420 = frame.buffer.to_i420();
                    &owned_i420
                }
            };
            let (stride_y, stride_u, stride_v) = i420.strides();
            let (data_y, data_u, data_v) = i420.data();
            let uv_w = (dims.0 + 1) / 2;
            let uv_h = (dims.1 + 1) / 2;

            if stride_y >= dims.0 {
                queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &state.y_tex,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    data_y,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(stride_y),
                        rows_per_image: Some(dims.1),
                    },
                    wgpu::Extent3d { width: dims.0, height: dims.1, depth_or_array_layers: 1 },
                );
            }

            if stride_u >= uv_w {
                queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &state.u_tex,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    data_u,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(stride_u),
                        rows_per_image: Some(uv_h),
                    },
                    wgpu::Extent3d { width: uv_w, height: uv_h, depth_or_array_layers: 1 },
                );
            }

            if stride_v >= uv_w {
                queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &state.v_tex,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    data_v,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(stride_v),
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
                    y_tex_w: state.y_tex_w,
                    uv_tex_w: state.uv_tex_w,
                    yuv_layout: state.yuv_layout,
                    _pad0: 0,
                    _pad1: 0,
                    _pad2: 0,
                }),
            );
            gpu_sample_in_flight = Some(sample);
        }

        // Ride an empty command buffer with egui's submit so we can stamp GPU-done.
        if let Some(sample) = gpu_sample_in_flight {
            let shared_for_cb = self.shared.clone();
            let encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("yuv_gpu_done_probe"),
            });
            let cb = encoder.finish();
            cb.on_submitted_work_done(move || {
                let gpu_done_us = current_timestamp_us();
                let mut s = shared_for_cb.lock();
                s.gpu_done = Some(GpuDoneSample {
                    frame_id: sample.frame_id,
                    capture_timestamp_us: sample.capture_timestamp_us,
                    cpu_received_us: sample.cpu_received_us,
                    gpu_done_us,
                });
            });
            vec![cb]
        } else {
            Vec::new()
        }
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
