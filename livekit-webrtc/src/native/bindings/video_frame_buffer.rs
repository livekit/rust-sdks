use cxx::UniquePtr;
use livekit_utils::enum_dispatch;
use std::pin::Pin;
use std::slice;
use webrtc_sys::video_frame_buffer as vfb_sys;

use crate::yuv_helper::{self, ConvertError};

macro_rules! recursive_cast {
    ($ptr:expr $(, $fnc:ident)*) => {
        {
            let ptr = $ptr;
            $(
                let ptr = unsafe { vfb_sys::ffi::$fnc(ptr) };
            )*
            ptr
        }
    };
}

macro_rules! impl_to_argb {
    (i420: $fnc:ty, $dst:ident, $dst_stride:ident, $dst_width:ident, $dst_height:ident) => {
        yuv_helper::$fnc(
            self.data_y(),
            self.stride_y(),
            self.data_u(),
            self.stride_u(),
            self.data_v(),
            self.stride_v(),
            $dst,
            $dst_stride,
            $dst_width,
            $dst_height,
        )
    };
}

#[derive(Debug)]
pub enum VideoFrameBufferType {
    Native,
    I420,
    I420A,
    I422,
    I444,
    I010,
    NV12,
}

#[derive(Debug)]
pub enum VideoFormatType {
    ARGB,
    BGRA,
    ABGR,
    RGBA,
}

impl From<vfb_sys::ffi::VideoFrameBufferType> for VideoFrameBufferType {
    fn from(buffer_type: vfb_sys::ffi::VideoFrameBufferType) -> Self {
        match buffer_type {
            vfb_sys::ffi::VideoFrameBufferType::Native => Self::Native,
            vfb_sys::ffi::VideoFrameBufferType::I420 => Self::I420,
            vfb_sys::ffi::VideoFrameBufferType::I420A => Self::I420A,
            vfb_sys::ffi::VideoFrameBufferType::I422 => Self::I422,
            vfb_sys::ffi::VideoFrameBufferType::I444 => Self::I444,
            vfb_sys::ffi::VideoFrameBufferType::I010 => Self::I010,
            vfb_sys::ffi::VideoFrameBufferType::NV12 => Self::NV12,
            _ => unreachable!(),
        }
    }
}

pub trait VideoFrameBufferTrait {
    fn buffer_type(&self) -> VideoFrameBufferType; // Useful for the FFI
    fn width(&self) -> i32;
    fn height(&self) -> i32;
    fn to_i420(self) -> I420Buffer;
}

pub trait PlanarYuvBuffer: VideoFrameBufferTrait {
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

pub trait BiplanarYuvBuffer: VideoFrameBufferTrait {
    fn chroma_width(&self) -> i32;
    fn chroma_height(&self) -> i32;
    fn stride_y(&self) -> i32;
    fn stride_uv(&self) -> i32;
}

pub trait BiplanarYuv8Buffer: BiplanarYuvBuffer {
    fn data_y(&self) -> &[u8];
    fn data_uv(&self) -> &[u8];
}

pub enum VideoFrameBuffer {
    Native(NativeBuffer),
    I420(I420Buffer),
    I420A(I420ABuffer),
    I422(I422Buffer),
    I444(I444Buffer),
    I010(I010Buffer),
    NV12(NV12Buffer),
}

impl VideoFrameBuffer {
    pub(crate) fn new(mut cxx_handle: UniquePtr<vfb_sys::ffi::VideoFrameBuffer>) -> Self {
        unsafe {
            match cxx_handle.buffer_type().into() {
                VideoFrameBufferType::Native => Self::Native(NativeBuffer::from(cxx_handle)),
                VideoFrameBufferType::I420 => {
                    Self::I420(I420Buffer::from(cxx_handle.pin_mut().get_i420()))
                }
                VideoFrameBufferType::I420A => {
                    Self::I420A(I420ABuffer::from(cxx_handle.pin_mut().get_i420a()))
                }
                VideoFrameBufferType::I422 => {
                    Self::I422(I422Buffer::from(cxx_handle.pin_mut().get_i422()))
                }
                VideoFrameBufferType::I444 => {
                    Self::I444(I444Buffer::from(cxx_handle.pin_mut().get_i444()))
                }
                VideoFrameBufferType::I010 => {
                    Self::I010(I010Buffer::from(cxx_handle.pin_mut().get_i010()))
                }
                VideoFrameBufferType::NV12 => {
                    Self::NV12(NV12Buffer::from(cxx_handle.pin_mut().get_nv12()))
                }
            }
        }
    }

    #[allow(unused_unsafe)]
    pub(crate) fn release(self) -> UniquePtr<vfb_sys::ffi::VideoFrameBuffer> {
        unsafe {
            match self {
                VideoFrameBuffer::Native(native) => native.release(),
                VideoFrameBuffer::I420(i420) => UniquePtr::from_raw(recursive_cast!(
                    i420.release().into_raw(),
                    i420_to_yuv8,
                    yuv8_to_yuv,
                    yuv_to_vfb
                ) as *mut _),
                VideoFrameBuffer::I420A(i420a) => UniquePtr::from_raw(recursive_cast!(
                    i420a.release().into_raw(),
                    i420a_to_yuv8,
                    yuv8_to_yuv,
                    yuv_to_vfb
                ) as *mut _),
                VideoFrameBuffer::I422(i422) => UniquePtr::from_raw(recursive_cast!(
                    i422.release().into_raw(),
                    i422_to_yuv8,
                    yuv8_to_yuv,
                    yuv_to_vfb
                ) as *mut _),
                VideoFrameBuffer::I444(i444) => UniquePtr::from_raw(recursive_cast!(
                    i444.release().into_raw(),
                    i444_to_yuv8,
                    yuv8_to_yuv,
                    yuv_to_vfb
                ) as *mut _),
                VideoFrameBuffer::I010(i010) => UniquePtr::from_raw(recursive_cast!(
                    i010.release().into_raw(),
                    i010_to_yuv16b,
                    yuv16b_to_yuv,
                    yuv_to_vfb
                ) as *mut _),
                VideoFrameBuffer::NV12(nv12) => UniquePtr::from_raw(recursive_cast!(
                    nv12.release().into_raw(),
                    nv12_to_biyuv8,
                    biyuv8_to_biyuv,
                    biyuv_to_vfb
                ) as *mut _),
            }
        }
    }

    pub fn to_argb(
        &self,
        format: VideoFormatType,
        dst: &mut [u8],
        dst_stride: i32,
        dst_width: i32,
        dst_height: i32,
    ) -> Result<(), ConvertError> {
        match self {
            Self::I420(i420) => match format {
                VideoFormatType::ARGB => yuv_helper::i420_to_argb(
                    i420.data_y(),
                    i420.stride_y(),
                    i420.data_u(),
                    i420.stride_u(),
                    i420.data_v(),
                    i420.stride_v(),
                    dst,
                    dst_stride,
                    dst_width,
                    dst_height,
                )?,
                VideoFormatType::BGRA => yuv_helper::i420_to_bgra(
                    i420.data_y(),
                    i420.stride_y(),
                    i420.data_u(),
                    i420.stride_u(),
                    i420.data_v(),
                    i420.stride_v(),
                    dst,
                    dst_stride,
                    dst_width,
                    dst_height,
                )?,
                VideoFormatType::ABGR => yuv_helper::i420_to_abgr(
                    i420.data_y(),
                    i420.stride_y(),
                    i420.data_u(),
                    i420.stride_u(),
                    i420.data_v(),
                    i420.stride_v(),
                    dst,
                    dst_stride,
                    dst_width,
                    dst_height,
                )?,
                VideoFormatType::RGBA => yuv_helper::i420_to_rgba(
                    i420.data_y(),
                    i420.stride_y(),
                    i420.data_u(),
                    i420.stride_u(),
                    i420.data_v(),
                    i420.stride_v(),
                    dst,
                    dst_stride,
                    dst_width,
                    dst_height,
                )?,
            },
            _ => {
                // TODO(theomonnom): Support other buffer types
            }
        };

        Ok(())
    }
}

impl VideoFrameBufferTrait for VideoFrameBuffer {
    enum_dispatch!(
        [Native, I420, I420A, I422, I444, I010, NV12]
        fnc!(buffer_type, &Self, [], VideoFrameBufferType);
        fnc!(width, &Self, [], i32);
        fnc!(height, &Self, [], i32);
        fnc!(to_i420, Self, [], I420Buffer);
    );
}

macro_rules! impl_video_frame_buffer {
    ($x:ty $(, $cast:ident)*) => {

        // Allow unused_unsafe when we don't do any cast ( e.g. NativeBuffer )
        #[allow(unused_unsafe)]
        impl VideoFrameBufferTrait for $x {
            fn buffer_type(&self) -> VideoFrameBufferType {
                let ptr = recursive_cast!(&*self.cxx_handle $(, $cast)*);
                unsafe {
                    (*ptr).buffer_type().into()
                }
            }

            fn width(&self) -> i32 {
                let ptr = recursive_cast!(&*self.cxx_handle $(, $cast)*);
                unsafe {
                    (*ptr).width()
                }
            }

            fn height(&self) -> i32 {
                let ptr = recursive_cast!(&*self.cxx_handle $(, $cast)*);
                unsafe {
                    (*ptr).height()
                }
            }

            // Require ownership because libwebrtc uses the same pointers
            fn to_i420(self) -> I420Buffer {
                let ptr = recursive_cast!(&*self.cxx_handle $(, $cast)*)
                    as *const vfb_sys::ffi::VideoFrameBuffer
                    as *mut vfb_sys::ffi::VideoFrameBuffer;

                unsafe {
                    I420Buffer::from(Pin::new_unchecked(&mut *ptr).to_i420())
                }
            }
        }
    };
}

macro_rules! impl_yuv_buffer {
    ($x:ty $(, $cast:ident)*) => {
        impl PlanarYuvBuffer for $x {
            fn chroma_width(&self) -> i32 {
                let ptr = recursive_cast!(&*self.cxx_handle $(, $cast)*);
                unsafe {
                    (*ptr).chroma_width()
                }
            }

            fn chroma_height(&self) -> i32 {
                let ptr = recursive_cast!(&*self.cxx_handle $(, $cast)*);
                unsafe {
                    (*ptr).chroma_height()
                }
            }

            fn stride_y(&self) -> i32 {
                let ptr = recursive_cast!(&*self.cxx_handle $(, $cast)*);
                unsafe {
                    (*ptr).stride_y()
                }
            }

            fn stride_u(&self) -> i32 {
                let ptr = recursive_cast!(&*self.cxx_handle $(, $cast)*);
                unsafe {
                    (*ptr).stride_u()
                }
            }

            fn stride_v(&self) -> i32 {
                let ptr = recursive_cast!(&*self.cxx_handle $(, $cast)*);
                unsafe {
                    (*ptr).stride_v()
                }
            }
        }
    };
}

macro_rules! impl_yuv8_buffer {
    ($x:ty $(, $cast:ident)*) => {
        impl PlanarYuv8Buffer for $x {
            fn data_y(&self) -> &[u8] {
                let ptr = recursive_cast!(&*self.cxx_handle $(, $cast)*);
                unsafe {
                    slice::from_raw_parts((*ptr).data_y(), (self.width() * self.height()) as usize)
                }
            }

            fn data_u(&self) -> &[u8] {
                let ptr = recursive_cast!(&*self.cxx_handle $(, $cast)*);
                unsafe {
                    let chroma_height = (self.height() + 1) / 2;
                    slice::from_raw_parts((*ptr).data_u(), (self.stride_u() * chroma_height) as usize)
                }
            }

            fn data_v(&self) -> &[u8] {
                let ptr = recursive_cast!(&*self.cxx_handle $(, $cast)*);
                unsafe {
                    let chroma_height = (self.height() + 1) / 2;
                    slice::from_raw_parts((*ptr).data_v(), (self.stride_v() * chroma_height) as usize)
                }
            }
        }
    };
}

macro_rules! impl_yuv16_buffer {
    ($x:ty $(, $cast:ident)*) => {
        impl PlanarYuv16BBuffer for $x {
            fn data_y(&self) -> &[u16] {
                let ptr = recursive_cast!(&*self.cxx_handle $(, $cast)*);
                unsafe {
                    slice::from_raw_parts((*ptr).data_y(), (self.width() * self.height()) as usize)
                }
            }

            fn data_u(&self) -> &[u16] {
                let ptr = recursive_cast!(&*self.cxx_handle $(, $cast)*);
                unsafe {
                    let chroma_height = (self.height() + 1) / 2;
                    slice::from_raw_parts((*ptr).data_u(), (self.stride_u() * chroma_height) as usize)
                }
            }

            fn data_v(&self) -> &[u16] {
                let ptr = recursive_cast!(&*self.cxx_handle $(, $cast)*);
                unsafe {
                    let chroma_height = (self.height() + 1) / 2;
                    slice::from_raw_parts((*ptr).data_v(), (self.stride_v() * chroma_height) as usize)
                }
            }
        }
    };
}

macro_rules! impl_biyuv_buffer {
    ($x:ty $(, $cast:ident)*) => {
        impl BiplanarYuvBuffer for $x {
            fn chroma_width(&self) -> i32 {
                let ptr = recursive_cast!(&*self.cxx_handle $(, $cast)*);
                unsafe {
                    (*ptr).chroma_width()
                }
            }

            fn chroma_height(&self) -> i32 {
                let ptr = recursive_cast!(&*self.cxx_handle $(, $cast)*);
                unsafe {
                    (*ptr).chroma_height()
                }
            }

            fn stride_y(&self) -> i32 {
                let ptr = recursive_cast!(&*self.cxx_handle $(, $cast)*);
                unsafe {
                    (*ptr).stride_y()
                }
            }

            fn stride_uv(&self) -> i32 {
                let ptr = recursive_cast!(&*self.cxx_handle $(, $cast)*);
                unsafe {
                    (*ptr).stride_uv()
                }
            }
        }
    };
}

macro_rules! impl_biyuv8_buffer {
    ($x:ty $(, $cast:ident)*) => {
        impl BiplanarYuv8Buffer for $x {
            fn data_y(&self) -> &[u8] {
                let ptr = recursive_cast!(&*self.cxx_handle $(, $cast)*);
                unsafe {
                    slice::from_raw_parts((*ptr).data_y(), (self.width() * self.height()) as usize)
                }
            }

            fn data_uv(&self) -> &[u8] {
                let ptr = recursive_cast!(&*self.cxx_handle $(, $cast)*);
                unsafe {
                    let chroma_height = (self.height() + 1) / 2;
                    slice::from_raw_parts((*ptr).data_uv(), (self.stride_uv() * chroma_height) as usize)
                }
            }
        }
    };
}

pub struct NativeBuffer {
    cxx_handle: UniquePtr<vfb_sys::ffi::VideoFrameBuffer>,
}

pub struct I420Buffer {
    cxx_handle: UniquePtr<vfb_sys::ffi::I420Buffer>,
}

pub struct I420ABuffer {
    cxx_handle: UniquePtr<vfb_sys::ffi::I420ABuffer>,
}

pub struct I422Buffer {
    cxx_handle: UniquePtr<vfb_sys::ffi::I422Buffer>,
}

pub struct I444Buffer {
    cxx_handle: UniquePtr<vfb_sys::ffi::I444Buffer>,
}

pub struct I010Buffer {
    cxx_handle: UniquePtr<vfb_sys::ffi::I010Buffer>,
}

pub struct NV12Buffer {
    cxx_handle: UniquePtr<vfb_sys::ffi::NV12Buffer>,
}

impl_video_frame_buffer!(NativeBuffer);
impl_video_frame_buffer!(I420Buffer, i420_to_yuv8, yuv8_to_yuv, yuv_to_vfb);
impl_video_frame_buffer!(I420ABuffer, i420a_to_yuv8, yuv8_to_yuv, yuv_to_vfb);
impl_video_frame_buffer!(I422Buffer, i422_to_yuv8, yuv8_to_yuv, yuv_to_vfb);
impl_video_frame_buffer!(I444Buffer, i444_to_yuv8, yuv8_to_yuv, yuv_to_vfb);
impl_video_frame_buffer!(I010Buffer, i010_to_yuv16b, yuv16b_to_yuv, yuv_to_vfb);
impl_video_frame_buffer!(NV12Buffer, nv12_to_biyuv8, biyuv8_to_biyuv, biyuv_to_vfb);

impl_yuv_buffer!(I420Buffer, i420_to_yuv8, yuv8_to_yuv);
impl_yuv_buffer!(I420ABuffer, i420a_to_yuv8, yuv8_to_yuv);
impl_yuv_buffer!(I422Buffer, i422_to_yuv8, yuv8_to_yuv);
impl_yuv_buffer!(I444Buffer, i444_to_yuv8, yuv8_to_yuv);
impl_yuv_buffer!(I010Buffer, i010_to_yuv16b, yuv16b_to_yuv);

impl_yuv8_buffer!(I420Buffer, i420_to_yuv8);
impl_yuv8_buffer!(I420ABuffer, i420a_to_yuv8);
impl_yuv8_buffer!(I422Buffer, i422_to_yuv8);
impl_yuv8_buffer!(I444Buffer, i444_to_yuv8);

impl_yuv16_buffer!(I010Buffer, i010_to_yuv16b);

impl_biyuv_buffer!(NV12Buffer, nv12_to_biyuv8, biyuv8_to_biyuv);

impl_biyuv8_buffer!(NV12Buffer, nv12_to_biyuv8);

impl NativeBuffer {
    fn from(cxx_handle: UniquePtr<vfb_sys::ffi::VideoFrameBuffer>) -> Self {
        Self { cxx_handle }
    }

    pub(crate) fn release(self) -> UniquePtr<vfb_sys::ffi::VideoFrameBuffer> {
        self.cxx_handle
    }
}

impl I420Buffer {
    pub fn new(width: u32, height: u32) -> Self {
        Self::from(vfb_sys::ffi::create_i420_buffer(
            width as i32,
            height as i32,
        ))
    }

    fn from(cxx_handle: UniquePtr<vfb_sys::ffi::I420Buffer>) -> Self {
        Self { cxx_handle }
    }

    pub(crate) fn release(self) -> UniquePtr<vfb_sys::ffi::I420Buffer> {
        self.cxx_handle
    }
}

impl I420ABuffer {
    fn from(cxx_handle: UniquePtr<vfb_sys::ffi::I420ABuffer>) -> Self {
        Self { cxx_handle }
    }

    pub(crate) fn release(self) -> UniquePtr<vfb_sys::ffi::I420ABuffer> {
        self.cxx_handle
    }
}

impl I422Buffer {
    fn from(cxx_handle: UniquePtr<vfb_sys::ffi::I422Buffer>) -> Self {
        Self { cxx_handle }
    }

    pub(crate) fn release(self) -> UniquePtr<vfb_sys::ffi::I422Buffer> {
        self.cxx_handle
    }
}

impl I444Buffer {
    fn from(cxx_handle: UniquePtr<vfb_sys::ffi::I444Buffer>) -> Self {
        Self { cxx_handle }
    }

    pub(crate) fn release(self) -> UniquePtr<vfb_sys::ffi::I444Buffer> {
        self.cxx_handle
    }
}

impl I010Buffer {
    fn from(cxx_handle: UniquePtr<vfb_sys::ffi::I010Buffer>) -> Self {
        Self { cxx_handle }
    }

    pub(crate) fn release(self) -> UniquePtr<vfb_sys::ffi::I010Buffer> {
        self.cxx_handle
    }
}

impl NV12Buffer {
    fn from(cxx_handle: UniquePtr<vfb_sys::ffi::NV12Buffer>) -> Self {
        Self { cxx_handle }
    }

    pub(crate) fn release(self) -> UniquePtr<vfb_sys::ffi::NV12Buffer> {
        self.cxx_handle
    }
}
