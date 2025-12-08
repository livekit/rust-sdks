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

#[cfg(not(target_arch = "wasm32"))]
pub mod native {
    use std::{
        fmt::{Debug, Formatter},
        pin::Pin,
        sync::Arc,
        task::{Context, Poll},
    };

    use livekit_runtime::Stream;
    use tokio::sync::mpsc;

    use crate::{
        audio_frame::AudioFrame,
        audio_source::native::{AudioSinkWrapper, AudioTrackObserver, NativeAudioSink},
        audio_track::RtcAudioTrack,
    };

    pub struct NativeAudioStream {
        native_sink: Arc<NativeAudioSink>,
        pub audio_track: RtcAudioTrack,
        pub frame_rx: mpsc::UnboundedReceiver<AudioFrame<'static>>,
    }

    impl Debug for NativeAudioStream {
        fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
            f.debug_struct("NativeAudioStream").field("track", &self.track()).finish()
        }
    }

    impl NativeAudioStream {
        pub fn new(audio_track: RtcAudioTrack, sample_rate: i32, num_channels: i32) -> Self {
            let (frame_tx, frame_rx) = mpsc::unbounded_channel();
            let observer = Arc::new(AudioTrackObserver { frame_tx });
            let native_sink = Arc::new(NativeAudioSink::new(
                Box::new(AudioSinkWrapper::new(observer.clone())).into(),
                sample_rate,
                num_channels,
            ));

            audio_track.add_sink(native_sink.clone());

            Self { native_sink, audio_track, frame_rx }
        }

        pub fn track(&self) -> RtcAudioTrack {
            self.audio_track.clone()
        }

        pub fn close(&mut self) {
            self.audio_track.remove_sink(self.native_sink.clone());
            self.frame_rx.close();
        }
    }

    impl Drop for NativeAudioStream {
        fn drop(&mut self) {
            self.close();
        }
    }

    impl Stream for NativeAudioStream {
        type Item = AudioFrame<'static>;

        fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
            self.frame_rx.poll_recv(cx)
        }
    }
}
