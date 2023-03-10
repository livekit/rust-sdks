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
    pub rotation: VideoRotation,
    pub buffer: T,
}

pub type BoxVideoFrame = VideoFrame<Box<dyn VideoFrameBuffer + Send + Sync>>;

macro_rules! new_buffer_type {
    ($x:ident) => {
        pub struct $x {
            pub(crate) handle: vf_imp::$x,
        }

        impl $crate::video_frame::internal::BufferInternal for $x {
            #[cfg(not(target_arch = "wasm32"))]
            fn sys_handle(&self) -> &webrtc_sys::video_frame_buffer::ffi::VideoFrameBuffer {
                self.handle.sys_handle()
            }

            #[cfg(not(target_arch = "wasm32"))]
            fn to_i420(&self) -> I420Buffer {
                I420Buffer {
                    handle: self.handle.to_i420(),
                }
            }

            #[cfg(not(target_arch = "wasm32"))]
            fn to_argb(
                &self,
                format: VideoFormatType,
                dst: &mut [u8],
                stride: i32,
                width: i32,
                height: i32,
            ) -> Result<(), $crate::video_frame::native::ConvertError> {
                self.handle.to_argb(format, dst, stride, width, height)
            }
        }

        impl VideoFrameBuffer for $x {
            fn width(&self) -> i32 {
                self.handle.width()
            }

            fn height(&self) -> i32 {
                self.handle.height()
            }
        }

        impl Debug for $x {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.debug_struct(stringify!($x))
                    .field("width", &self.width())
                    .field("height", &self.height())
                    .finish()
            }
        }
    };
}

new_buffer_type!(I420Buffer);
new_buffer_type!(I420ABuffer);
new_buffer_type!(I422Buffer);
new_buffer_type!(I444Buffer);
new_buffer_type!(I010Buffer);
new_buffer_type!(NV12Buffer);

pub(crate) mod internal {
    use super::{I420Buffer, VideoFormatType};

    pub trait BufferInternal {
        #[cfg(not(target_arch = "wasm32"))]
        fn sys_handle(&self) -> &webrtc_sys::video_frame_buffer::ffi::VideoFrameBuffer;

        #[cfg(not(target_arch = "wasm32"))]
        fn to_i420(&self) -> I420Buffer;

        #[cfg(not(target_arch = "wasm32"))]
        fn to_argb(
            &self,
            format: VideoFormatType,
            dst: &mut [u8],
            dst_stride: i32,
            dst_width: i32,
            dst_height: i32,
        ) -> Result<(), super::native::ConvertError>;
    }
}

pub trait VideoFrameBuffer: internal::BufferInternal + Debug {
    fn width(&self) -> i32;
    fn height(&self) -> i32;
}

impl I420Buffer {
    pub fn chroma_width(&self) -> i32 {
        self.handle.chroma_width()
    }

    pub fn chroma_height(&self) -> i32 {
        self.handle.chroma_height()
    }

    pub fn stride_y(&self) -> i32 {
        self.handle.stride_y()
    }

    pub fn stride_u(&self) -> i32 {
        self.handle.stride_u()
    }

    pub fn stride_v(&self) -> i32 {
        self.handle.stride_v()
    }

    pub fn data(&self) -> (&[u8], &[u8], &[u8]) {
        self.handle.data()
    }

    pub fn data_mut(&mut self) -> (&mut [u8], &mut [u8], &mut [u8]) {
        let (data_y, data_u, data_v) = self.handle.data();
        unsafe {
            (
                std::slice::from_raw_parts_mut(data_y.as_ptr() as *mut u8, data_y.len()),
                std::slice::from_raw_parts_mut(data_u.as_ptr() as *mut u8, data_u.len()),
                std::slice::from_raw_parts_mut(data_v.as_ptr() as *mut u8, data_v.len()),
            )
        }
    }
}

impl I420ABuffer {
    pub fn chroma_width(&self) -> i32 {
        self.handle.chroma_width()
    }

    pub fn chroma_height(&self) -> i32 {
        self.handle.chroma_height()
    }

    pub fn stride_y(&self) -> i32 {
        self.handle.stride_y()
    }

    pub fn stride_u(&self) -> i32 {
        self.handle.stride_u()
    }

    pub fn stride_v(&self) -> i32 {
        self.handle.stride_v()
    }

    pub fn stride_a(&self) -> i32 {
        self.handle.stride_a()
    }

    pub fn data(&self) -> (&[u8], &[u8], &[u8], Option<&[u8]>) {
        self.handle.data()
    }

    pub fn data_mut(&self) -> (&mut [u8], &mut [u8], &mut [u8], Option<&mut [u8]>) {
        let (data_y, data_u, data_v, data_a) = self.handle.data();
        unsafe {
            (
                std::slice::from_raw_parts_mut(data_y.as_ptr() as *mut u8, data_y.len()),
                std::slice::from_raw_parts_mut(data_u.as_ptr() as *mut u8, data_u.len()),
                std::slice::from_raw_parts_mut(data_v.as_ptr() as *mut u8, data_v.len()),
                data_a.map(|data_a| {
                    std::slice::from_raw_parts_mut(data_a.as_ptr() as *mut u8, data_a.len())
                }),
            )
        }
    }
}

impl I422Buffer {
    pub fn chroma_width(&self) -> i32 {
        self.handle.chroma_width()
    }

    pub fn chroma_height(&self) -> i32 {
        self.handle.chroma_height()
    }

    pub fn stride_y(&self) -> i32 {
        self.handle.stride_y()
    }

    pub fn stride_u(&self) -> i32 {
        self.handle.stride_u()
    }

    pub fn stride_v(&self) -> i32 {
        self.handle.stride_v()
    }
}

impl I444Buffer {
    pub fn chroma_width(&self) -> i32 {
        self.handle.chroma_width()
    }

    pub fn chroma_height(&self) -> i32 {
        self.handle.chroma_height()
    }

    pub fn stride_y(&self) -> i32 {
        self.handle.stride_y()
    }

    pub fn stride_u(&self) -> i32 {
        self.handle.stride_u()
    }

    pub fn stride_v(&self) -> i32 {
        self.handle.stride_v()
    }
}

impl I010Buffer {
    pub fn chroma_width(&self) -> i32 {
        self.handle.chroma_width()
    }

    pub fn chroma_height(&self) -> i32 {
        self.handle.chroma_height()
    }

    pub fn stride_y(&self) -> i32 {
        self.handle.stride_y()
    }

    pub fn stride_u(&self) -> i32 {
        self.handle.stride_u()
    }

    pub fn stride_v(&self) -> i32 {
        self.handle.stride_v()
    }
}

impl NV12Buffer {
    pub fn chroma_width(&self) -> i32 {
        self.handle.chroma_width()
    }

    pub fn chroma_height(&self) -> i32 {
        self.handle.chroma_height()
    }

    pub fn stride_y(&self) -> i32 {
        self.handle.stride_y()
    }

    pub fn stride_uv(&self) -> i32 {
        self.handle.stride_uv()
    }
}

impl<T: VideoFrameBuffer + ?Sized> internal::BufferInternal for Box<T> {
    fn sys_handle(&self) -> &webrtc_sys::video_frame_buffer::ffi::VideoFrameBuffer {
        self.as_ref().sys_handle()
    }

    fn to_i420(&self) -> I420Buffer {
        self.as_ref().to_i420()
    }

    fn to_argb(
        &self,
        format: VideoFormatType,
        dst: &mut [u8],
        dst_stride: i32,
        dst_width: i32,
        dst_height: i32,
    ) -> Result<(), self::native::ConvertError> {
        self.as_ref()
            .to_argb(format, dst, dst_stride, dst_width, dst_height)
    }
}

impl<T: VideoFrameBuffer + ?Sized> VideoFrameBuffer for Box<T> {
    fn width(&self) -> i32 {
        self.as_ref().width()
    }

    fn height(&self) -> i32 {
        self.as_ref().height()
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub mod native {
    use super::{vf_imp, I420Buffer, VideoFormatType, VideoFrameBuffer};
    use std::fmt::Debug;

    pub use crate::imp::yuv_helper::ConvertError;

    new_buffer_type!(NativeBuffer);

    pub trait I420BufferExt {
        fn new(width: u32, height: u32) -> I420Buffer;
    }

    impl I420BufferExt for I420Buffer {
        fn new(width: u32, height: u32) -> I420Buffer {
            vf_imp::I420Buffer::new(width, height)
        }
    }

    pub trait VideoFrameBufferExt: VideoFrameBuffer {
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

    impl<T: VideoFrameBuffer> VideoFrameBufferExt for T {
        fn to_i420(&self) -> I420Buffer {
            self.to_i420()
        }

        fn to_argb(
            &self,
            format: VideoFormatType,
            dst: &mut [u8],
            dst_stride: i32,
            dst_width: i32,
            dst_height: i32,
        ) -> Result<(), ConvertError> {
            self.to_argb(format, dst, dst_stride, dst_width, dst_height)
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub mod web {
    use super::VideoFrameBuffer;

    #[derive(Debug)]
    pub struct WebGlBuffer {}

    impl VideoFrameBuffer for WebGlBuffer {}
}
