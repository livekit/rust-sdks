use super::yuv_helper::{self, ConvertError};
use crate::video_frame::{self as vf, VideoFormatType, VideoFrameBuffer};
use cxx::UniquePtr;
use std::slice;
use webrtc_sys::video_frame_buffer as vfb_sys;

/// We don't use vf::VideoFrameBuffer trait for the types inside this module to avoid confusion
/// because irectly using platform specific types is not valid (e.g user callback)
/// All the types inside this module are only used internally. For public types, see the top level video_frame.rs

pub fn new_video_frame_buffer(
    mut sys_handle: UniquePtr<vfb_sys::ffi::VideoFrameBuffer>,
) -> Box<dyn vf::VideoFrameBuffer> {
    unsafe {
        match sys_handle.buffer_type().into() {
            vfb_sys::ffi::VideoFrameBufferType::Native => Box::new(vf::native::NativeBuffer {
                handle: NativeBuffer { sys_handle },
            }),
            vfb_sys::ffi::VideoFrameBufferType::I420 => Box::new(vf::I420Buffer {
                handle: I420Buffer {
                    sys_handle: sys_handle.pin_mut().get_i420(),
                },
            }),
            vfb_sys::ffi::VideoFrameBufferType::I420A => Box::new(vf::I420ABuffer {
                handle: I420ABuffer {
                    sys_handle: sys_handle.pin_mut().get_i420a(),
                },
            }),
            vfb_sys::ffi::VideoFrameBufferType::I422 => Box::new(vf::I422Buffer {
                handle: I422Buffer {
                    sys_handle: sys_handle.pin_mut().get_i422(),
                },
            }),
            vfb_sys::ffi::VideoFrameBufferType::I444 => Box::new(vf::I444Buffer {
                handle: I444Buffer {
                    sys_handle: sys_handle.pin_mut().get_i444(),
                },
            }),
            vfb_sys::ffi::VideoFrameBufferType::I010 => Box::new(vf::I010Buffer {
                handle: I010Buffer {
                    sys_handle: sys_handle.pin_mut().get_i010(),
                },
            }),
            vfb_sys::ffi::VideoFrameBufferType::NV12 => Box::new(vf::NV12Buffer {
                handle: NV12Buffer {
                    sys_handle: sys_handle.pin_mut().get_nv12(),
                },
            }),
            _ => unreachable!(),
        }
    }
}

macro_rules! recursive_cast {
    ($ptr:expr $(, $fnc:ident)*) => {
        {
            let ptr = $ptr;
            $(
                let ptr = vfb_sys::ffi::$fnc(ptr);
            )*
            ptr
        }
    };
}

macro_rules! impl_to_argb {
    (I420Buffer [$($variant:ident: $fnc:ident),+], $format:ident, $self:ident, $dst:ident, $dst_stride:ident, $dst_width:ident, $dst_height:ident) => {
        match $format {
        $(
            VideoFormatType::$variant => {
                yuv_helper::$fnc(
                    $self.data_y(),
                    $self.stride_y(),
                    $self.data_u(),
                    $self.stride_u(),
                    $self.data_v(),
                    $self.stride_v(),
                    $dst,
                    $dst_stride,
                    $dst_width,
                    $dst_height,
                )
            }
        )+
        }
    };
    (I420ABuffer) => {
        todo!();
    }
}

macro_rules! impl_vfb_buffer {
    ($($cast:ident),*) => {
        pub fn width(&self) -> i32 {
            unsafe {
                let ptr = recursive_cast!(&*self.sys_handle $(, $cast)*);
                (*ptr).width()
            }
        }

        pub fn height(&self) -> i32 {
            unsafe {
                let ptr = recursive_cast!(&*self.sys_handle $(, $cast)*);
                (*ptr).height()
            }
        }
    };
}

macro_rules! impl_yuv_buffer {
    ($($cast:ident),*) => {
        pub fn chroma_width(&self) -> i32 {
            unsafe {
                let ptr = recursive_cast!(&*self.sys_handle $(, $cast)*);
                (*ptr).chroma_width()
            }
        }

        pub fn chroma_height(&self) -> i32 {
            unsafe {
                let ptr = recursive_cast!(&*self.sys_handle $(, $cast)*);
                (*ptr).chroma_height()
            }
        }

        pub fn stride_y(&self) -> i32 {
            unsafe {
                let ptr = recursive_cast!(&*self.sys_handle $(, $cast)*);
                (*ptr).stride_y()
            }
        }

        pub fn stride_u(&self) -> i32 {
            unsafe {
                let ptr = recursive_cast!(&*self.sys_handle $(, $cast)*);
                (*ptr).stride_u()
            }
        }

        pub fn stride_v(&self) -> i32 {
            unsafe {
                let ptr = recursive_cast!(&*self.sys_handle $(, $cast)*);
                (*ptr).stride_v()
            }
        }
    };
}

macro_rules! impl_biyuv_buffer {
    ($($cast:ident),*) => {
        pub fn chroma_width(&self) -> i32 {
            unsafe {
                let ptr = recursive_cast!(&*self.sys_handle $(, $cast)*);
                (*ptr).chroma_width()
            }
        }

        pub fn chroma_height(&self) -> i32 {
            unsafe {
                let ptr = recursive_cast!(&*self.sys_handle $(, $cast)*);
                (*ptr).chroma_height()
            }
        }

        pub fn stride_y(&self) -> i32 {
            unsafe {
                let ptr = recursive_cast!(&*self.sys_handle $(, $cast)*);
                (*ptr).stride_y()
            }
        }

        pub fn stride_uv(&self) -> i32 {
            unsafe {
                let ptr = recursive_cast!(&*self.sys_handle $(, $cast)*);
                (*ptr).stride_uv()
            }
        }
    };
}

pub struct NativeBuffer {
    sys_handle: UniquePtr<vfb_sys::ffi::VideoFrameBuffer>,
}

impl NativeBuffer {
    impl_vfb_buffer!();

    pub fn to_i420(&self) -> I420Buffer {
        I420Buffer {
            sys_handle: unsafe { self.sys_handle.to_i420() },
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
        self.to_i420()
            .to_argb(format, dst, dst_stride, dst_width, dst_height)
    }
}

pub struct I420Buffer {
    sys_handle: UniquePtr<vfb_sys::ffi::I420Buffer>,
}

impl I420Buffer {
    impl_vfb_buffer!(i420_to_yuv8, yuv8_to_yuv, yuv_to_vfb);
    impl_yuv_buffer!(i420_to_yuv8, yuv8_to_yuv);

    pub fn to_i420(&self) -> I420Buffer {
        I420Buffer {
            sys_handle: unsafe {
                // We make a copy of the buffer because internally, when calling ToI420()
                // if the buffer is of type I420, libwebrtc will reuse the same underlying pointer
                // for the new created type
                let copy = vfb_sys::ffi::copy_i420_buffer(&self.sys_handle);
                let ptr = recursive_cast!(&*copy, i420_to_yuv8, yuv8_to_yuv, yuv_to_vfb);
                (*ptr).to_i420()
            },
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
        impl_to_argb!(
            I420Buffer
            [
                ARGB: i420_to_argb,
                BGRA: i420_to_bgra,
                ABGR: i420_to_abgr,
                RGBA: i420_to_rgba
            ],
            format, self, dst, dst_stride, dst_width, dst_height
        )
    }

    pub fn data_y(&self) -> &[u8] {
        unsafe {
            let ptr = recursive_cast!(&*self.sys_handle, i420_to_yuv8);
            slice::from_raw_parts((*ptr).data_y(), (self.stride_y() * self.height()) as usize)
        }
    }

    pub fn data_u(&self) -> &[u8] {
        unsafe {
            let ptr = recursive_cast!(&*self.sys_handle, i420_to_yuv8);
            let chroma_height = (self.height() + 1) / 2;
            slice::from_raw_parts((*ptr).data_u(), (self.stride_u() * chroma_height) as usize)
        }
    }

    pub fn data_v(&self) -> &[u8] {
        unsafe {
            let ptr = recursive_cast!(&*self.sys_handle, i420_to_yuv8);
            let chroma_height = (self.height() + 1) / 2;
            slice::from_raw_parts((*ptr).data_v(), (self.stride_v() * chroma_height) as usize)
        }
    }
}

pub struct I420ABuffer {
    sys_handle: UniquePtr<vfb_sys::ffi::I420ABuffer>,
}

impl I420ABuffer {
    impl_vfb_buffer!(i420a_to_yuv8, yuv8_to_yuv, yuv_to_vfb);
    impl_yuv_buffer!(i420a_to_yuv8, yuv8_to_yuv);

    pub fn stride_a(&self) -> i32 {
        self.sys_handle.stride_a()
    }

    pub fn data_a(&self) -> Option<&[u8]> {
        unsafe {
            let data_a = self.sys_handle.data_a();
            if data_a.is_null() {
                return None;
            }
            Some(slice::from_raw_parts(
                data_a,
                (self.stride_a() * self.height()) as usize,
            ))
        }
    }

    pub fn to_i420(&self) -> I420Buffer {
        I420Buffer {
            sys_handle: unsafe {
                let ptr =
                    recursive_cast!(&*self.sys_handle, i420a_to_yuv8, yuv8_to_yuv, yuv_to_vfb);
                (*ptr).to_i420()
            },
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
        self.to_i420()
            .to_argb(format, dst, dst_stride, dst_width, dst_height)
    }

    pub fn data_y(&self) -> &[u8] {
        unsafe {
            let ptr = recursive_cast!(&*self.sys_handle, i420a_to_yuv8);
            slice::from_raw_parts((*ptr).data_y(), (self.stride_y() * self.height()) as usize)
        }
    }

    pub fn data_u(&self) -> &[u8] {
        unsafe {
            let ptr = recursive_cast!(&*self.sys_handle, i420a_to_yuv8);
            let chroma_height = (self.height() + 1) / 2;
            slice::from_raw_parts((*ptr).data_u(), (self.stride_u() * chroma_height) as usize)
        }
    }

    pub fn data_v(&self) -> &[u8] {
        unsafe {
            let ptr = recursive_cast!(&*self.sys_handle, i420a_to_yuv8);
            let chroma_height = (self.height() + 1) / 2;
            slice::from_raw_parts((*ptr).data_v(), (self.stride_v() * chroma_height) as usize)
        }
    }
}

pub struct I422Buffer {
    sys_handle: UniquePtr<vfb_sys::ffi::I422Buffer>,
}

impl I422Buffer {
    impl_vfb_buffer!(i422_to_yuv8, yuv8_to_yuv, yuv_to_vfb);
    impl_yuv_buffer!(i422_to_yuv8, yuv8_to_yuv);

    pub fn to_i420(&self) -> I420Buffer {
        I420Buffer {
            sys_handle: unsafe {
                let ptr = recursive_cast!(&*self.sys_handle, i422_to_yuv8, yuv8_to_yuv, yuv_to_vfb);
                (*ptr).to_i420()
            },
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
        self.to_i420()
            .to_argb(format, dst, dst_stride, dst_width, dst_height)
    }

    pub fn data_y(&self) -> &[u8] {
        unsafe {
            let ptr = recursive_cast!(&*self.sys_handle, i422_to_yuv8);
            slice::from_raw_parts((*ptr).data_y(), (self.stride_y() * self.height()) as usize)
        }
    }

    pub fn data_u(&self) -> &[u8] {
        unsafe {
            let ptr = recursive_cast!(&*self.sys_handle, i422_to_yuv8);
            slice::from_raw_parts((*ptr).data_u(), (self.stride_u() * self.height()) as usize)
        }
    }

    pub fn data_v(&self) -> &[u8] {
        unsafe {
            let ptr = recursive_cast!(&*self.sys_handle, i422_to_yuv8);
            slice::from_raw_parts((*ptr).data_v(), (self.stride_v() * self.height()) as usize)
        }
    }
}

pub struct I444Buffer {
    sys_handle: UniquePtr<vfb_sys::ffi::I444Buffer>,
}

impl I444Buffer {
    impl_vfb_buffer!(i444_to_yuv8, yuv8_to_yuv, yuv_to_vfb);
    impl_yuv_buffer!(i444_to_yuv8, yuv8_to_yuv);

    pub fn to_i420(&self) -> I420Buffer {
        I420Buffer {
            sys_handle: unsafe {
                let ptr = recursive_cast!(&*self.sys_handle, i444_to_yuv8, yuv8_to_yuv, yuv_to_vfb);
                (*ptr).to_i420()
            },
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
        self.to_i420()
            .to_argb(format, dst, dst_stride, dst_width, dst_height)
    }

    pub fn data_y(&self) -> &[u8] {
        unsafe {
            let ptr = recursive_cast!(&*self.sys_handle, i444_to_yuv8);
            slice::from_raw_parts((*ptr).data_y(), (self.stride_y() * self.height()) as usize)
        }
    }

    pub fn data_u(&self) -> &[u8] {
        unsafe {
            let ptr = recursive_cast!(&*self.sys_handle, i444_to_yuv8);
            slice::from_raw_parts((*ptr).data_u(), (self.stride_u() * self.height()) as usize)
        }
    }

    pub fn data_v(&self) -> &[u8] {
        unsafe {
            let ptr = recursive_cast!(&*self.sys_handle, i444_to_yuv8);
            slice::from_raw_parts((*ptr).data_v(), (self.stride_v() * self.height()) as usize)
        }
    }
}

pub struct I010Buffer {
    sys_handle: UniquePtr<vfb_sys::ffi::I010Buffer>,
}

impl I010Buffer {
    impl_vfb_buffer!(i010_to_yuv16b, yuv16b_to_yuv, yuv_to_vfb);
    impl_yuv_buffer!(i010_to_yuv16b, yuv16b_to_yuv);

    pub fn to_i420(&self) -> I420Buffer {
        I420Buffer {
            sys_handle: unsafe {
                let ptr =
                    recursive_cast!(&*self.sys_handle, i010_to_yuv16b, yuv16b_to_yuv, yuv_to_vfb);
                (*ptr).to_i420()
            },
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
        self.to_i420()
            .to_argb(format, dst, dst_stride, dst_width, dst_height)
    }

    pub fn data_y(&self) -> &[u16] {
        unsafe {
            let ptr = recursive_cast!(&*self.sys_handle, i010_to_yuv16b);
            slice::from_raw_parts(
                (*ptr).data_y(),
                (self.stride_y() * self.height()) as usize / 2,
            )
        }
    }

    pub fn data_u(&self) -> &[u16] {
        unsafe {
            let ptr = recursive_cast!(&*self.sys_handle, i010_to_yuv16b);
            let chroma_height = (self.height() + 1) / 2;
            slice::from_raw_parts(
                (*ptr).data_u(),
                (self.stride_u() * chroma_height) as usize / 2,
            )
        }
    }

    pub fn data_v(&self) -> &[u16] {
        unsafe {
            let ptr = recursive_cast!(&*self.sys_handle, i010_to_yuv16b);
            let chroma_height = (self.height() + 1) / 2;
            slice::from_raw_parts(
                (*ptr).data_v(),
                (self.stride_v() * chroma_height) as usize / 2,
            )
        }
    }
}

pub struct NV12Buffer {
    sys_handle: UniquePtr<vfb_sys::ffi::NV12Buffer>,
}

impl NV12Buffer {
    impl_vfb_buffer!(nv12_to_biyuv8, biyuv8_to_biyuv, biyuv_to_vfb);
    impl_biyuv_buffer!(nv12_to_biyuv8, biyuv8_to_biyuv);

    pub fn to_i420(&self) -> I420Buffer {
        I420Buffer {
            sys_handle: unsafe {
                let ptr = recursive_cast!(
                    &*self.sys_handle,
                    nv12_to_biyuv8,
                    biyuv8_to_biyuv,
                    biyuv_to_vfb
                );
                (*ptr).to_i420()
            },
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
        self.to_i420()
            .to_argb(format, dst, dst_stride, dst_width, dst_height)
    }

    pub fn data_y(&self) -> &[u8] {
        unsafe {
            let ptr = recursive_cast!(&*self.sys_handle, nv12_to_biyuv8);
            slice::from_raw_parts((*ptr).data_y(), (self.stride_y() * self.height()) as usize)
        }
    }

    pub fn data_uv(&self) -> &[u8] {
        unsafe {
            let ptr = recursive_cast!(&*self.sys_handle, nv12_to_biyuv8);
            let chroma_height = (self.height() + 1) / 2;
            slice::from_raw_parts(
                (*ptr).data_uv(),
                (self.stride_uv() * chroma_height) as usize,
            )
        }
    }
}
