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

//! Test pattern video source.
//!
//! [`PatternVideoSource`] renders a built-in test pattern ([`Pattern`])
//! on the GPU and yields the result as pixel video. Rendering is
//! offscreen through [wgpu], so the source needs no window or display.
//!
//! The source reads each frame back from the GPU and converts it to I420
//! on the CPU.
//!
//! [wgpu]: https://wgpu.rs

use crate::{
    error::SourceError, pixel::PixelVideoSource, primitive::VideoResolution, pump::PumpStop,
};
use livekit::webrtc::video_frame::{BoxVideoFrame, I420Buffer, VideoFrame, VideoRotation};
use std::{
    fmt,
    sync::{mpsc, Arc, Mutex},
    thread,
    time::{Duration, Instant},
};
use thiserror::Error;

/// Render target format. Its memory layout (B, G, R, A) is the layout
/// libyuv names ARGB.
const TARGET_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Bgra8Unorm;

/// Bytes per pixel of [`TARGET_FORMAT`].
const TARGET_BYTES_PER_PIXEL: u32 = 4;

/// Size of the uniform block: `vec2<f32>` + `f32` + `u32`.
const UNIFORM_BUFFER_SIZE: u64 = 16;

/// Upper bound on one blocking GPU wait, so the stop token is observed
/// promptly.
const STOP_POLL_INTERVAL: Duration = Duration::from_millis(100);

/// Total time to wait for one frame readback before the source fails.
const READBACK_TIMEOUT: Duration = Duration::from_secs(5);

/// Prelude prepended to every fragment snippet. It draws one triangle
/// that covers the full target and calls `shade` per pixel.
const FRAGMENT_PRELUDE: &str = include_str!("../../shaders/prelude.wgsl");

/// Fragment snippet for [`Pattern::Gradient`].
const GRADIENT_SHADER: &str = include_str!("../../shaders/gradient.wgsl");

/// Test pattern rendered by a [`PatternVideoSource`].
///
/// Every pattern is a pure function of position, resolution, and time:
/// the same configuration produces the same frames on every machine.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "snake_case")
)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[non_exhaustive]
pub enum Pattern {
    /// Animated color gradient.
    Gradient,
}

impl Pattern {
    /// Returns the complete WGSL module to compile.
    fn module_code(&self) -> String {
        match self {
            Self::Gradient => assemble_module(GRADIENT_SHADER),
        }
    }
}

/// Prepends the prelude to a pattern's fragment snippet.
fn assemble_module(snippet: &str) -> String {
    format!("{FRAGMENT_PRELUDE}\n{snippet}")
}

/// Configuration for a [`PatternVideoSource`].
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(deny_unknown_fields)
)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct PatternVideoSourceConfig {
    /// Output resolution.
    pub resolution: VideoResolution,
    /// Output frame rate in frames per second.
    pub framerate_fps: u32,
    /// Pattern to render.
    pub pattern: Pattern,
}

/// Error returned by a pattern video source.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum PatternVideoSourceError {
    /// The configured resolution has a zero component.
    #[error("pattern source resolution must be non-zero")]
    ZeroResolution,
    /// The configured frame rate is zero.
    #[error("pattern source frame rate must be non-zero")]
    ZeroFramerate,
    /// No compatible GPU adapter is available.
    #[error("no compatible GPU adapter: {0}")]
    NoAdapter(String),
    /// The GPU adapter rejected the device request.
    #[error("failed to open the GPU device: {0}")]
    Device(String),
    /// The shader or its pipeline failed to build.
    #[error("failed to build the shader pipeline: {0}")]
    ShaderCompile(String),
    /// The GPU reported an error.
    #[error("GPU error: {0}")]
    Backend(String),
    /// Reading the rendered frame back from the GPU failed.
    #[error("failed to read the frame back from the GPU: {0}")]
    Readback(String),
    /// Pixel conversion failed.
    #[error("failed to convert the rendered frame to I420: {0}")]
    Convert(&'static str),
}

/// Pixel video source that renders a test pattern on the GPU.
///
/// The source sleeps to pace itself to the configured frame rate. It
/// never reaches the end of its stream — stop the pump that drives it
/// instead.
pub struct PatternVideoSource {
    config: PatternVideoSourceConfig,
    renderer: PatternRenderer,
    started: Option<Instant>,
    frame_index: u64,
}

impl PatternVideoSource {
    /// Creates the source. GPU setup runs on the tokio blocking pool.
    ///
    /// Requires a running tokio runtime. Use
    /// [`PatternVideoSource::new_blocking`] outside of async contexts.
    #[cfg(feature = "tokio")]
    pub async fn new(config: PatternVideoSourceConfig) -> Result<Self, SourceError> {
        crate::utils::run_blocking(move || Self::new_blocking(config)).await
    }

    /// Selects a GPU adapter, compiles the pattern's shader, and builds
    /// the render pipeline.
    ///
    /// Construction fails when no GPU is available, or for a zero
    /// resolution or frame rate.
    pub fn new_blocking(config: PatternVideoSourceConfig) -> Result<Self, SourceError> {
        validate_config(&config).map_err(SourceError::new)?;
        let renderer = PatternRenderer::new(&config).map_err(SourceError::new)?;
        Ok(Self { config, renderer, started: None, frame_index: 0 })
    }

    /// Returns the configuration the source was created with.
    pub fn config(&self) -> &PatternVideoSourceConfig {
        &self.config
    }

    fn frame_interval(&self) -> Duration {
        Duration::from_secs(1) / self.config.framerate_fps
    }
}

impl PixelVideoSource for PatternVideoSource {
    fn resolution(&self) -> VideoResolution {
        self.config.resolution
    }

    // The pacing sleep is at most one frame interval, and every readback
    // wait is bounded by STOP_POLL_INTERVAL, so the stop token is
    // observed promptly.
    fn next_frame(&mut self, stop: &PumpStop) -> Result<Option<BoxVideoFrame>, SourceError> {
        let started = *self.started.get_or_insert_with(Instant::now);

        // Pace against the ideal timeline so timestamps stay jitter-free.
        let interval_us = self.frame_interval().as_micros() as u64;
        let elapsed = Duration::from_micros(self.frame_index.saturating_mul(interval_us));
        let due = started + elapsed;
        if let Some(wait) = due.checked_duration_since(Instant::now()) {
            thread::sleep(wait);
        }

        // The uniform frame index wraps after u32::MAX frames.
        let frame_index = self.frame_index as u32;
        self.frame_index += 1;
        let buffer = self
            .renderer
            .render_frame(elapsed.as_secs_f32(), frame_index, stop)
            .map_err(SourceError::new)?;
        let Some(buffer) = buffer else {
            // The stop token fired during the readback wait.
            return Ok(None);
        };

        Ok(Some(VideoFrame {
            rotation: VideoRotation::VideoRotation0,
            timestamp_us: elapsed.as_micros() as i64,
            frame_metadata: None,
            buffer: Box::new(buffer),
        }))
    }
}

impl fmt::Debug for PatternVideoSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PatternVideoSource")
            .field("config", &self.config)
            .field("frame_index", &self.frame_index)
            .finish_non_exhaustive()
    }
}

/// Validates the CPU-checkable parts of a configuration.
fn validate_config(config: &PatternVideoSourceConfig) -> Result<(), PatternVideoSourceError> {
    let VideoResolution { width, height } = config.resolution;
    if width == 0 || height == 0 {
        return Err(PatternVideoSourceError::ZeroResolution);
    }
    if config.framerate_fps == 0 {
        return Err(PatternVideoSourceError::ZeroFramerate);
    }
    Ok(())
}

/// Owns the wgpu state and renders one frame at a time.
struct PatternRenderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    uniform_buffer: wgpu::Buffer,
    target: wgpu::Texture,
    target_view: wgpu::TextureView,
    /// Readback destination, reused across frames. Rows are padded to
    /// the wgpu copy alignment.
    staging: wgpu::Buffer,
    padded_bytes_per_row: u32,
    resolution: VideoResolution,
    /// First uncaptured GPU error, stashed by the device error handler
    /// and surfaced on the next frame.
    device_error: Arc<Mutex<Option<String>>>,
}

impl PatternRenderer {
    fn new(config: &PatternVideoSourceConfig) -> Result<Self, PatternVideoSourceError> {
        let VideoResolution { width, height } = config.resolution;
        let module_code = config.pattern.module_code();
        let padded_bytes_per_row = padded_bytes_per_row(width).ok_or_else(|| {
            PatternVideoSourceError::Backend("resolution is too large".to_owned())
        })?;

        // Rendering is offscreen, so no display handle is needed. WGPU_*
        // environment variables can override the backend and adapter
        // selection.
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::from_env().unwrap_or_default(),
            ..Default::default()
        }))
        .map_err(|err| PatternVideoSourceError::NoAdapter(err.to_string()))?;

        let info = adapter.get_info();
        log::info!("Rendering pattern source on \"{}\" ({})", info.name, info.backend);

        // Clamp the default limits to what the adapter supports, so weaker
        // adapters (GL, software rasterizers) still open. A resolution
        // beyond the clamped limits fails texture creation below.
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("lk_pattern_device"),
            required_limits: wgpu::Limits::default().or_worse_values_from(&adapter.limits()),
            ..Default::default()
        }))
        .map_err(|err| PatternVideoSourceError::Device(err.to_string()))?;

        // Runtime GPU errors have no return channel of their own: stash
        // the first one and report it from the next render_frame call.
        let device_error: Arc<Mutex<Option<String>>> = Arc::default();
        let sink = Arc::clone(&device_error);
        device.on_uncaptured_error(Arc::new(move |error: wgpu::Error| {
            log::error!("pattern source GPU error: {error}");
            let mut slot = sink.lock().unwrap();
            if slot.is_none() {
                *slot = Some(error.to_string());
            }
        }));

        // Compile the shader and build the pipeline under an error scope,
        // so a bad shader fails construction with its compile message.
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("lk_pattern_module"),
            source: wgpu::ShaderSource::Wgsl(module_code.into()),
        });
        let bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("lk_pattern_bind_group_layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(UNIFORM_BUFFER_SIZE),
                    },
                    count: None,
                }],
            });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("lk_pattern_pipeline_layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("lk_pattern_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: TARGET_FORMAT,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        if let Some(error) = pollster::block_on(scope.pop()) {
            return Err(PatternVideoSourceError::ShaderCompile(error.to_string()));
        }

        // Build the target and readback resources under their own scope,
        // so an unsupported resolution also fails construction.
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("lk_pattern_target"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: TARGET_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let target_view = target.create_view(&wgpu::TextureViewDescriptor::default());
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("lk_pattern_uniforms"),
            size: UNIFORM_BUFFER_SIZE,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("lk_pattern_bind_group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });
        let staging = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("lk_pattern_staging"),
            size: u64::from(padded_bytes_per_row) * u64::from(height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        if let Some(error) = pollster::block_on(scope.pop()) {
            return Err(PatternVideoSourceError::Backend(error.to_string()));
        }

        Ok(Self {
            device,
            queue,
            pipeline,
            bind_group,
            uniform_buffer,
            target,
            target_view,
            staging,
            padded_bytes_per_row,
            resolution: config.resolution,
            device_error,
        })
    }

    /// Renders one frame and reads it back as I420. Returns `Ok(None)`
    /// when the stop token fires during the readback wait.
    fn render_frame(
        &self,
        time_s: f32,
        frame_index: u32,
        stop: &PumpStop,
    ) -> Result<Option<I420Buffer>, PatternVideoSourceError> {
        self.check_device_error()?;

        let VideoResolution { width, height } = self.resolution;
        self.queue.write_buffer(
            &self.uniform_buffer,
            0,
            &uniform_bytes(self.resolution, time_s, frame_index),
        );

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("lk_pattern") });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("lk_pattern_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.target_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.draw(0..3, 0..1);
        }
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &self.target,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &self.staging,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(self.padded_bytes_per_row),
                    rows_per_image: None,
                },
            },
            wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        );
        // Schedule the mapping with the submission, so no separate
        // map_async call is needed after submit.
        let (mapped_tx, mapped_rx) = mpsc::channel();
        encoder.map_buffer_on_submit(&self.staging, wgpu::MapMode::Read, .., move |result| {
            let _ = mapped_tx.send(result);
        });
        let submission = self.queue.submit([encoder.finish()]);

        if !self.wait_for_map(submission, &mapped_rx, stop)? {
            // Stopped: cancel the pending mapping to leave the buffer
            // reusable.
            self.staging.unmap();
            return Ok(None);
        }

        let mapped = self.staging.slice(..).get_mapped_range();
        let converted = convert_to_i420(&mapped, self.padded_bytes_per_row, width, height);
        drop(mapped);
        self.staging.unmap();
        converted.map(Some)
    }

    /// Waits for the staging buffer to be mapped. Returns `Ok(false)` when
    /// the stop token fires first.
    fn wait_for_map(
        &self,
        submission: wgpu::SubmissionIndex,
        mapped: &mpsc::Receiver<Result<(), wgpu::BufferAsyncError>>,
        stop: &PumpStop,
    ) -> Result<bool, PatternVideoSourceError> {
        let deadline = Instant::now() + READBACK_TIMEOUT;
        loop {
            let poll = self.device.poll(wgpu::PollType::Wait {
                submission_index: Some(submission.clone()),
                timeout: Some(STOP_POLL_INTERVAL),
            });
            match poll {
                Ok(_) | Err(wgpu::PollError::Timeout) => {}
                Err(err) => return Err(PatternVideoSourceError::Readback(err.to_string())),
            }
            match mapped.try_recv() {
                Ok(Ok(())) => return Ok(true),
                Ok(Err(err)) => return Err(PatternVideoSourceError::Readback(err.to_string())),
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => {
                    return Err(PatternVideoSourceError::Readback(
                        "map callback was dropped".to_owned(),
                    ));
                }
            }
            self.check_device_error()?;
            if stop.is_stopped() {
                return Ok(false);
            }
            if Instant::now() >= deadline {
                return Err(PatternVideoSourceError::Readback(
                    "timed out waiting for the GPU".to_owned(),
                ));
            }
        }
    }

    /// Reports the first stashed GPU error, if there is one.
    fn check_device_error(&self) -> Result<(), PatternVideoSourceError> {
        match &*self.device_error.lock().unwrap() {
            Some(message) => Err(PatternVideoSourceError::Backend(message.clone())),
            None => Ok(()),
        }
    }
}

/// Serializes the uniform block: resolution, time, and frame index.
fn uniform_bytes(
    resolution: VideoResolution,
    time_s: f32,
    frame_index: u32,
) -> [u8; UNIFORM_BUFFER_SIZE as usize] {
    let mut bytes = [0u8; UNIFORM_BUFFER_SIZE as usize];
    bytes[0..4].copy_from_slice(&(resolution.width as f32).to_ne_bytes());
    bytes[4..8].copy_from_slice(&(resolution.height as f32).to_ne_bytes());
    bytes[8..12].copy_from_slice(&time_s.to_ne_bytes());
    bytes[12..16].copy_from_slice(&frame_index.to_ne_bytes());
    bytes
}

/// Returns the staging-buffer row stride: the pixel row size rounded up
/// to the wgpu copy alignment. `None` when the value overflows `u32`.
fn padded_bytes_per_row(width: u32) -> Option<u32> {
    let unpadded = u64::from(width) * u64::from(TARGET_BYTES_PER_PIXEL);
    let align = u64::from(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
    u32::try_from(unpadded.div_ceil(align) * align).ok()
}

/// Returns whether a GPU adapter is available. Tests that need a GPU
/// skip when there is none.
#[cfg(test)]
pub(crate) fn gpu_available() -> bool {
    let instance =
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())).is_ok()
}

/// Converts one padded BGRA image to a freshly allocated I420 buffer.
fn convert_to_i420(
    source: &[u8],
    source_stride: u32,
    width: u32,
    height: u32,
) -> Result<I420Buffer, PatternVideoSourceError> {
    if source.len() < source_stride as usize * height as usize {
        return Err(PatternVideoSourceError::Convert("mapped frame is too short"));
    }
    let source_stride = i32::try_from(source_stride)
        .map_err(|_| PatternVideoSourceError::Convert("stride exceeds supported range"))?;
    let width_i32 = i32::try_from(width)
        .map_err(|_| PatternVideoSourceError::Convert("width exceeds supported range"))?;
    let height_i32 = i32::try_from(height)
        .map_err(|_| PatternVideoSourceError::Convert("height exceeds supported range"))?;

    let mut buffer = I420Buffer::new(width, height);
    let (stride_y, stride_u, stride_v) = buffer.strides();
    let (dst_y, dst_u, dst_v) = buffer.data_mut();
    // SAFETY: The source slice covers `height` rows of `source_stride` bytes, and the
    // destination planes come from a freshly allocated I420Buffer with matching width,
    // height, and strides.
    let ret = unsafe {
        yuv_sys::rs_ARGBToI420(
            source.as_ptr(),
            source_stride,
            dst_y.as_mut_ptr(),
            stride_y as i32,
            dst_u.as_mut_ptr(),
            stride_u as i32,
            dst_v.as_mut_ptr(),
            stride_v as i32,
            width_i32,
            height_i32,
        )
    };
    if ret != 0 {
        return Err(PatternVideoSourceError::Convert("ARGBToI420 failed"));
    }
    Ok(buffer)
}

#[cfg(test)]
mod tests {
    use super::*;

    const RESOLUTION: VideoResolution = VideoResolution { width: 64, height: 36 };

    fn gradient_config() -> PatternVideoSourceConfig {
        PatternVideoSourceConfig {
            resolution: RESOLUTION,
            framerate_fps: 1000,
            pattern: Pattern::Gradient,
        }
    }

    #[test]
    fn validation_rejects_zero_resolution_and_framerate() {
        let mut config = gradient_config();
        config.resolution = VideoResolution::new(0, 36);
        assert!(matches!(
            validate_config(&config),
            Err(PatternVideoSourceError::ZeroResolution)
        ));

        let mut config = gradient_config();
        config.framerate_fps = 0;
        assert!(matches!(validate_config(&config), Err(PatternVideoSourceError::ZeroFramerate)));
    }

    #[test]
    fn gradient_module_includes_the_prelude() {
        let code = Pattern::Gradient.module_code();
        assert!(code.contains("fn vs_main"));
        assert!(code.contains("fn fs_main"));
        assert!(code.contains("fn shade"));
    }

    #[test]
    fn rows_are_padded_to_the_copy_alignment() {
        assert_eq!(padded_bytes_per_row(64), Some(256));
        assert_eq!(padded_bytes_per_row(321), Some(1536));
        assert_eq!(padded_bytes_per_row(1280), Some(5120));
        assert_eq!(padded_bytes_per_row(u32::MAX), None);
    }

    #[test]
    fn gradient_renders_frames_at_the_frame_rate() {
        if !gpu_available() {
            eprintln!("skipping: no GPU adapter available");
            return;
        }
        let mut source = PatternVideoSource::new_blocking(gradient_config()).unwrap();

        let stop = PumpStop::new();
        let first = source.next_frame(&stop).unwrap().unwrap();
        let second = source.next_frame(&stop).unwrap().unwrap();
        assert_eq!((first.buffer.width(), first.buffer.height()), (64, 36));
        assert_eq!(first.timestamp_us, 0);
        assert_eq!(second.timestamp_us, 1_000);

        // The gradient's top-left pixel at time zero is red-dominant:
        // RGB (255, 68, 47), which is about (121, 91, 211) in
        // limited-range BT.601. A red/blue channel swap in the readback
        // path flips the two chroma values, so this check catches it.
        let i420 = first.buffer.as_i420().expect("pattern source yields I420 buffers");
        let (y, u, v) = i420.data();
        assert!(y[0].abs_diff(121) <= 5, "unexpected luma {}", y[0]);
        assert!(u[0].abs_diff(91) <= 6, "unexpected chroma-u {}", u[0]);
        assert!(v[0].abs_diff(211) <= 6, "unexpected chroma-v {}", v[0]);
    }
}
