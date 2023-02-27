use crate::imp::video_frame as vf_imp;
use std::fmt::Debug;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SinkError {
    #[error("platform error: {0}")]
    Platform(String),
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum VideoRotation {
    VideoRotation0 = 0,
    VideoRotation90 = 90,
    VideoRotation180 = 180,
    VideoRotation270 = 270,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum VideoFormatType {
    ARGB,
    BGRA,
    ABGR,
    RGBA,
}

#[derive(Debug)]
pub struct VideoFrame<T>
where
    T: VideoFrameBuffer,
{
    pub id: u16,
    pub rotation: VideoRotation,
    pub buffer: T,
}

pub trait VideoFrameBuffer: Debug {
    fn width(&self) -> i32;
    fn height(&self) -> i32;
}

pub trait PlanarYuvBuffer: VideoFrameBuffer {
    fn chroma_width(&self) -> i32;
    fn chroma_height(&self) -> i32;
    fn stride_y(&self) -> i32;
    fn stride_u(&self) -> i32;
    fn stride_v(&self) -> i32;
}

pub trait PlanarYuv8Buffer: PlanarYuvBuffer {
    fn data_y(&self) -> &[u8];
    fn data_u(&self) -> &[u8];
    fn data_v(&self) -> &[u8];
}

pub trait PlanarYuv16BBuffer: PlanarYuvBuffer {
    fn data_y(&self) -> &[u16];
    fn data_u(&self) -> &[u16];
    fn data_v(&self) -> &[u16];
}

pub trait BiplanarYuvBuffer: VideoFrameBuffer {
    fn chroma_width(&self) -> i32;
    fn chroma_height(&self) -> i32;
    fn stride_y(&self) -> i32;
    fn stride_uv(&self) -> i32;
}

pub trait BiplanarYuv8Buffer: BiplanarYuvBuffer {
    fn data_y(&self) -> &[u8];
    fn data_uv(&self) -> &[u8];
}

macro_rules! impl_video_frame_buffer {
    ($x:ty) => {
        impl VideoFrameBuffer for $x {
            fn width(&self) -> i32 {
                self.handle.width()
            }

            fn height(&self) -> i32 {
                self.handle.height()
            }
        }
    };
}

macro_rules! impl_planar_yuv_buffer {
    ($x:ty) => {
        impl PlanarYuvBuffer for $x {
            fn chroma_width(&self) -> i32 {
                self.handle.chroma_width()
            }

            fn chroma_height(&self) -> i32 {
                self.handle.chroma_height()
            }

            fn stride_y(&self) -> i32 {
                self.handle.stride_y()
            }

            fn stride_u(&self) -> i32 {
                self.handle.stride_u()
            }

            fn stride_v(&self) -> i32 {
                self.handle.stride_v()
            }
        }
    };
}

macro_rules! impl_planar_yuv8_buffer {
    ($x:ty) => {
        impl PlanarYuv8Buffer for $x {
            fn data_y(&self) -> &[u8] {
                self.handle.data_y()
            }

            fn data_u(&self) -> &[u8] {
                self.handle.data_u()
            }

            fn data_v(&self) -> &[u8] {
                self.handle.data_v()
            }
        }
    };
}

macro_rules! impl_planar_yuv16b_buffer {
    ($x:ty) => {
        impl PlanarYuv16BBuffer for $x {
            fn data_y(&self) -> &[u16] {
                self.handle.data_y()
            }

            fn data_u(&self) -> &[u16] {
                self.handle.data_u()
            }

            fn data_v(&self) -> &[u16] {
                self.handle.data_v()
            }
        }
    };
}

macro_rules! impl_biplanar_yuv_buffer {
    ($x:ty) => {
        impl BiplanarYuvBuffer for $x {
            fn chroma_width(&self) -> i32 {
                self.handle.chroma_width()
            }

            fn chroma_height(&self) -> i32 {
                self.handle.chroma_height()
            }

            fn stride_y(&self) -> i32 {
                self.handle.stride_y()
            }

            fn stride_uv(&self) -> i32 {
                self.handle.stride_uv()
            }
        }
    };
}

macro_rules! impl_biplanar_yuv8_buffer {
    ($x:ty) => {
        impl BiplanarYuv8Buffer for $x {
            fn data_y(&self) -> &[u8] {
                self.handle.data_y()
            }

            fn data_uv(&self) -> &[u8] {
                self.handle.data_uv()
            }
        }
    };
}

pub struct I420Buffer {
    pub(crate) handle: vf_imp::I420Buffer,
}

impl_video_frame_buffer!(I420Buffer);
impl_planar_yuv_buffer!(I420Buffer);
impl_planar_yuv8_buffer!(I420Buffer);

impl Debug for I420Buffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("I420Buffer")
            .field("width", &self.width())
            .field("height", &self.height())
            .finish()
    }
}

pub struct I420ABuffer {
    pub(crate) handle: vf_imp::I420ABuffer,
}

impl I420ABuffer {
    pub fn stride_a(&self) -> i32 {
        self.handle.stride_a()
    }

    pub fn data_a(&self) -> Option<&[u8]> {
        self.handle.data_a()
    }
}

impl_video_frame_buffer!(I420ABuffer);
impl_planar_yuv_buffer!(I420ABuffer);
impl_planar_yuv8_buffer!(I420ABuffer);

impl Debug for I420ABuffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("I420ABuffer")
            .field("width", &self.width())
            .field("height", &self.height())
            .field("data_a", &self.data_a())
            .finish()
    }
}

pub struct I422Buffer {
    pub(crate) handle: vf_imp::I422Buffer,
}

impl_video_frame_buffer!(I422Buffer);
impl_planar_yuv_buffer!(I422Buffer);
impl_planar_yuv8_buffer!(I422Buffer);

impl Debug for I422Buffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("I422Buffer")
            .field("width", &self.width())
            .field("height", &self.height())
            .finish()
    }
}

pub struct I444Buffer {
    pub(crate) handle: vf_imp::I444Buffer,
}

impl_video_frame_buffer!(I444Buffer);
impl_planar_yuv_buffer!(I444Buffer);
impl_planar_yuv8_buffer!(I444Buffer);

impl Debug for I444Buffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("I444Buffer")
            .field("width", &self.width())
            .field("height", &self.height())
            .finish()
    }
}

pub struct I010Buffer {
    pub(crate) handle: vf_imp::I010Buffer,
}

impl_video_frame_buffer!(I010Buffer);
impl_planar_yuv_buffer!(I010Buffer);
impl_planar_yuv16b_buffer!(I010Buffer);

impl Debug for I010Buffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("I010Buffer")
            .field("width", &self.width())
            .field("height", &self.height())
            .finish()
    }
}

pub struct NV12Buffer {
    pub(crate) handle: vf_imp::NV12Buffer,
}

impl_video_frame_buffer!(NV12Buffer);
impl_biplanar_yuv_buffer!(NV12Buffer);
impl_biplanar_yuv8_buffer!(NV12Buffer);

impl Debug for NV12Buffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NV12Buffer")
            .field("width", &self.width())
            .field("height", &self.height())
            .finish()
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub mod native {
    use std::fmt::Debug;

    use super::{
        vf_imp, I010Buffer, I420ABuffer, I420Buffer, I422Buffer, I444Buffer, NV12Buffer,
        VideoFormatType, VideoFrameBuffer,
    };

    pub use crate::imp::yuv_helper::ConvertError;

    pub trait VideoFrameBufferExt: VideoFrameBuffer + Debug {
        fn to_i420(&self) -> I420Buffer;
        fn to_argb(
            &self,
            format: VideoFormatType,
            dst: &mut [u8],
            dst_stride: i32,
            dst_width: i32,
            dst_height: i32,
        ) -> Result<(), ConvertError>;
    }

    macro_rules! impl_video_frame_buffer_winext {
        ($x:ty) => {
            impl VideoFrameBufferExt for $x {
                fn to_i420(&self) -> I420Buffer {
                    I420Buffer {
                        handle: self.handle.to_i420(),
                    }
                }

                fn to_argb(
                    &self,
                    format: VideoFormatType,
                    dst: &mut [u8],
                    dst_stride: i32,
                    dst_width: i32,
                    dst_height: i32,
                ) -> Result<(), ConvertError> {
                    self.handle
                        .to_argb(format, dst, dst_stride, dst_width, dst_height)
                }
            }
        };
    }

    pub struct NativeBuffer {
        pub(crate) handle: vf_imp::NativeBuffer,
    }

    impl_video_frame_buffer!(NativeBuffer);

    impl Debug for NativeBuffer {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("NativeBuffer")
                .field("width", &self.width())
                .field("height", &self.height())
                .finish()
        }
    }

    impl_video_frame_buffer_winext!(NativeBuffer);
    impl_video_frame_buffer_winext!(I420Buffer);
    impl_video_frame_buffer_winext!(I420ABuffer);
    impl_video_frame_buffer_winext!(I422Buffer);
    impl_video_frame_buffer_winext!(I444Buffer);
    impl_video_frame_buffer_winext!(I010Buffer);
    impl_video_frame_buffer_winext!(NV12Buffer);
}

#[cfg(target_arch = "wasm32")]
pub mod web {
    use super::VideoFrameBuffer;

    #[derive(Debug)]
    pub struct WebGlBuffer {}

    impl VideoFrameBuffer for WebGlBuffer {}
}
