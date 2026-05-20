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

use prost::Message;
use server::FfiDataBuffer;
use std::ffi::CStr;
use std::os::raw::c_char;
use std::{panic, sync::Arc};

use crate::{
    proto,
    server::{self, FfiConfig},
    FfiError, FfiHandleId, FFI_SERVER, INVALID_HANDLE,
};

/// # SAFTEY: The "C" callback must be threadsafe and not block
pub type FfiCallbackFn = unsafe extern "C" fn(*const u8, usize);

/// # Safety
///
/// The foreign language must only provide valid pointers
#[no_mangle]
pub unsafe extern "C" fn livekit_ffi_initialize(
    cb: FfiCallbackFn,
    capture_logs: bool,
    sdk: *const c_char,
    sdk_version: *const c_char,
) {
    FFI_SERVER.setup(FfiConfig {
        callback_fn: Arc::new(move |event| {
            let data = event.encode_to_vec();
            cb(data.as_ptr(), data.len());
        }),
        capture_logs,
        sdk: CStr::from_ptr(sdk).to_string_lossy().into_owned(),
        sdk_version: CStr::from_ptr(sdk_version).to_string_lossy().into_owned(),
    });

    log::info!("initializing ffi server v{}", env!("CARGO_PKG_VERSION"));
}

/// # Safety
///
/// The foreign language must only provide valid pointers
#[no_mangle]
pub unsafe extern "C" fn livekit_ffi_request(
    data: *const u8,
    len: usize,
    res_ptr: *mut *const u8,
    res_len: *mut usize,
) -> FfiHandleId {
    let res = panic::catch_unwind(|| {
        let data = unsafe { std::slice::from_raw_parts(data, len) };
        let req = match proto::FfiRequest::decode(data) {
            Ok(req) => req,
            Err(err) => {
                log::error!("failed to decode request: {:?}", err);
                return INVALID_HANDLE;
            }
        };

        let res = match server::requests::handle_request(&FFI_SERVER, req.clone()) {
            Ok(res) => res,
            Err(err) => {
                log::error!("failed to handle request {:?}: {:?}", req, err);
                return INVALID_HANDLE;
            }
        }
        .encode_to_vec();

        unsafe {
            *res_ptr = res.as_ptr();
            *res_len = res.len();
        }

        let handle_id = FFI_SERVER.next_id();
        let ffi_data = FfiDataBuffer { handle: handle_id, data: Arc::new(res) };

        FFI_SERVER.store_handle(handle_id, ffi_data);
        handle_id
    });

    match res {
        Ok(handle_id) => handle_id,
        Err(err) => {
            log::error!("panic while handling request: {:?}", err);
            FFI_SERVER.send_panic(Box::new(FfiError::InvalidRequest(
                "panic while handling request".into(),
            )));
            INVALID_HANDLE
        }
    }
}

#[no_mangle]
pub extern "C" fn livekit_ffi_drop_handle(handle_id: FfiHandleId) -> bool {
    FFI_SERVER.drop_handle(handle_id)
}

#[no_mangle]
pub extern "C" fn livekit_ffi_dispose() {
    FFI_SERVER.async_runtime.block_on(FFI_SERVER.dispose());
}

#[cfg(target_os = "android")]
pub mod android {
    use jni::{
        objects::{JObject, JValue},
        sys::{jint, jobject, JNI_VERSION_1_6},
        JavaVM,
    };
    use std::os::raw::c_void;
    use std::sync::atomic::{AtomicBool, Ordering};

    /// Track whether Android WebRTC has been initialized to prevent double-initialization.
    /// WebRTC's InitAndroid() will crash if called twice (g_jvm check fails).
    static ANDROID_INITIALIZED: AtomicBool = AtomicBool::new(false);

    /// Track whether ContextUtils has been initialized.
    static CONTEXT_INITIALIZED: AtomicBool = AtomicBool::new(false);

    /// Internal function to initialize Android WebRTC. Returns true if initialization
    /// was performed, false if already initialized.
    fn do_android_init(vm: &JavaVM) -> bool {
        // Use compare_exchange to ensure only one thread can initialize
        if ANDROID_INITIALIZED
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            livekit::webrtc::android::initialize_android(vm);
            device_info::android::init_vm(vm);
            true
        } else {
            false
        }
    }

    /// Initialize WebRTC ContextUtils with the Android application context.
    /// This is required for Android audio (microphone/speaker) to work.
    ///
    /// # Arguments
    /// * `vm` - The JavaVM instance
    /// * `context` - The Android application context (must be a global reference or
    ///               guaranteed to be valid for the duration of the call)
    ///
    /// # Returns
    /// true if initialization was successful, false otherwise
    fn do_context_init(vm: &JavaVM, context: JObject) -> bool {
        // Only initialize once
        if CONTEXT_INITIALIZED
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return true; // Already initialized
        }

        let mut env = match vm.attach_current_thread() {
            Ok(env) => env,
            Err(e) => {
                log::error!("Failed to attach thread to JVM: {:?}", e);
                CONTEXT_INITIALIZED.store(false, Ordering::SeqCst);
                return false;
            }
        };

        // Call livekit.org.webrtc.ContextUtils.initialize(context)
        // The class is prefixed with "livekit" because WebRTC JNI is built with
        // android_package_prefix="livekit" to avoid conflicts with other WebRTC builds.
        let class_name = "livekit/org/webrtc/ContextUtils";
        let class = match env.find_class(class_name) {
            Ok(c) => c,
            Err(e) => {
                log::error!("Failed to find class {}: {:?}", class_name, e);
                CONTEXT_INITIALIZED.store(false, Ordering::SeqCst);
                return false;
            }
        };

        match env.call_static_method(
            class,
            "initialize",
            "(Landroid/content/Context;)V",
            &[JValue::Object(&context)],
        ) {
            Ok(_) => {
                log::info!("Android WebRTC ContextUtils initialized successfully");
                true
            }
            Err(e) => {
                log::error!("Failed to call ContextUtils.initialize: {:?}", e);
                CONTEXT_INITIALIZED.store(false, Ordering::SeqCst);
                false
            }
        }
    }

    #[allow(non_snake_case)]
    #[no_mangle]
    pub extern "C" fn JNI_OnLoad(vm: JavaVM, _: *mut c_void) -> jint {
        do_android_init(&vm);
        JNI_VERSION_1_6
    }

    // TODO(CLT-xxxx): Add JNI entry point for C++/Python SDKs when they support Android.
    // This would allow Java/Kotlin code to call:
    //   System.loadLibrary("livekit_ffi");
    //   LiveKitFfi.initializeContext(getApplicationContext());
    //
    // The native method would be:
    //   #[allow(non_snake_case)]
    //   #[no_mangle]
    //   pub extern "C" fn Java_livekit_ffi_LiveKitFfi_initializeContext(...)

    /// Initialize Android WebRTC with the application context.
    /// This is required for Android audio (microphone/speaker) to work.
    ///
    /// This function performs two initializations:
    /// 1. JVM initialization (WebRTC's InitAndroid)
    /// 2. ContextUtils initialization with the application context
    ///
    /// # Safety
    /// * `vm_ptr` must be a valid pointer to a JavaVM
    /// * `context_ptr` must be a valid jobject pointing to an Android Context
    ///
    /// # Arguments
    /// * `vm_ptr` - Pointer to the JavaVM
    /// * `context_ptr` - The Android application context (jobject)
    ///
    /// # Returns
    /// true if context initialization was successful, false otherwise.
    /// Note: JVM initialization always happens regardless of return value.
    #[no_mangle]
    pub unsafe extern "C" fn livekit_ffi_initialize_android_context(
        vm_ptr: *mut c_void,
        context_ptr: jobject,
    ) -> bool {
        if vm_ptr.is_null() || context_ptr.is_null() {
            log::error!("livekit_ffi_initialize_android_context: null pointer provided");
            return false;
        }

        // Safety: vm_ptr must be a valid JavaVM pointer
        let vm = match JavaVM::from_raw(vm_ptr as *mut _) {
            Ok(vm) => vm,
            Err(e) => {
                log::error!("Failed to get JavaVM from pointer: {:?}", e);
                return false;
            }
        };

        // Initialize the JVM first
        do_android_init(&vm);

        // Initialize ContextUtils with the application context
        // Safety: context_ptr is guaranteed to be valid by the caller
        let context = JObject::from_raw(context_ptr);
        do_context_init(&vm, context)
    }
}
