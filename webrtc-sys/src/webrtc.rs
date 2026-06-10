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

use crate::impl_thread_safety;

#[cxx::bridge(namespace = "livekit_ffi")]
pub mod ffi {
    #[derive(Debug)]
    #[repr(i32)]
    pub enum MediaType {
        Audio,
        Video,
        Data,
        Unsupported,
    }

    #[derive(Debug)]
    #[repr(i32)]
    pub enum Priority {
        VeryLow,
        Low,
        Medium,
        High,
    }

    #[derive(Debug)]
    #[repr(i32)]
    pub enum RtpTransceiverDirection {
        SendRecv,
        SendOnly,
        RecvOnly,
        Inactive,
        Stopped,
    }

    #[derive(Debug)]
    #[repr(i32)]
    pub enum LoggingSeverity {
        Verbose,
        Info,
        Warning,
        Error,
        None,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    #[repr(i32)]
    pub enum VideoEncoderBackend {
        Auto,
        Software,
        Hardware,
        Nvenc,
        Vaapi,
        VideoToolbox,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    #[repr(i32)]
    pub enum FecMaskType {
        Random,
        Bursty,
    }

    #[derive(Debug, Clone, Copy)]
    pub struct FecOverrideConfig {
        pub has_fec_rate: bool,
        pub fec_rate: u8, // 0-255 (255 ~= 100% protection overhead)
        pub has_mask_type: bool,
        pub mask_type: FecMaskType,
        pub has_max_frames: bool,
        pub max_frames: u32,
    }

    unsafe extern "C++" {
        include!("livekit/webrtc.h");

        type LogSink;

        fn create_random_uuid() -> String;
        fn video_encoder_backend_list() -> Vec<VideoEncoderBackend>;
        fn new_log_sink(fnc: fn(String, LoggingSeverity)) -> UniquePtr<LogSink>;
        fn init_field_trials(trials: String) -> bool;
        fn set_fec_override_config(config: FecOverrideConfig);
    }
}

impl_thread_safety!(ffi::LogSink, Send + Sync);
