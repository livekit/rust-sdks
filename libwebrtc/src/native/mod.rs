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

pub mod apm;
pub mod audio_resampler;
pub mod frame_cryptor;
pub mod yuv_helper;

pub use apm::*;
pub use audio_resampler::*;
pub use frame_cryptor::*;
pub use yuv_helper::*;

#[cfg(not(target_arch = "wasm32"))]
pub fn create_random_uuid() -> String {
    use uuid::Uuid;
    Uuid::new_v4().to_string()
}
