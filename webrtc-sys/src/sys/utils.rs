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
            sys::lkStringGetData(self.ffi.as_ptr(), buf.as_mut_ptr() as *mut i8, len);
            let cstr = std::ffi::CStr::from_ptr(buf.as_ptr() as *const i8);
            cstr.to_string_lossy().into_owned()
        }
    }
}

pub struct RefCountedVector {
    pub ffi: sys::RefCounted<sys::lkVectorGeneric>,
    pub vec: Vec<crate::sys::RefCounted<sys::lkRefCountedObject>>,
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
