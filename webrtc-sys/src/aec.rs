// Copyright 2023 LiveKit, Inc.
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

use crate::impl_thread_safety;

#[cxx::bridge(namespace = "livekit")]
pub mod ffi {
    unsafe extern "C++" {
        include!("livekit/aec.h");

        type Aec;

        fn create_aec(sample_rate: i32, num_channels: i32) -> UniquePtr<Aec>;

        unsafe fn cancel_echo(
            self: Pin<&mut Aec>,
            capture: *mut i16,
            capture_len: usize,
            render: *const i16,
            render_len: usize,
        );
    }
}


impl_thread_safety!(ffi::Aec, Send + Sync);