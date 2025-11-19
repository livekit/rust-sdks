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

//use livekit_protocol::enum_dispatch;

#[derive(Default, Debug)]
pub struct AudioSourceOptions {
    pub echo_cancellation: bool,
    pub noise_suppression: bool,
    pub auto_gain_control: bool,
}

#[non_exhaustive]
#[derive(Debug, Clone)]
pub enum RtcAudioSource {
    //#[cfg(not(target_arch = "wasm32"))]
    //Native(native::NativeAudioSource),
}

impl RtcAudioSource {
    /*enum_dispatch!(
        [Native];
        fn set_audio_options(self: &Self, options: AudioSourceOptions) -> ();
        fn audio_options(self: &Self) -> AudioSourceOptions;
        fn sample_rate(self: &Self) -> u32;
        fn num_channels(self: &Self) -> u32;
    );
    */
}

#[cfg(not(target_arch = "wasm32"))]
pub mod native {
    use std::{
        fmt::{Debug, Formatter},
        sync::Arc,
    };

    use tokio::sync::mpsc;

    use crate::{audio_frame::AudioFrame, sys, RtcError};
    /*
        #[derive(Clone)]
        pub struct NativeAudioSource {
            pub(crate) ffi: sys::RefCounted<sys::lkAudioSource>,
        }

        impl Debug for NativeAudioSource {
            fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
                f.debug_struct("NativeAudioSource").finish()
            }
        }

        impl NativeAudioSource {
            pub fn new(
                options: AudioSourceOptions,
                sample_rate: u32,
                num_channels: u32,
                queue_size_ms: u32,
            ) -> NativeAudioSource {
                Self {
                    ffi: imp_as::NativeAudioSource::new(
                        options,
                        sample_rate,
                        num_channels,
                        queue_size_ms,
                    ),
                }
            }

            pub fn clear_buffer(&self) {
                self.ffi.clear_buffer()
            }

            pub async fn capture_frame(&self, frame: &AudioFrame<'_>) -> Result<(), RtcError> {
                self.ffi.capture_frame(frame).await
            }

            pub fn set_audio_options(&self, options: AudioSourceOptions) {
                self.ffi.set_audio_options(options)
            }

            pub fn audio_options(&self) -> AudioSourceOptions {
                self.ffi.audio_options()
            }

            pub fn sample_rate(&self) -> u32 {
                self.ffi.sample_rate()
            }

            pub fn num_channels(&self) -> u32 {
                self.ffi.num_channels()
            }
        }
    */

    pub trait AudioSink: Send {
        fn on_data(&self, data: &[i16], sample_rate: i32, nb_channels: usize, nb_frames: usize);
    }

    pub struct AudioSinkWrapper {
        observer: Arc<dyn AudioSink>,
    }

    impl AudioSinkWrapper {
        pub fn new(observer: Arc<dyn AudioSink>) -> Self {
            Self { observer }
        }

        fn on_data(&self, data: &[i16], sample_rate: i32, nb_channels: usize, nb_frames: usize) {
            self.observer.on_data(data, sample_rate, nb_channels, nb_frames);
        }
    }

    pub struct NativeAudioSink {
        pub(crate) ffi: sys::RefCounted<sys::lkNativeAudioSink>,
        observer: Arc<AudioSinkWrapper>,
    }

    impl NativeAudioSink {
        pub fn new(
            audio_sink_wrapper: Arc<AudioSinkWrapper>,
            sample_rate: i32,
            number_of_channels: i32,
        ) -> Self {
            let observer = sys::lkNativeAudioSinkObserver {
                onAudioData: Some(NativeAudioSink::native_on_audio_data),
            };
            let audio_sink_box: *mut Arc<AudioSinkWrapper> =
                Box::into_raw(Box::new(audio_sink_wrapper.clone()));
            Self {
                observer: audio_sink_wrapper,
                ffi: unsafe {
                    let sink = sys::lkCreateNativeAudioSink(
                        &observer as *const _ as *mut _,
                        audio_sink_box as *mut ::std::os::raw::c_void,
                        sample_rate,
                        number_of_channels,
                    );
                    sys::RefCounted::from_raw(sink)
                },
            }
        }

        pub extern "C" fn native_on_audio_data(
            audio_data: *const i16,
            sample_rate: u32,
            number_of_channels: u32,
            number_of_frames: ::std::os::raw::c_int,
            userdata: *mut ::std::os::raw::c_void,
        ) {
            let audio_slice = unsafe {
                std::slice::from_raw_parts(
                    audio_data,
                    (number_of_frames as u32 * number_of_channels) as usize,
                )
            };
            let audio_sink_wrapper = unsafe { &*(userdata as *const Arc<AudioSinkWrapper>) };
            audio_sink_wrapper.on_data(
                audio_slice,
                sample_rate as i32,
                number_of_channels as usize,
                number_of_frames as usize,
            );
        }
    }

    impl Debug for NativeAudioSink {
        fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
            f.debug_struct("NativeAudioSink").finish()
        }
    }

    pub struct AudioTrackObserver {
        pub(crate) frame_tx: mpsc::UnboundedSender<AudioFrame<'static>>,
    }

    impl AudioSink for AudioTrackObserver {
        fn on_data(&self, data: &[i16], sample_rate: i32, nb_channels: usize, nb_frames: usize) {
            let _ = self.frame_tx.send(AudioFrame {
                data: data.to_owned().into(),
                sample_rate: sample_rate as u32,
                num_channels: nb_channels as u32,
                samples_per_channel: nb_frames as u32,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tokio::sync::mpsc;

    #[tokio::test]
    async fn create_audio_native_sink() {
        {
            let (frame_tx, mut frame_rx) = mpsc::unbounded_channel();
            let observer = Arc::new(super::native::AudioTrackObserver { frame_tx });
            let audio_sink_wrapper = Arc::new(super::native::AudioSinkWrapper::new(observer));
            let _sink = super::native::NativeAudioSink::new(
                audio_sink_wrapper,
                48000,
                2,
            );

           
           let audio_frame = frame_rx.recv().await;
        }
        println!("Created NativeAudioSink");
    }
}
