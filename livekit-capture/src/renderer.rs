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

//! Crate-internal GPU renderer shared by the shader-backed sources.
//!
//! [`ShaderRenderer`] renders a WGSL module offscreen through wgpu, one
//! frame at a time, and reads each frame back as I420. The caller
//! supplies the module and the per-frame uniform bytes. [`FramePacer`]
//! paces the frames against an ideal timeline.

use crate::{primitive::VideoResolution, pump::PumpStop};
use livekit::webrtc::video_frame::I420Buffer;
use std::{
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

/// Upper bound on one blocking GPU wait, so the stop token is observed
/// promptly.
const STOP_POLL_INTERVAL: Duration = Duration::from_millis(100);

/// Total time to wait for one frame readback before the renderer fails.
const READBACK_TIMEOUT: Duration = Duration::from_secs(5);

/// Error returned by the GPU renderer.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum RendererError {
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

/// Paces frames against an ideal timeline, so frame timestamps are
/// jitter-free.
#[derive(Debug)]
pub(crate) struct FramePacer {
    interval_us: u64,
    started: Option<Instant>,
    frame_index: u64,
}

impl FramePacer {
    /// Creates a pacer. The frame rate must be non-zero.
    pub(crate) fn new(framerate_fps: u32) -> Self {
        let interval_us = (Duration::from_secs(1) / framerate_fps).as_micros() as u64;
        Self { interval_us, started: None, frame_index: 0 }
    }

    /// Sleeps until the next frame is due. Returns the elapsed time on
    /// the ideal timeline and the index of the frame.
    ///
    /// The sleep is at most one frame interval.
    pub(crate) fn wait_for_next_frame(&mut self) -> (Duration, u64) {
        let started = *self.started.get_or_insert_with(Instant::now);
        let elapsed = Duration::from_micros(self.frame_index.saturating_mul(self.interval_us));
        let due = started + elapsed;
        if let Some(wait) = due.checked_duration_since(Instant::now()) {
            thread::sleep(wait);
        }
        let frame_index = self.frame_index;
        self.frame_index += 1;
        (elapsed, frame_index)
    }
}

/// Renders a WGSL module offscreen and reads frames back as I420.
///
/// The module must define a vertex entry point `vs_main` and a fragment
/// entry point `fs_main`. The renderer draws one triangle, which must
/// cover the full target. The module can declare one uniform buffer at
/// group 0, binding 0. The caller supplies its bytes for each frame.
pub(crate) struct ShaderRenderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    uniform_buffer: wgpu::Buffer,
    uniform_size: u64,
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

impl ShaderRenderer {
    /// Opens a GPU device, compiles the module, and builds the pipeline
    /// and readback resources.
    pub(crate) fn new(
        resolution: VideoResolution,
        module_code: &str,
        uniform_size: u64,
    ) -> Result<Self, RendererError> {
        let VideoResolution { width, height } = resolution;
        let padded_bytes_per_row = padded_bytes_per_row(width)
            .ok_or_else(|| RendererError::Backend("resolution is too large".to_owned()))?;

        // Rendering is offscreen, so no display handle is needed. WGPU_*
        // environment variables can override the backend and adapter
        // selection.
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::from_env().unwrap_or_default(),
            ..Default::default()
        }))
        .map_err(|err| RendererError::NoAdapter(err.to_string()))?;

        let info = adapter.get_info();
        log::info!("Rendering with GPU \"{}\" ({})", info.name, info.backend);

        // Clamp the default limits to what the adapter supports, so weaker
        // adapters (GL, software rasterizers) still open. A resolution
        // beyond the clamped limits fails texture creation below.
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("lk_render_device"),
            required_limits: wgpu::Limits::default().or_worse_values_from(&adapter.limits()),
            ..Default::default()
        }))
        .map_err(|err| RendererError::Device(err.to_string()))?;

        // Runtime GPU errors have no return channel of their own: stash
        // the first one and report it from the next render_frame call.
        let device_error: Arc<Mutex<Option<String>>> = Arc::default();
        let sink = Arc::clone(&device_error);
        device.on_uncaptured_error(Arc::new(move |error: wgpu::Error| {
            log::error!("render GPU error: {error}");
            let mut slot = sink.lock().unwrap();
            if slot.is_none() {
                *slot = Some(error.to_string());
            }
        }));

        // Compile the shader and build the pipeline under an error scope,
        // so a bad shader fails construction with its compile message.
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("lk_render_module"),
            source: wgpu::ShaderSource::Wgsl(module_code.into()),
        });
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("lk_render_bind_group_layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(uniform_size),
                },
                count: None,
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("lk_render_pipeline_layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("lk_render_pipeline"),
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
            return Err(RendererError::ShaderCompile(error.to_string()));
        }

        // Build the target and readback resources under their own scope,
        // so an unsupported resolution also fails construction.
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("lk_render_target"),
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
            label: Some("lk_render_uniforms"),
            size: uniform_size,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("lk_render_bind_group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });
        let staging = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("lk_render_staging"),
            size: u64::from(padded_bytes_per_row) * u64::from(height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        if let Some(error) = pollster::block_on(scope.pop()) {
            return Err(RendererError::Backend(error.to_string()));
        }

        Ok(Self {
            device,
            queue,
            pipeline,
            bind_group,
            uniform_buffer,
            uniform_size,
            target,
            target_view,
            staging,
            padded_bytes_per_row,
            resolution,
            device_error,
        })
    }

    /// Renders one frame with the given uniform bytes and reads it back
    /// as I420. Returns `Ok(None)` when the stop token fires during the
    /// readback wait.
    ///
    /// Every blocking wait is bounded by [`STOP_POLL_INTERVAL`], so the
    /// stop token is observed promptly.
    pub(crate) fn render_frame(
        &self,
        uniform: &[u8],
        stop: &PumpStop,
    ) -> Result<Option<I420Buffer>, RendererError> {
        debug_assert_eq!(uniform.len() as u64, self.uniform_size);
        self.check_device_error()?;

        let VideoResolution { width, height } = self.resolution;
        self.queue.write_buffer(&self.uniform_buffer, 0, uniform);

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("lk_render") });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("lk_render_pass"),
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
    ) -> Result<bool, RendererError> {
        let deadline = Instant::now() + READBACK_TIMEOUT;
        loop {
            let poll = self.device.poll(wgpu::PollType::Wait {
                submission_index: Some(submission.clone()),
                timeout: Some(STOP_POLL_INTERVAL),
            });
            match poll {
                Ok(_) | Err(wgpu::PollError::Timeout) => {}
                Err(err) => return Err(RendererError::Readback(err.to_string())),
            }
            match mapped.try_recv() {
                Ok(Ok(())) => return Ok(true),
                Ok(Err(err)) => return Err(RendererError::Readback(err.to_string())),
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => {
                    return Err(RendererError::Readback("map callback was dropped".to_owned()));
                }
            }
            self.check_device_error()?;
            if stop.is_stopped() {
                return Ok(false);
            }
            if Instant::now() >= deadline {
                return Err(RendererError::Readback("timed out waiting for the GPU".to_owned()));
            }
        }
    }

    /// Reports the first stashed GPU error, if there is one.
    fn check_device_error(&self) -> Result<(), RendererError> {
        match &*self.device_error.lock().unwrap() {
            Some(message) => Err(RendererError::Backend(message.clone())),
            None => Ok(()),
        }
    }
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
) -> Result<I420Buffer, RendererError> {
    if source.len() < source_stride as usize * height as usize {
        return Err(RendererError::Convert("mapped frame is too short"));
    }
    let source_stride = i32::try_from(source_stride)
        .map_err(|_| RendererError::Convert("stride exceeds supported range"))?;
    let width_i32 = i32::try_from(width)
        .map_err(|_| RendererError::Convert("width exceeds supported range"))?;
    let height_i32 = i32::try_from(height)
        .map_err(|_| RendererError::Convert("height exceeds supported range"))?;

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
        return Err(RendererError::Convert("ARGBToI420 failed"));
    }
    Ok(buffer)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_are_padded_to_the_copy_alignment() {
        assert_eq!(padded_bytes_per_row(64), Some(256));
        assert_eq!(padded_bytes_per_row(321), Some(1536));
        assert_eq!(padded_bytes_per_row(1280), Some(5120));
        assert_eq!(padded_bytes_per_row(u32::MAX), None);
    }

    #[test]
    fn pacer_reports_the_ideal_timeline() {
        let mut pacer = FramePacer::new(1000);
        let (first_elapsed, first_index) = pacer.wait_for_next_frame();
        let (second_elapsed, second_index) = pacer.wait_for_next_frame();
        assert_eq!((first_elapsed.as_micros(), first_index), (0, 0));
        assert_eq!((second_elapsed.as_micros(), second_index), (1_000, 1));
    }
}
