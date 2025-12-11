use eframe::egui;
use egui_wgpu as egui_wgpu_backend;
use egui_wgpu_backend::CallbackTrait;
use eframe::wgpu::{self, util::DeviceExt};
use parking_lot::Mutex;
use std::sync::Arc;

/// Shared I420 YUV frame storage for GPU rendering.
pub struct SharedYuv {
    pub width: u32,
    pub height: u32,
    pub stride_y: u32,
    pub stride_u: u32,
    pub stride_v: u32,
    pub y: Vec<u8>,
    pub u: Vec<u8>,
    pub v: Vec<u8>,
    pub dirty: bool,
    /// Optional user timestamp in microseconds since UNIX epoch.
    pub user_timestamp: Option<i64>,
}

/// egui-wgpu callback that renders a fullscreen quad from a `SharedYuv` buffer.
pub struct YuvPaintCallback {
    pub shared: Arc<Mutex<SharedYuv>>,
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
        let uv_size = wgpu::Extent3d {
            width: uv_pad_w,
            height: (height + 1) / 2,
            depth_or_array_layers: 1,
        };
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
                                    std::mem::size_of::<ParamsUniform>() as u64,
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

            // Initial tiny textures
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
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: params_buf.as_entire_binding(),
                    },
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
            let (y_tex, u_tex, v_tex, y_view, u_view, v_view) =
                YuvGpuState::create_textures(device, shared.width, shared.height, y_pad_w, uv_pad_w);
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
                    let src =
                        &shared.y[(row * shared.stride_y) as usize..][..shared.width as usize];
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
                    wgpu::Extent3d {
                        width: state.y_pad_w,
                        height: shared.height,
                        depth_or_array_layers: 1,
                    },
                );
            }

            // Pack and upload U,V
            if shared.stride_u >= uv_w && shared.stride_v >= uv_w {
                let mut packed_u = vec![0u8; (uv_bytes_per_row * uv_h) as usize];
                let mut packed_v = vec![0u8; (uv_bytes_per_row * uv_h) as usize];
                for row in 0..uv_h {
                    let src_u =
                        &shared.u[(row * shared.stride_u) as usize..][..uv_w as usize];
                    let src_v =
                        &shared.v[(row * shared.stride_v) as usize..][..uv_w as usize];
                    let dst_off = (row * uv_bytes_per_row) as usize;
                    packed_u[dst_off..dst_off + uv_w as usize].copy_from_slice(src_u);
                    packed_v[dst_off..dst_off + uv_w as usize].copy_from_slice(src_v);
                }
                queue.write_texture(
                    wgpu::ImageCopyTexture {
                        texture: &state.u_tex,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    &packed_u,
                    wgpu::ImageDataLayout {
                        offset: 0,
                        bytes_per_row: Some(uv_bytes_per_row),
                        rows_per_image: Some(uv_h),
                    },
                    wgpu::Extent3d {
                        width: state.uv_pad_w,
                        height: uv_h,
                        depth_or_array_layers: 1,
                    },
                );
                queue.write_texture(
                    wgpu::ImageCopyTexture {
                        texture: &state.v_tex,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    &packed_v,
                    wgpu::ImageDataLayout {
                        offset: 0,
                        bytes_per_row: Some(uv_bytes_per_row),
                        rows_per_image: Some(uv_h),
                    },
                    wgpu::Extent3d {
                        width: state.uv_pad_w,
                        height: uv_h,
                        depth_or_array_layers: 1,
                    },
                );
            }

            // Update params uniform
            let params = ParamsUniform {
                src_w: shared.width,
                src_h: shared.height,
                y_tex_w: state.y_pad_w,
                uv_tex_w: state.uv_pad_w,
            };
            queue.write_buffer(&state.params_buf, 0, bytemuck::bytes_of(&params));

            shared.dirty = false;
        }

        Vec::new()
    }

    fn paint(
        &self,
        _info: egui::PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'static>,
        resources: &egui_wgpu_backend::CallbackResources,
    ) {
        // Acquire current frame
        let shared = self.shared.lock();
        if shared.width == 0 || shared.height == 0 {
            return;
        }

        // Build pipeline and textures on first paint or on resize
        let Some(state) = resources.get::<YuvGpuState>() else {
            // prepare may not have created the state yet (race with first frame); skip this paint
            return;
        };
        
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


