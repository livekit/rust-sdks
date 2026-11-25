use std::{sync::mpsc, time::Duration};

use anyhow::{ensure, Context, Result};
use eframe::wgpu;
use image::RgbImage;

use crate::video::Canvas;

/// Owns the compute pipeline and reusable buffers for YUYV-to-mesh conversion.
pub(super) struct MeshSampler {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::ComputePipeline,
    bind_group: wgpu::BindGroup,
    input: wgpu::Buffer,
    output: wgpu::Buffer,
    readback: wgpu::Buffer,
    canvas: Canvas,
}

impl MeshSampler {
    /// Creates a headless compute pipeline on a hardware adapter.
    pub(super) async fn new(canvas: Canvas) -> Result<Option<Self>> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let Ok(adapter) = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::LowPower,
                ..Default::default()
            })
            .await
        else {
            return Ok(None);
        };
        if adapter.get_info().device_type == wgpu::DeviceType::Cpu {
            return Ok(None);
        }
        let (device, queue) = adapter.request_device(&wgpu::DeviceDescriptor::default()).await?;
        let validation = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Vector video mesh sampling"),
            source: wgpu::ShaderSource::Wgsl(include_str!("mesh_samples.wgsl").into()),
        });
        let pixels = canvas.width * canvas.height;
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("YUYV to RGB mesh samples"),
            layout: None,
            module: &shader,
            entry_point: Some("main"),
            compilation_options: wgpu::PipelineCompilationOptions {
                constants: &[("width", canvas.width as f64), ("pixels", pixels as f64)],
                ..Default::default()
            },
            cache: None,
        });
        if let Some(error) = validation.pop().await {
            return Err(error.into());
        }
        let buffer = |label, size, usage| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size,
                usage,
                mapped_at_creation: false,
            })
        };
        let size = (pixels.div_ceil(4) * 12) as u64;
        let input = buffer(
            "Packed YUYV",
            (pixels * 8) as u64,
            wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        );
        let output = buffer(
            "RGB mesh samples",
            size,
            wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        );
        let readback = buffer(
            "Mesh sample readback",
            size,
            wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        );
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Mesh sampling buffers"),
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: input.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: output.as_entire_binding() },
            ],
        });
        Ok(Some(Self { device, queue, pipeline, bind_group, input, output, readback, canvas }))
    }

    /// Averages each 2×2 block and reads back only the mesh's packed RGB samples.
    pub(super) fn sample(&self, yuyv: &[u8]) -> Result<RgbImage> {
        let pixels = self.canvas.width * self.canvas.height;
        ensure!(yuyv.len() == pixels * 8, "Unexpected packed YUYV frame length");
        self.queue.write_buffer(&self.input, 0, yuyv);
        let mut enc =
            self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        {
            let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor::default());
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.dispatch_workgroups(pixels.div_ceil(4 * 64) as u32, 1, 1);
        }
        enc.copy_buffer_to_buffer(&self.output, 0, &self.readback, 0, self.readback.size());
        let submitted = self.queue.submit([enc.finish()]);
        let view = self.readback.slice(..);
        let (tx, rx) = mpsc::sync_channel(1);
        view.map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.try_send(result);
        });
        self.device.poll(wgpu::PollType::Wait {
            submission_index: Some(submitted),
            timeout: Some(Duration::from_millis(250)),
        })?;
        rx.try_recv().context("GPU did not return mesh samples")??;
        let rgb = view.get_mapped_range()[..pixels * 3].to_vec();
        self.readback.unmap();
        RgbImage::from_raw(self.canvas.width as u32, self.canvas.height as u32, rgb)
            .context("Invalid GPU mesh sample dimensions")
    }
}
