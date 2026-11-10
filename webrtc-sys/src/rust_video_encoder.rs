// Copyright 2026 LiveKit, Inc.
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

//! Bridges livekit-hwcodec to the C++ adapter. No `Result`s: `cxx` would throw
//! them as C++ exceptions into WebRTC.

use std::{
    panic::{self, AssertUnwindSafe},
    path::Path,
};

use cxx::{SharedPtr, UniquePtr};
use livekit_hwcodec::{
    Codec, EncodeStatus, EncoderConfig, Frame, FrameMeta, Plane, Sink, V4l2Encoder,
    V4l2EncoderFactory, I420,
};

use crate::impl_thread_safety;

#[cxx::bridge(namespace = "livekit_ffi")]
mod ffi {
    struct EncoderInit {
        width: u32,
        height: u32,
        bitrate_bps: u32,
        framerate: u32,
    }

    struct FrameInfo {
        rtp_timestamp: u32,
        capture_time_ms: i64,
    }

    #[repr(i32)]
    enum CodecKind {
        H264,
    }

    #[repr(i32)]
    enum EncodeStatus {
        Queued,
        Dropped,
        Failed,
    }

    unsafe extern "C++" {
        include!("livekit/rust_video_encoder.h");

        type VideoFrameRef;
        type I420View;
        fn to_i420(self: &VideoFrameRef) -> UniquePtr<I420View>;
        fn width(self: &I420View) -> u32;
        fn height(self: &I420View) -> u32;
        fn plane(self: &I420View, index: usize) -> &[u8];
        fn stride(self: &I420View, index: usize) -> u32;

        type EncodedSink;
        fn deliver(self: &EncodedSink, data: &[u8], info: &FrameInfo, keyframe: bool);
        fn dropped(self: &EncodedSink, info: &FrameInfo);
    }

    extern "Rust" {
        type RustEncoderFactory;
        type RustVideoEncoder;

        fn new_v4l2_encoder_factory() -> Box<RustEncoderFactory>;
        fn new_v4l2_encoder_factory_for_test(device: &str) -> Box<RustEncoderFactory>;
        fn supports(self: &RustEncoderFactory, codec: CodecKind) -> bool;
        fn create_encoder(self: &RustEncoderFactory, codec: CodecKind) -> Box<RustVideoEncoder>;

        fn usable(self: &RustVideoEncoder) -> bool;
        fn implementation_name(self: &RustVideoEncoder) -> String;
        fn init(
            self: &mut RustVideoEncoder,
            init: &EncoderInit,
            sink: SharedPtr<EncodedSink>,
        ) -> bool;
        fn encode(
            self: &mut RustVideoEncoder,
            frame: UniquePtr<VideoFrameRef>,
            info: &FrameInfo,
            keyframe: bool,
        ) -> EncodeStatus;
        fn set_rates(self: &RustVideoEncoder, bitrate_bps: u32, framerate: u32);
        fn release(self: &mut RustVideoEncoder);
    }
}

// Frame buffers are immutable and atomically refcounted; the sink is mutex-guarded.
impl_thread_safety!(ffi::VideoFrameRef, Send + Sync);
impl_thread_safety!(ffi::EncodedSink, Send + Sync);

/// Unwinding into C++ would abort, so a panic is logged and becomes `fallback`,
/// which the adapter treats as failure.
fn guard<T>(call: &str, fallback: T, f: impl FnOnce() -> T) -> T {
    panic::catch_unwind(AssertUnwindSafe(f)).unwrap_or_else(|panic| {
        let message = panic
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| panic.downcast_ref::<String>().map(String::as_str))
            .unwrap_or("unknown");
        log::error!("hardware encoder panicked in {call}: {message}");
        fallback
    })
}

fn codec(kind: ffi::CodecKind) -> Option<Codec> {
    match kind {
        ffi::CodecKind::H264 => Some(Codec::H264),
        _ => None,
    }
}

struct FrameRef(UniquePtr<ffi::VideoFrameRef>);

impl Frame for FrameRef {
    fn with_i420<R>(&self, f: impl FnOnce(I420<'_>) -> R) -> Option<R> {
        let view = self.0.as_ref()?.to_i420();
        let view = view.as_ref()?;
        let plane = |index| Plane { data: view.plane(index), stride: view.stride(index) as usize };
        Some(f(I420 {
            width: view.width(),
            height: view.height(),
            y: plane(0),
            u: plane(1),
            v: plane(2),
        }))
    }
}

impl From<&FrameMeta> for ffi::FrameInfo {
    fn from(meta: &FrameMeta) -> Self {
        Self { rtp_timestamp: meta.rtp_timestamp, capture_time_ms: meta.capture_time_ms }
    }
}

impl From<&ffi::FrameInfo> for FrameMeta {
    fn from(info: &ffi::FrameInfo) -> Self {
        Self { rtp_timestamp: info.rtp_timestamp, capture_time_ms: info.capture_time_ms }
    }
}

impl From<&ffi::EncoderInit> for EncoderConfig {
    fn from(init: &ffi::EncoderInit) -> Self {
        Self {
            width: init.width,
            height: init.height,
            bitrate_bps: init.bitrate_bps,
            framerate: init.framerate,
        }
    }
}

impl From<EncodeStatus> for ffi::EncodeStatus {
    fn from(status: EncodeStatus) -> Self {
        match status {
            EncodeStatus::Queued => Self::Queued,
            EncodeStatus::Dropped => Self::Dropped,
            EncodeStatus::Failed => Self::Failed,
        }
    }
}

struct SinkRef(SharedPtr<ffi::EncodedSink>);

impl Sink for SinkRef {
    fn deliver(&self, data: &[u8], meta: &FrameMeta, keyframe: bool) {
        if let Some(sink) = self.0.as_ref() {
            sink.deliver(data, &meta.into(), keyframe);
        }
    }

    fn dropped(&self, meta: &FrameMeta) {
        if let Some(sink) = self.0.as_ref() {
            sink.dropped(&meta.into());
        }
    }
}

// With a second backend, these two become enums over the backends.

/// `None` if probing panicked.
struct RustEncoderFactory(Option<V4l2EncoderFactory>);

/// `None` if there was no usable device.
struct RustVideoEncoder(Option<V4l2Encoder<FrameRef>>);

fn new_v4l2_encoder_factory() -> Box<RustEncoderFactory> {
    Box::new(RustEncoderFactory(guard("probing", None, || Some(V4l2EncoderFactory::probe()))))
}

fn new_v4l2_encoder_factory_for_test(device: &str) -> Box<RustEncoderFactory> {
    Box::new(RustEncoderFactory(Some(V4l2EncoderFactory::with_device(Path::new(device)))))
}

impl RustEncoderFactory {
    fn supports(&self, kind: ffi::CodecKind) -> bool {
        let Some((factory, codec)) = self.0.as_ref().zip(codec(kind)) else {
            return false;
        };
        factory.codecs().contains(&codec)
    }

    fn create_encoder(&self, kind: ffi::CodecKind) -> Box<RustVideoEncoder> {
        let encoder =
            self.0.as_ref().zip(codec(kind)).and_then(|(factory, codec)| factory.create(codec));
        Box::new(RustVideoEncoder(encoder))
    }
}

impl RustVideoEncoder {
    fn usable(&self) -> bool {
        self.0.is_some()
    }

    fn implementation_name(&self) -> String {
        match &self.0 {
            Some(encoder) => format!("V4L2 ({})", encoder.driver()),
            None => String::new(),
        }
    }

    fn init(&mut self, init: &ffi::EncoderInit, sink: SharedPtr<ffi::EncodedSink>) -> bool {
        let config = EncoderConfig::from(init);
        guard("init", false, || {
            self.0.as_mut().is_some_and(|encoder| encoder.init(config, SinkRef(sink)))
        })
    }

    fn encode(
        &mut self,
        frame: UniquePtr<ffi::VideoFrameRef>,
        info: &ffi::FrameInfo,
        keyframe: bool,
    ) -> ffi::EncodeStatus {
        let meta = FrameMeta::from(info);
        guard("encode", EncodeStatus::Failed, || match &mut self.0 {
            Some(encoder) => encoder.encode(FrameRef(frame), meta, keyframe),
            None => EncodeStatus::Failed,
        })
        .into()
    }

    fn set_rates(&self, bitrate_bps: u32, framerate: u32) {
        guard("set_rates", (), || {
            if let Some(encoder) = &self.0 {
                encoder.set_rates(bitrate_bps, framerate);
            }
        });
    }

    fn release(&mut self) {
        guard("release", (), || {
            if let Some(encoder) = &mut self.0 {
                encoder.release();
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        sync::{Mutex, PoisonError},
    };

    #[cxx::bridge(namespace = "livekit_ffi")]
    mod ffi_tests {
        unsafe extern "C++" {
            include!("livekit/rust_video_encoder.h");

            fn rust_encoder_fallback_for_test() -> Vec<String>;
            fn rust_encoder_lifecycle_for_test(cycles: u32);
            fn encoded_sink_detach_for_test() -> u32;
        }
    }

    /// The lifecycle test counts process-wide threads.
    static ENCODERS: Mutex<()> = Mutex::new(());

    fn encoder_resources() -> (usize, usize) {
        let threads = fs::read_dir("/proc/self/task")
            .unwrap()
            .filter_map(|task| fs::read_to_string(task.ok()?.path().join("comm")).ok())
            .filter(|name| name.trim() == "lk-v4l2-encoder")
            .count();
        let eventfds = fs::read_dir("/proc/self/fd")
            .unwrap()
            .filter_map(|fd| fs::read_link(fd.ok()?.path()).ok())
            .filter(|target| target.as_os_str() == "anon_inode:[eventfd]")
            .count();
        (threads, eventfds)
    }

    #[test]
    fn failed_hardware_encoder_falls_back_to_software() {
        let _encoders = ENCODERS.lock().unwrap_or_else(PoisonError::into_inner);
        let implementations = ffi_tests::rust_encoder_fallback_for_test();
        assert_eq!(implementations, ["OpenH264", "OpenH264", "OpenH264"]);
    }

    #[test]
    fn encoder_lifetimes_leave_nothing_behind() {
        let _encoders = ENCODERS.lock().unwrap_or_else(PoisonError::into_inner);
        let baseline = encoder_resources();
        ffi_tests::rust_encoder_lifecycle_for_test(30);
        assert_eq!(encoder_resources(), baseline);
    }

    #[test]
    fn detached_sink_delivers_nothing() {
        assert_eq!(ffi_tests::encoded_sink_detach_for_test(), 1);
    }
}
