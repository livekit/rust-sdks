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

use crate::sys::lkAudioSourceOptions;

#[derive(Default, Debug)]
pub struct AudioSourceOptions {
    pub echo_cancellation: bool,
    pub noise_suppression: bool,
    pub auto_gain_control: bool,
}

impl From<AudioSourceOptions> for lkAudioSourceOptions {
    fn from(options: AudioSourceOptions) -> Self {
        lkAudioSourceOptions {
            echoCancellation: options.echo_cancellation,
            noiseSuppression: options.noise_suppression,
            autoGainControl: options.auto_gain_control,
        }
    }
}

impl From<lkAudioSourceOptions> for AudioSourceOptions {
    fn from(ffi_options: lkAudioSourceOptions) -> Self {
        AudioSourceOptions {
            echo_cancellation: ffi_options.echoCancellation,
            noise_suppression: ffi_options.noiseSuppression,
            auto_gain_control: ffi_options.autoGainControl,
        }
    }
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

    use crate::{
        audio_frame::AudioFrame, audio_source::AudioSourceOptions, sys, RtcError, RtcErrorType,
    };

    #[derive(Clone)]
    pub struct NativeAudioSource {
        pub(crate) ffi: sys::RefCounted<sys::lkAudioTrackSource>,
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
            let ffi = unsafe {
                sys::RefCounted::from_raw(sys::lkCreateAudioTrackSource(
                    options.into(),
                    sample_rate as i32,
                    num_channels as i32,
                    queue_size_ms as i32,
                ))
            };
            Self { ffi }
        }

        pub fn add_sink(&self, sink: &sys::RefCounted<sys::lkNativeAudioSink>) {
            unsafe {
                sys::lkAudioTrackSourceAddSink(self.ffi.as_ptr(), sink.as_ptr());
            }
        }

        pub fn remove_sink(&self, sink: &sys::RefCounted<sys::lkNativeAudioSink>) {
            unsafe {
                sys::lkAudioTrackSourceRemoveSink(self.ffi.as_ptr(), sink.as_ptr());
            }
        }

        pub fn clear_buffer(&self) {
            unsafe {
                sys::lkAudioTrackSourceClearBuffer(self.ffi.as_ptr());
            }
        }

        pub async fn capture_frame(&self, frame: &AudioFrame<'_>) -> Result<(), RtcError> {
            let (tx, mut rx) = mpsc::channel::<Result<(), RtcError>>(1);
            let tx_box = Box::new(tx.clone());
            let userdata = Box::into_raw(tx_box) as *mut std::ffi::c_void;

            unsafe extern "C" fn on_complete(userdata: *mut ::std::os::raw::c_void) {
                let tx: Box<mpsc::Sender<Result<(), RtcError>>> = Box::from_raw(userdata as *mut _);
                let _ = tx.blocking_send(Ok(()));
            }

            unsafe {
                sys::lkAudioTrackSourceCaptureFrame(
                    self.ffi.as_ptr(),
                    frame.data.as_ptr() as *const i16,
                    frame.sample_rate,
                    frame.num_channels,
                    frame.samples_per_channel as i32,
                    userdata,
                    Some(on_complete),
                );
            }

            rx.recv().await.unwrap()
        }

        pub fn set_audio_options(&self, options: AudioSourceOptions) {
            unsafe {
                sys::lkAudioTrackSourceSetAudioOptions(self.ffi.as_ptr(), &options.into());
            }
        }

        pub fn audio_options(&self) -> AudioSourceOptions {
            let ffi_options = unsafe { sys::lkAudioTrackSourceGetAudioOptions(self.ffi.as_ptr()) };
            ffi_options.into()
        }

        pub fn sample_rate(&self) -> u32 {
            unsafe { sys::lkAudioTrackSourceGetSampleRate(self.ffi.as_ptr()) as u32 }
        }

        pub fn num_channels(&self) -> u32 {
            unsafe { sys::lkAudioTrackSourceGetNumChannels(self.ffi.as_ptr()) as u32 }
        }
    }

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
            let audio_sink_box: *mut Arc<AudioSinkWrapper> =
                Box::into_raw(Box::new(audio_sink_wrapper.clone()));
            Self {
                observer: audio_sink_wrapper,
                ffi: unsafe {
                    let sink = sys::lkCreateNativeAudioSink(
                        sample_rate,
                        number_of_channels,
                        Some(NativeAudioSink::native_on_audio_data),
                        audio_sink_box as *mut ::std::os::raw::c_void,
                    );
                    sys::RefCounted::from_raw(sink)
                },
            }
        }

        pub extern "C" fn native_on_audio_data(
            audio_data: *mut i16,
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

    use crate::audio_frame::AudioFrame;

    #[tokio::test]
    async fn create_audio_native_sink() {
        {
            let (frame_tx, mut frame_rx) = mpsc::unbounded_channel();
            let observer = Arc::new(super::native::AudioTrackObserver { frame_tx });
            let audio_sink_wrapper = Arc::new(super::native::AudioSinkWrapper::new(observer));
            let _sink = super::native::NativeAudioSink::new(audio_sink_wrapper, 32000, 1);

            let _source = super::native::NativeAudioSource::new(
                super::AudioSourceOptions::default(),
                48000,
                2,
                100,
            );

            _source.add_sink(&_sink.ffi);

            let options = _source.audio_options();
            println!("Audio source options: {:?}", options);

            _source.set_audio_options(super::AudioSourceOptions {
                echo_cancellation: true,
                noise_suppression: true,
                auto_gain_control: false,
            });

            let options2 = _source.audio_options();
            println!("Audio source options2: {:?}", options2);

            let sampe_rate = _source.sample_rate();
            let num_channels = _source.num_channels();

            _source.clear_buffer();

            _source.capture_frame(&AudioFrame::new(48000, 2, 960)).await.unwrap();

            println!("Audio source sample rate: {}, num channels: {}", sampe_rate, num_channels);

            let audio_frame = frame_rx.recv().await;
            println!("Received audio frame: {:?}", audio_frame);
            assert_eq!(audio_frame.is_some(), true);
            assert_eq!(audio_frame.clone().unwrap().sample_rate, 32000);
            assert_eq!(audio_frame.unwrap().num_channels, 1);
        }
    }
}
