use std::fmt::Debug;

use crate::sys;

#[repr(transparent)]
pub struct RefCounted<T> {
    ptr: *mut T,
}

impl<T> RefCounted<T> {
    /// # Safety
    /// The ptr must be owned and implement rtc::RefCountInterface
    pub unsafe fn from_raw(owned_ptr: *mut T) -> Self {
        sys::lkAddRef(owned_ptr as *mut _);
        RefCounted { ptr: owned_ptr }
    }

    pub fn as_ptr(&self) -> *mut T {
        self.ptr
    }

    pub fn is_null(&self) -> bool {
        self.ptr.is_null()
    }
}

impl<T> Clone for RefCounted<T> {
    fn clone(&self) -> Self {
        // increase refcount
        unsafe {
            if !self.is_null() {
                sys::lkAddRef(self.ptr as *mut _);
            }

            Self::from_raw(self.ptr)
        }
    }
}

impl<T> Drop for RefCounted<T> {
    fn drop(&mut self) {
        unsafe {
            if !self.is_null() {
                sys::lkReleaseRef(self.ptr as *mut _);
            }
        }
    }
}

impl<T> Debug for RefCounted<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let type_name = std::any::type_name::<T>();
        write!(f, "RefCounted<{}>({:p})", type_name, self.ptr)
    }
}
