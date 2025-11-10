use futures::StreamExt;
use livekit::webrtc::native::yuv_helper;
use livekit::webrtc::prelude::*;
use livekit::webrtc::video_stream::native::NativeVideoStream;
use parking_lot::Mutex;
use std::{ops::DerefMut, sync::Arc};

pub struct VideoRenderer {
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
    pub fn new(
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

        // TODO(theomonnom) Gracefully close the thread
        let mut video_sink = NativeVideoStream::new(rtc_track.clone());

        std::thread::spawn({
            let async_handle = async_handle.clone();
            let internal = internal.clone();
            move || {
                while let Some(frame) = async_handle.block_on(video_sink.next()) {
                    // Process the frame
                    let mut internal = internal.lock();
                    let buffer = frame.buffer.to_i420();

                    let width: u32 = buffer.width();
                    let height: u32 = buffer.height();

                    internal.ensure_texture_size(width, height);

                    let rgba_ptr = internal.rgba_data.deref_mut();
                    let rgba_stride = buffer.width() * 4;

                    let (stride_y, stride_u, stride_v) = buffer.strides();
                    let (data_y, data_u, data_v) = buffer.data();

                    yuv_helper::i420_to_abgr(
                        data_y,
                        stride_y,
                        data_u,
                        stride_u,
                        data_v,
                        stride_v,
                        rgba_ptr,
                        rgba_stride,
                        buffer.width() as i32,
                        buffer.height() as i32,
                    );

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
                }
            }
        });

        Self { rtc_track, internal }
    }

    // Returns the last frame resolution
    pub fn resolution(&self) -> (u32, u32) {
        let internal = self.internal.lock();
        (internal.width, internal.height)
    }

    // Returns the texture id, can be used to draw the texture on the UI
    pub fn texture_id(&self) -> Option<egui::TextureId> {
        self.internal.lock().egui_texture
    }
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
                label: Some("lk-videotexture"),
                usage: eframe::wgpu::TextureUsages::TEXTURE_BINDING
                    | eframe::wgpu::TextureUsages::COPY_DST,
                dimension: eframe::wgpu::TextureDimension::D2,
                size: eframe::wgpu::Extent3d { width, height, ..Default::default() },
                sample_count: 1,
                mip_level_count: 1,
                format: eframe::wgpu::TextureFormat::Rgba8UnormSrgb,
                view_formats: &[eframe::wgpu::TextureFormat::Rgba8UnormSrgb],
            }));

        self.texture_view = Some(self.texture.as_mut().unwrap().create_view(
            &eframe::wgpu::TextureViewDescriptor {
                label: Some("lk-videotexture-view"),
                format: Some(eframe::wgpu::TextureFormat::Rgba8UnormSrgb),
                dimension: Some(eframe::wgpu::TextureViewDimension::D2),
                mip_level_count: Some(1),
                array_layer_count: Some(1),
                ..Default::default()
            },
        ));

        if let Some(texture_id) = self.egui_texture {
            // Update the existing texture
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
