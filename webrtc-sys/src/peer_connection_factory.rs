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
    rc::Rc,
    sync::{Arc, Mutex},
};

use crate::{
    peer_connection::{PeerConnection, PeerObserver, PEER_OBSERVER},
    rtp_parameters::RtpCapabilities,
    sys::{self, lkRtcConfiguration},
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
            iceServers: std::ptr::null_mut(), // TODO: implement ice servers
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

impl PeerConnectionFactory {
    pub fn create_peer_connection(
        &self,
        config: RtcConfiguration,
    ) -> Result<PeerConnection, RtcError> {
        let lk_config = sys::lkRtcConfiguration {
            iceServersCount: config.ice_servers.len() as i32,
            iceServers: sys::toLKIceServers(&config.ice_servers),
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
        if sys_peer == std::ptr::null_mut() {
            unsafe {
                let _ = Rc::from_raw(observer_ptr);
            }
            return Err(RtcError {
                error_type: RtcErrorType::Internal,
                message: "Failed to create PeerConnection".to_owned(),
            });
        }
        let ffi = unsafe { sys::RefCounted::from_raw(sys_peer) };
        let peer = PeerConnection { observer: observer, ffi: ffi };
        Ok(peer)
    }

    pub fn get_rtp_sender_capabilities(&self, _media_type: MediaType) -> RtpCapabilities {
        todo!()
    }

    pub fn get_rtp_receiver_capabilities(&self, _media_type: MediaType) -> RtpCapabilities {
        todo!()
    }
}

pub mod native {
    use crate::sys;
    use crate::{
        video_source::native::NativeVideoSource, video_track::RtcVideoTrack,
        audio_track::RtcAudioTrack,
        audio_source::native::NativeAudioSource, peer_connection_factory::PeerConnectionFactory,
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
        audio_source::{native::NativeAudioSource, AudioSourceOptions},
        peer_connection_factory::native::PeerConnectionFactoryExt,
    };

    #[tokio::test]
    async fn create_audio_track_from_source() {
        let _factory = crate::peer_connection_factory::PeerConnectionFactory::default();
        let _source = NativeAudioSource::new(AudioSourceOptions::default(), 48000, 2, 100);
        let _track = _factory.create_audio_track("audio_track_1", _source);
        println!("Created audio track: {:?}", _track.id());
        assert_eq!(_track.id(), "audio_track_1");
        assert_eq!(_track.enabled(), true);
        _track.set_enabled(true);
        assert_eq!(_track.state(), crate::media_stream_track::RtcTrackState::Live);
    }

    #[tokio::test]
    async fn create_video_track_from_source() {
        let factory = crate::peer_connection_factory::PeerConnectionFactory::default();
        let source = crate::video_source::native::NativeVideoSource::new(
            crate::video_source::VideoResolution { width: 640, height: 480 },
        );
        let track = factory.create_video_track("video_track_1", source);
        println!("Created video track: {:?}", track.id());
        assert_eq!(track.id(), "video_track_1");
        assert_eq!(track.enabled(), true);
        track.set_enabled(true);
        assert_eq!(track.state(), crate::media_stream_track::RtcTrackState::Live);
    }
}
