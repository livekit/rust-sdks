use webrtc_sys::android::ffi as sys_android;

pub fn initialize_android(vm: &jni::JavaVM) {
    unsafe {
        sys_android::init_android(vm.get_java_vm_pointer() as *mut _);
    }
}
