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

use std::{
    pin::Pin,
    task::{Context, Poll},
};

use futures_util::Stream;
use livekit::{
    webrtc::{audio_stream::native::NativeAudioStream, prelude::AudioFrame},
    AudioFilterAudioStream,
};

pub trait AudioStream: Stream<Item = AudioFrame<'static>> + Send + Sync + Unpin {
    fn close(&mut self);
}

pub enum AudioStreamKind {
    Native(NativeAudioStream),
    Filtered(AudioFilterAudioStream),
}

impl Stream for AudioStreamKind {
    type Item = AudioFrame<'static>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        match self.get_mut() {
            AudioStreamKind::Native(native_stream) => Pin::new(native_stream).poll_next(cx),
            AudioStreamKind::Filtered(duration_stream) => Pin::new(duration_stream).poll_next(cx),
        }
    }
}
