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
/// This must be called before any WebRTC operations on Android.
pub fn initialize_android(vm: &jni::JavaVM) {
    unsafe {
        sys_android::init_android(vm.get_java_vm_pointer() as *mut _);
    }
}

/// Initialize the Android application context for WebRTC audio.
/// This must be called before using PlatformAudio on Android.
///
/// # Arguments
/// * `vm` - The JavaVM instance
/// * `context` - The Android application context
///
/// # Returns
/// true if initialization was successful, false otherwise
///
/// # Example
/// ```ignore
/// use jni::JavaVM;
/// use livekit::webrtc::android::{initialize_android, initialize_android_context};
///
/// // In JNI_OnLoad or similar:
/// fn init(vm: JavaVM, context: JObject) {
///     initialize_android(&vm);
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
