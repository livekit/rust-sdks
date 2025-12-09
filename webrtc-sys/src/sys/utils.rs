use crate::sys;

pub struct RefCountedString {
    pub ffi: sys::RefCounted<sys::lkString>,
}

impl RefCountedString {
    pub fn new(s: &str) -> Self {
        let c_string = std::ffi::CString::new(s).unwrap();
        let ffi = unsafe { sys::lkCreateString(c_string.as_ptr()) };
        Self { ffi: unsafe { sys::RefCounted::from_raw(ffi) } }
    }

    pub fn as_str(&self) -> String {
        unsafe {
            let len = sys::lkStringGetLength(self.ffi.as_ptr());
            let mut buf = vec![0u8; len as usize + 1];
            sys::lkStringGetData(self.ffi.as_ptr(), buf.as_mut_ptr() as *mut i8, len);
            let cstr = std::ffi::CStr::from_ptr(buf.as_ptr() as *const i8);
            cstr.to_string_lossy().into_owned()
        }
    }
}

pub struct RefCountedVector<T> {
    pub ffi: sys::RefCounted<sys::lkVectorGeneric>,
    pub vec: Vec<T>,
}

impl<T> RefCountedVector<T> {
    pub fn new() -> Self {
        let ffi = unsafe { sys::lkCreateVectorGeneric() };
        Self { ffi: unsafe { sys::RefCounted::from_raw(ffi) }, vec: Vec::new() }
    }

    pub fn from_vec(vec: Vec<T>) -> Self {
        let ffi = unsafe { sys::lkCreateVectorGeneric() };
        let rc_vector = Self { ffi: unsafe { sys::RefCounted::from_raw(ffi) }, vec };
        for element in &rc_vector.vec {
            let element_ptr = element as *const T as *mut sys::lkRefCountedObject;
            unsafe {
                sys::lkVectorGenericPushBack(
                    rc_vector.ffi.as_ptr(),
                    element_ptr as *mut std::ffi::c_void,
                );
            }
        }
        rc_vector
    }

    pub fn from_native_vec(vec_ptr: *mut sys::lkVectorGeneric) -> Self {
        let ffi = unsafe { sys::RefCounted::from_raw(vec_ptr) };

        let size = unsafe { sys::lkVectorGenericGetSize(ffi.as_ptr()) as usize };
        let mut vec = Vec::with_capacity(size);

        for i in 0..size {
            let element_ptr = unsafe {
                sys::lkVectorGenericGetAt(ffi.as_ptr(), i as u32)
                    as *mut sys::lkRefCountedObject
            };
            // SAFETY: We are assuming that T can be safely constructed from a raw pointer.
            let element = unsafe { std::mem::transmute_copy::<*mut sys::lkRefCountedObject, T>(&element_ptr) };
            vec.push(element);
        }

        Self { ffi, vec }
    }
}
