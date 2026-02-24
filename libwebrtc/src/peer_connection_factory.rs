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
    fmt::Debug,
    sync::{Arc, Mutex},
};

use crate::{
    peer_connection::{PeerConnection, PeerObserver, PEER_OBSERVER},
    rtp_parameters::RtpCapabilities,
    sys::{self, ice_servers_to_native, lkRtcConfiguration},
    MediaType, RtcError, RtcErrorType,
};

#[derive(Debug, Clone)]
pub struct IceServer {
    pub urls: Vec<String>,
    pub username: String,
    pub password: String,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ContinualGatheringPolicy {
    GatherOnce,
    GatherContinually,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum IceTransportsType {
    Relay,
    NoHost,
    All,
}

#[derive(Debug, Clone)]
pub struct RtcConfiguration {
    pub ice_servers: Vec<IceServer>,
    pub continual_gathering_policy: ContinualGatheringPolicy,
    pub ice_transport_type: IceTransportsType,
}

impl Default for RtcConfiguration {
    fn default() -> Self {
        Self {
            ice_servers: vec![],
            continual_gathering_policy: ContinualGatheringPolicy::GatherContinually,
            ice_transport_type: IceTransportsType::All,
        }
    }
}

impl From<RtcConfiguration> for lkRtcConfiguration {
    fn from(config: RtcConfiguration) -> Self {
        lkRtcConfiguration {
            iceServersCount: config.ice_servers.len() as i32,
            iceServers: ice_servers_to_native(&config.ice_servers),
            iceTransportType: config.ice_transport_type.into(),
            gatheringPolicy: config.continual_gathering_policy.into(),
        }
    }
}

#[derive(Clone)]
pub struct PeerConnectionFactory {
    pub(crate) ffi: sys::RefCounted<sys::lkPeerFactory>,
}

impl Default for PeerConnectionFactory {
    fn default() -> Self {
        unsafe {
            let factory = sys::lkCreatePeerFactory();
            Self { ffi: sys::RefCounted::from_raw(factory) }
        }
    }
}

impl Debug for PeerConnectionFactory {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.debug_struct("PeerConnectionFactory").finish()
    }
}

impl From<IceTransportsType> for sys::lkIceTransportType {
    fn from(itt: IceTransportsType) -> Self {
        match itt {
            IceTransportsType::Relay => sys::lkIceTransportType::LK_ICE_TRANSPORT_TYPE_RELAY,
            IceTransportsType::NoHost => sys::lkIceTransportType::LK_ICE_TRANSPORT_TYPE_NO_HOST,
            IceTransportsType::All => sys::lkIceTransportType::LK_ICE_TRANSPORT_TYPE_ALL,
        }
    }
}

impl From<ContinualGatheringPolicy> for sys::lkContinualGatheringPolicy {
    fn from(cgp: ContinualGatheringPolicy) -> Self {
        match cgp {
            ContinualGatheringPolicy::GatherOnce => {
                sys::lkContinualGatheringPolicy::LK_GATHERING_POLICY_ONCE
            }
            ContinualGatheringPolicy::GatherContinually => {
                sys::lkContinualGatheringPolicy::LK_GATHERING_POLICY_CONTINUALLY
            }
        }
    }
}

impl From<MediaType> for sys::lkMediaType {
    fn from(media_type: MediaType) -> Self {
        match media_type {
            MediaType::Audio => sys::lkMediaType::LK_MEDIA_TYPE_AUDIO,
            MediaType::Video => sys::lkMediaType::LK_MEDIA_TYPE_VIDEO,
            MediaType::Data => sys::lkMediaType::LK_MEDIA_TYPE_DATA,
            MediaType::Unsupported => sys::lkMediaType::LK_MEDIA_TYPE_UNSUPPORTED,
        }
    }
}

impl PeerConnectionFactory {
    pub fn create_peer_connection(
        &self,
        config: RtcConfiguration,
    ) -> Result<PeerConnection, RtcError> {
        let lk_config = sys::lkRtcConfiguration {
            iceServersCount: config.ice_servers.len() as i32,
            iceServers: sys::ice_servers_to_native(&config.ice_servers),
            iceTransportType: config.ice_transport_type.into(),
            gatheringPolicy: config.continual_gathering_policy.into(),
        };
        let observer = Arc::new(Mutex::new(PeerObserver::default()));
        let observer_ptr = Arc::into_raw(observer.clone());
        let sys_peer = unsafe {
            sys::lkCreatePeer(
                self.ffi.as_ptr(),
                &lk_config,
                &PEER_OBSERVER,
                observer_ptr as *mut ::std::os::raw::c_void,
            )
        };
        if sys_peer.is_null() {
            unsafe {
                let _ = Arc::from_raw(observer_ptr);
            }
            return Err(RtcError {
                error_type: RtcErrorType::Internal,
                message: "Failed to create PeerConnection".to_owned(),
            });
        }
        let ffi = unsafe { sys::RefCounted::from_raw(sys_peer) };
        let peer = PeerConnection { observer, ffi };
        Ok(peer)
    }

    pub fn get_rtp_sender_capabilities(&self, media_type: MediaType) -> RtpCapabilities {
        let lk_caps =
            unsafe { sys::lkGetRtpSenderCapabilities(self.ffi.as_ptr(), media_type.into()) };

        sys::rtp_capabilities_from_native(unsafe { sys::RefCounted::from_raw(lk_caps) })
    }

    pub fn get_rtp_receiver_capabilities(&self, media_type: MediaType) -> RtpCapabilities {
        let lk_caps =
            unsafe { sys::lkGetRtpReceiverCapabilities(self.ffi.as_ptr(), media_type.into()) };

        sys::rtp_capabilities_from_native(unsafe { sys::RefCounted::from_raw(lk_caps) })
    }
}

pub mod native {
    use crate::sys;
    use crate::{
        audio_source::native::NativeAudioSource, audio_track::RtcAudioTrack,
        peer_connection_factory::PeerConnectionFactory, video_source::native::NativeVideoSource,
        video_track::RtcVideoTrack,
    };

    pub trait PeerConnectionFactoryExt {
        fn create_video_track(&self, label: &str, source: NativeVideoSource) -> RtcVideoTrack;
        fn create_audio_track(&self, label: &str, source: NativeAudioSource) -> RtcAudioTrack;
    }

    impl PeerConnectionFactoryExt for PeerConnectionFactory {
        fn create_video_track(&self, label: &str, source: NativeVideoSource) -> RtcVideoTrack {
            unsafe {
                let sys_track = sys::lkPeerFactoryCreateVideoTrack(
                    self.ffi.as_ptr(),
                    std::ffi::CString::new(label).unwrap().as_ptr(),
                    source.ffi.as_ptr(),
                );
                RtcVideoTrack { ffi: sys::RefCounted::from_raw(sys_track) }
            }
        }

        fn create_audio_track(&self, label: &str, source: NativeAudioSource) -> RtcAudioTrack {
            unsafe {
                let sys_track = sys::lkPeerFactoryCreateAudioTrack(
                    self.ffi.as_ptr(),
                    std::ffi::CString::new(label).unwrap().as_ptr(),
                    source.ffi.as_ptr(),
                );
                RtcAudioTrack { ffi: sys::RefCounted::from_raw(sys_track) }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        audio_frame::AudioFrame,
        audio_source::{native::NativeAudioSource, AudioSourceOptions},
        audio_stream::native::NativeAudioStream,
        peer_connection_factory::native::PeerConnectionFactoryExt,
        video_frame::{I420Buffer, VideoFrame, VideoRotation},
        video_stream::native::NativeVideoStream,
    };

    #[tokio::test]
    async fn create_audio_track_from_source() {
        let _factory = crate::peer_connection_factory::PeerConnectionFactory::default();
        let _source = NativeAudioSource::new(AudioSourceOptions::default(), 48000, 2, 100);
        let _track = _factory.create_audio_track("audio_track_1", _source.clone());
        println!("Created audio track: {:?}", _track.id());
        assert_eq!(_track.id(), "audio_track_1");
        assert_eq!(_track.enabled(), true);
        _track.set_enabled(true);
        assert_eq!(_track.state(), crate::media_stream_track::RtcTrackState::Live);

        let mut audio_stream = NativeAudioStream::new(_track.clone(), 48000, 2);

        let mut audio_frame = AudioFrame::new(48000, 2, 4800);
        audio_frame.data.to_mut().iter_mut().enumerate().for_each(|(i, sample)| {
            *sample = (i as i16) % 100;
        });

        _source.capture_frame(&audio_frame).await.unwrap();

        if let Some(frame) = audio_stream.frame_rx.recv().await {
            println!("Received audio frame with sample rate: {}", frame.sample_rate);
            assert_eq!(frame.sample_rate, 48000);
            assert_eq!(frame.num_channels, 2);
            assert_eq!(frame.samples_per_channel, 480);
        } else {
            panic!("Did not receive audio frame");
        }
    }

    #[tokio::test]
    async fn create_video_track_from_source() {
        let factory = crate::peer_connection_factory::PeerConnectionFactory::default();
        let source = crate::video_source::native::NativeVideoSource::new(
            crate::video_source::VideoResolution { width: 640, height: 480 },
        );
        let track = factory.create_video_track("video_track_1", source.clone());
        println!("Created video track: {:?}", track.id());
        assert_eq!(track.id(), "video_track_1");
        track.set_enabled(false);
        assert_eq!(track.enabled(), false);
        track.set_enabled(true);
        assert_eq!(track.enabled(), true);
        assert_eq!(track.state(), crate::media_stream_track::RtcTrackState::Live);

        let mut stream = NativeVideoStream::new(track.clone());
        let video_frame = VideoFrame {
            buffer: I420Buffer::new(640, 480),
            rotation: VideoRotation::VideoRotation90,
            timestamp_us: 0,
        };
        source.capture_frame(&video_frame);

        if let Some(frame) = stream.frame_rx.recv().await {
            println!("Received video frame with timestamp: {}", frame.timestamp_us);
            assert_eq!(frame.buffer.width(), 640);
            assert_eq!(frame.buffer.height(), 480);
            assert_eq!(frame.rotation, crate::video_frame::VideoRotation::VideoRotation90);
            assert_ne!(frame.timestamp_us, 0);
        } else {
            panic!("Did not receive video frame");
        }
    }

    #[tokio::test]
    async fn get_capabilities() {
        let _factory = crate::peer_connection_factory::PeerConnectionFactory::default();
        let audio_caps = _factory.get_rtp_sender_capabilities(crate::MediaType::Audio);
        println!("Audio Capabilities: {:?}", audio_caps);

        for codec in audio_caps.codecs {
            println!("Audio Codec: {:?}", codec);
        }

        for ext in audio_caps.header_extensions {
            println!("Audio Header Extension: {:?}", ext);
        }

        let video_caps = _factory.get_rtp_receiver_capabilities(crate::MediaType::Video);

        println!("Video Capabilities: {:?}", video_caps);

        for codec in video_caps.codecs {
            println!("Video Codec: {:?}", codec);
        }

        for ext in video_caps.header_extensions {
            println!("Video Header Extension: {:?}", ext);
        }
    }

    #[tokio::test]
    async fn test_peer_connection_factory() {
        let _ = env_logger::builder().is_test(true).try_init();

        let factory = crate::peer_connection_factory::PeerConnectionFactory::default();
        let source = crate::video_source::native::NativeVideoSource::default();
        let _track = factory.create_video_track("test", source);
        drop(factory);
    }
}
