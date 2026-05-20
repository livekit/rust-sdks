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

#[cfg(target_os = "android")]
#[cxx::bridge(namespace = "livekit_ffi")]
pub mod ffi {
    unsafe extern "C++" {
        include!("livekit/android.h");

        type JavaVM;

        /// Initialize Android WebRTC with the JVM.
        /// This must be called before any WebRTC operations on Android.
        unsafe fn init_android(vm: *mut JavaVM);

        /// Initialize the Android application context for WebRTC audio.
        /// This must be called before using PlatformAudio on Android.
        ///
        /// # Arguments
        /// * `jvm` - The JavaVM pointer
        /// * `context` - The Android application context (jobject as usize)
        ///
        /// # Returns
        /// true if initialization was successful, false otherwise
        unsafe fn init_android_context(jvm: *mut JavaVM, context: usize) -> bool;
    }
}
