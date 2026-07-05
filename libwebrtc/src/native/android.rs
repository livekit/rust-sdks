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

use jni::objects::JObject;
use webrtc_sys::android::ffi as sys_android;

/// Initialize Android WebRTC with the JVM.
///
/// This is automatically called by [`initialize_android_context`], so you only
/// need to call this directly if you don't have access to an Android Context
/// (e.g., in `JNI_OnLoad`).
///
/// This function is idempotent - safe to call multiple times.
pub fn initialize_android(vm: &jni::JavaVM) {
    unsafe {
        sys_android::init_android(vm.get_java_vm_pointer() as *mut _);
    }
}

/// Initialize Android WebRTC with the application context.
///
/// This is the main initialization function for Android. It performs both:
/// 1. JVM initialization (same as [`initialize_android`])
/// 2. Context initialization (required for PlatformAudio)
///
/// This function is idempotent - safe to call multiple times.
///
/// # Arguments
/// * `vm` - The JavaVM instance
/// * `context` - The Android application context
///
/// # Returns
/// `true` if context initialization succeeded, `false` otherwise.
/// Note: JVM initialization always happens regardless of return value.
///
/// # Example
/// ```ignore
/// use jni::JavaVM;
/// use jni::objects::JObject;
/// use livekit::webrtc::android::initialize_android_context;
///
/// fn init(vm: JavaVM, context: JObject) {
///     // Just one call needed - handles both JVM and context init
///     initialize_android_context(&vm, &context);
/// }
/// ```
pub fn initialize_android_context(vm: &jni::JavaVM, context: &JObject) -> bool {
    unsafe {
        sys_android::init_android_context(
            vm.get_java_vm_pointer() as *mut _,
            context.as_raw() as usize,
        )
    }
}
