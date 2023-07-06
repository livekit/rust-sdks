#[cxx::bridge(namespace = "livekit")]
pub mod ffi {
    unsafe extern "C++" {
        include!("livekit/android.h");

        type JavaVM;
        unsafe fn init_android(vm: *mut JavaVM);
    }
}
