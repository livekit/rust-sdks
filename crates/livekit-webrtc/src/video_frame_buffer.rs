use cxx::UniquePtr;
use libwebrtc_sys::video_frame_buffer as vfb_sys;
use std::pin::Pin;
use std::slice;
use vfb_sys::ffi::VideoFrameBufferType;

pub trait VideoFrameBufferTrait {
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
            match cxx_handle.buffer_type() {
                VideoFrameBufferType::Native => Self::Native(NativeBuffer::new(cxx_handle)),
                VideoFrameBufferType::I420 => {
                    Self::I420(I420Buffer::new(cxx_handle.pin_mut().get_i420()))
                }
                VideoFrameBufferType::I420A => Self::I420A(I420ABuffer::new(cxx_handle)),
                VideoFrameBufferType::I422 => Self::I422(I422Buffer::new(cxx_handle)),
                VideoFrameBufferType::I444 => Self::I444(I444Buffer::new(cxx_handle)),
                VideoFrameBufferType::I010 => Self::I010(I010Buffer::new(cxx_handle)),
                VideoFrameBufferType::NV12 => Self::NV12(NV12Buffer::new(cxx_handle)),
                _ => unreachable!(), // VideoFrameBufferType is represented as i32
            }
        }
    }
}

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

macro_rules! impl_video_frame_buffer {
    ($x:ty $(, $cast:ident)*) => {

        // Allow unused_unsafe when we don't do any cast ( e.g. NativeBuffer )
        #[allow(unused_unsafe)]
        impl VideoFrameBufferTrait for $x {
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
                    I420Buffer::new(Pin::new_unchecked(&mut *ptr).to_i420())
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
                    slice::from_raw_parts((*ptr).data_y(), self.stride_y().try_into().unwrap())
                }
            }

            fn data_u(&self) -> &[u8] {
                let ptr = recursive_cast!(&*self.cxx_handle $(, $cast)*);
                unsafe {
                    slice::from_raw_parts((*ptr).data_u(), self.stride_u().try_into().unwrap())
                }
            }

            fn data_v(&self) -> &[u8] {
                let ptr = recursive_cast!(&*self.cxx_handle $(, $cast)*);
                unsafe {
                    slice::from_raw_parts((*ptr).data_v(), self.stride_v().try_into().unwrap())
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
    cxx_handle: UniquePtr<vfb_sys::ffi::VideoFrameBuffer>,
}

pub struct I422Buffer {
    cxx_handle: UniquePtr<vfb_sys::ffi::VideoFrameBuffer>,
}

pub struct I444Buffer {
    cxx_handle: UniquePtr<vfb_sys::ffi::VideoFrameBuffer>,
}

pub struct I010Buffer {
    cxx_handle: UniquePtr<vfb_sys::ffi::VideoFrameBuffer>,
}

pub struct NV12Buffer {
    cxx_handle: UniquePtr<vfb_sys::ffi::VideoFrameBuffer>,
}

impl_video_frame_buffer!(NativeBuffer);
impl_video_frame_buffer!(I420Buffer, i420_to_yuv8, yuv8_to_yuv, yuv_to_vfb);
impl_video_frame_buffer!(I420ABuffer);
impl_video_frame_buffer!(I422Buffer);
impl_video_frame_buffer!(I444Buffer);
impl_video_frame_buffer!(I010Buffer);
impl_video_frame_buffer!(NV12Buffer);

impl_yuv_buffer!(I420Buffer, i420_to_yuv8, yuv8_to_yuv);

impl_yuv8_buffer!(I420Buffer, i420_to_yuv8);

impl NativeBuffer {
    fn new(cxx_handle: UniquePtr<vfb_sys::ffi::VideoFrameBuffer>) -> Self {
        Self { cxx_handle }
    }
}

impl I420Buffer {
    fn new(cxx_handle: UniquePtr<vfb_sys::ffi::I420Buffer>) -> Self {
        Self { cxx_handle }
    }
}

impl I420ABuffer {
    fn new(cxx_handle: UniquePtr<vfb_sys::ffi::VideoFrameBuffer>) -> Self {
        Self { cxx_handle }
    }
}

impl I422Buffer {
    fn new(cxx_handle: UniquePtr<vfb_sys::ffi::VideoFrameBuffer>) -> Self {
        Self { cxx_handle }
    }
}

impl I444Buffer {
    fn new(cxx_handle: UniquePtr<vfb_sys::ffi::VideoFrameBuffer>) -> Self {
        Self { cxx_handle }
    }
}

impl I010Buffer {
    fn new(cxx_handle: UniquePtr<vfb_sys::ffi::VideoFrameBuffer>) -> Self {
        Self { cxx_handle }
    }
}

impl NV12Buffer {
    fn new(cxx_handle: UniquePtr<vfb_sys::ffi::VideoFrameBuffer>) -> Self {
        Self { cxx_handle }
    }
}
