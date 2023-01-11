use livekit::prelude::*;
use livekit::webrtc::video_frame_buffer::*;
use livekit::webrtc::yuv_helper;
use std::convert::TryInto;
use std::num::NonZeroU32;
use std::{
    ops::DerefMut,
    sync::{Arc, Mutex},
};
use tracing::debug_span;
use tracing::{error, warn};

pub struct VideoRenderer {
    internal: Arc<Mutex<RendererInternal>>,
    rtc_track: Arc<VideoTrack>,
}

struct RendererInternal {
    render_state: egui_wgpu::RenderState,
    width: u32,
    height: u32,
    rgba_data: Vec<u8>,
    texture: Option<wgpu::Texture>,
    texture_view: Option<wgpu::TextureView>,
    egui_texture: Option<egui::TextureId>,
}

impl RendererInternal {
    fn ensure_texture_size(&mut self, width: u32, height: u32) {
        if self.width == width && self.height == height {
            return;
        }

        self.width = width;
        self.height = height;
        self.rgba_data.resize((width * height * 4) as usize, 0);

        self.texture = Some(
            self.render_state
                .device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some("lk-videotexture"),
                    usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                    dimension: wgpu::TextureDimension::D2,
                    size: wgpu::Extent3d {
                        width,
                        height,
                        ..Default::default()
                    },
                    sample_count: 1,
                    mip_level_count: 1,
                    format: wgpu::TextureFormat::Rgba8UnormSrgb,
                }),
        );

        self.texture_view = Some(self.texture.as_mut().unwrap().create_view(
            &wgpu::TextureViewDescriptor {
                label: Some("lk-videotexture-view"),
                format: Some(wgpu::TextureFormat::Rgba8UnormSrgb),
                dimension: Some(wgpu::TextureViewDimension::D2),
                mip_level_count: NonZeroU32::new(1),
                array_layer_count: NonZeroU32::new(1),
                ..Default::default()
            },
        ));

        if let Some(texture_id) = self.egui_texture {
            // Update the existing texture
            self.render_state
                .renderer
                .write()
                .update_egui_texture_from_wgpu_texture(
                    &*self.render_state.device,
                    self.texture_view.as_ref().unwrap(),
                    wgpu::FilterMode::Linear,
                    texture_id,
                );
        } else {
            self.egui_texture = Some(self.render_state.renderer.write().register_native_texture(
                &*self.render_state.device,
                self.texture_view.as_ref().unwrap(),
                wgpu::FilterMode::Linear,
            ));
        }
    }
}

impl VideoRenderer {
    pub fn new(render_state: egui_wgpu::RenderState, rtc_track: Arc<VideoTrack>) -> Self {
        let internal = Arc::new(Mutex::new(RendererInternal {
            render_state,
            width: 0,
            height: 0,
            rgba_data: Vec::default(),
            texture: None,
            texture_view: None,
            egui_texture: None,
        }));

        rtc_track.on_frame({
            let internal = internal.clone();

            Box::new(move |_frame, buffer| {
                let span = debug_span!("texture_upload");
                let _enter = span.enter();

                let mut internal = internal.lock().unwrap();
                let buffer = buffer.to_i420();

                let width: u32 = buffer.width().try_into().unwrap();
                let height: u32 = buffer.height().try_into().unwrap();

                internal.ensure_texture_size(width, height);

                let rgba_ptr = internal.rgba_data.deref_mut();
                let rgba_stride = buffer.width() * 4;

                yuv_helper::i420_to_abgr(
                    buffer.data_y(),
                    buffer.stride_y(),
                    buffer.data_u(),
                    buffer.stride_u(),
                    buffer.data_v(),
                    buffer.stride_v(),
                    rgba_ptr,
                    rgba_stride,
                    buffer.width(),
                    buffer.height(),
                );

                let copy_desc = wgpu::ImageCopyTexture {
                    texture: internal.texture.as_ref().unwrap(),
                    mip_level: 0,
                    origin: wgpu::Origin3d::default(),
                    aspect: wgpu::TextureAspect::default(),
                };

                let copy_layout = wgpu::ImageDataLayout {
                    bytes_per_row: Some(NonZeroU32::new(width * 4).unwrap()),
                    ..Default::default()
                };

                let copy_size = wgpu::Extent3d {
                    width,
                    height,
                    ..Default::default()
                };

                internal.render_state.queue.write_texture(
                    copy_desc,
                    &internal.rgba_data,
                    copy_layout,
                    copy_size,
                );
            })
        });

        Self {
            rtc_track,
            internal,
        }
    }

    pub fn texture_id(&self) -> Option<egui::TextureId> {
        self.internal.lock().unwrap().egui_texture.clone()
    }
}

impl Drop for VideoRenderer {
    fn drop(&mut self) {
        self.rtc_track.on_frame(Box::new(|_, _| {}));
    }
}
