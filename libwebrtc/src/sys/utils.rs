// Copyright 2025 LiveKit, Inc.
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

use crate::sys;

pub struct RefCountedString {
    pub ffi: sys::RefCounted<sys::lkString>,
}

impl RefCountedString {
    pub fn from_native(vec_ptr: *mut sys::lkString) -> Self {
        let ffi = unsafe { sys::RefCounted::from_raw(vec_ptr) };
        Self { ffi }
    }

    pub fn new(s: &str) -> Self {
        let c_string = std::ffi::CString::new(s).unwrap();
        let ffi = unsafe { sys::lkCreateString(c_string.as_ptr()) };
        Self { ffi: unsafe { sys::RefCounted::from_raw(ffi) } }
    }

    pub fn as_str(&self) -> String {
        unsafe {
            let len = sys::lkStringGetLength(self.ffi.as_ptr());
            let mut buf = vec![0u8; len as usize + 1];
            sys::lkStringGetData(self.ffi.as_ptr(), buf.as_mut_ptr() as *mut ::std::os::raw::c_char, len);
            let cstr = std::ffi::CStr::from_ptr(buf.as_ptr() as *const ::std::os::raw::c_char);
            cstr.to_string_lossy().into_owned()
        }
    }
}

pub struct RefCountedData {
    pub ffi: sys::RefCounted<sys::lkData>,
}

impl RefCountedData {
    pub fn from_native(vec_ptr: *mut sys::lkData) -> Self {
        let ffi = unsafe { sys::RefCounted::from_raw(vec_ptr) };
        Self { ffi }
    }

    pub fn new(vec: &[u8]) -> Self {
        let ffi = unsafe {
            sys::lkCreateData(vec.as_ptr(), (vec.len() as u64).try_into().unwrap())
        };
        Self { ffi: unsafe { sys::RefCounted::from_raw(ffi) } }
    }

    pub fn as_bytes(&self) -> Vec<u8> {
        unsafe {
            let len = sys::lkDataGetSize(self.ffi.as_ptr());
            let mut buf = vec![0u8; len as usize];
            let buf_ptr = sys::lkDataGetData(self.ffi.as_ptr());
            std::ptr::copy_nonoverlapping(buf_ptr, buf.as_mut_ptr(), len as usize);
            buf
        }
    }

    pub fn as_slice(&self) -> &[i16] {
        unsafe {
            let len = sys::lkDataGetSize(self.ffi.as_ptr()) as usize;
            let data_ptr = sys::lkDataGetData(self.ffi.as_ptr());
            std::slice::from_raw_parts(data_ptr as *const i16, len / 2)
        }
    }

    pub fn as_ptr(&self) -> *const u8 {
        unsafe { sys::lkDataGetData(self.ffi.as_ptr()) }
    }

    pub fn len(&self) -> usize {
        unsafe { sys::lkDataGetSize(self.ffi.as_ptr()) as usize }
    }
}

pub struct RefCountedVector {
    pub ffi: sys::RefCounted<sys::lkVectorGeneric>,
    pub vec: Vec<crate::sys::RefCounted<sys::lkRefCountedObject>>,
}

impl Default for RefCountedVector {
    fn default() -> Self {
        Self::new()
    }
}

impl RefCountedVector {
    pub fn new() -> Self {
        let ffi = unsafe { sys::lkCreateVectorGeneric() };
        Self { ffi: unsafe { sys::RefCounted::from_raw(ffi) }, vec: Vec::new() }
    }

    pub fn push_back(&mut self, item: crate::sys::RefCounted<sys::lkRefCountedObject>) {
        unsafe {
            sys::lkVectorGenericPushBack(self.ffi.as_ptr(), item.as_ptr() as *mut _);
        }
        self.vec.push(item);
    }

    pub fn from_native_vec(vec_ptr: *mut sys::lkVectorGeneric) -> Self {
        let ffi = unsafe { sys::RefCounted::from_raw(vec_ptr) };

        let size = unsafe { sys::lkVectorGenericGetSize(ffi.as_ptr()) as usize };
        let mut vec = Vec::with_capacity(size);

        for i in 0..size {
            let element_ptr = unsafe {
                sys::lkVectorGenericGetAt(ffi.as_ptr(), i as u32) as *mut sys::lkRefCountedObject
            };
            // sys::lkAddRef to increase the reference count, because ffi in native owns the vector.
            unsafe { sys::lkAddRef(element_ptr as *mut _) }
            vec.push(unsafe { sys::RefCounted::from_raw(element_ptr) });
        }

        Self { ffi, vec }
    }
}
