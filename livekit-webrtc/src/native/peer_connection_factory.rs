use crate::imp::media_stream as imp_ms;
use crate::imp::peer_connection as imp_pc;
use crate::media_stream::RtcVideoTrack;
use crate::peer_connection::PeerConnection;
use crate::peer_connection_factory::{
    ContinualGatheringPolicy, IceServer, IceTransportsType, RtcConfiguration,
};
use crate::rtp_parameters::RtpCapabilities;
use crate::video_source::native::NativeVideoSource;
use crate::MediaType;
use crate::RtcError;
use cxx::SharedPtr;
use std::sync::Arc;
use webrtc_sys::peer_connection as sys_pc;
use webrtc_sys::peer_connection_factory as sys_pcf;
use webrtc_sys::rtc_error as sys_err;
use webrtc_sys::webrtc as sys_webrtc;

impl From<IceServer> for sys_pcf::ffi::ICEServer {
    fn from(value: IceServer) -> Self {
        sys_pcf::ffi::ICEServer {
            urls: value.urls,
            username: value.username,
            password: value.password,
        }
    }
}

impl From<ContinualGatheringPolicy> for sys_pcf::ffi::ContinualGatheringPolicy {
    fn from(value: ContinualGatheringPolicy) -> Self {
        match value {
            ContinualGatheringPolicy::GatherOnce => {
                sys_pcf::ffi::ContinualGatheringPolicy::GatherOnce
            }
            ContinualGatheringPolicy::GatherContinually => {
                sys_pcf::ffi::ContinualGatheringPolicy::GatherContinually
            }
        }
    }
}

impl From<IceTransportsType> for sys_pcf::ffi::IceTransportsType {
    fn from(value: IceTransportsType) -> Self {
        match value {
            IceTransportsType::None => sys_pcf::ffi::IceTransportsType::None,
            IceTransportsType::Relay => sys_pcf::ffi::IceTransportsType::Relay,
            IceTransportsType::NoHost => sys_pcf::ffi::IceTransportsType::NoHost,
            IceTransportsType::All => sys_pcf::ffi::IceTransportsType::All,
        }
    }
}

impl From<RtcConfiguration> for sys_pcf::ffi::RTCConfiguration {
    fn from(value: RtcConfiguration) -> Self {
        Self {
            ice_servers: value.ice_servers.into_iter().map(Into::into).collect(),
            continual_gathering_policy: value.continual_gathering_policy.into(),
            ice_transport_type: value.ice_transport_type.into(),
        }
    }
}

#[derive(Clone)]
pub struct RTCRuntime {
    pub(crate) sys_handle: SharedPtr<sys_webrtc::ffi::RTCRuntime>,
}

impl Default for RTCRuntime {
    fn default() -> Self {
        Self {
            sys_handle: sys_webrtc::ffi::create_rtc_runtime(),
        }
    }
}

#[derive(Clone)]
pub struct PeerConnectionFactory {
    sys_handle: SharedPtr<sys_pcf::ffi::PeerConnectionFactory>,

    #[allow(unused)]
    runtime: RTCRuntime,
}

impl Default for PeerConnectionFactory {
    fn default() -> Self {
        let runtime = RTCRuntime::default();
        Self {
            sys_handle: sys_pcf::ffi::create_peer_connection_factory(runtime.sys_handle.clone()),
            runtime,
        }
    }
}

impl PeerConnectionFactory {
    pub fn create_peer_connection(
        &self,
        config: RtcConfiguration,
    ) -> Result<PeerConnection, RtcError> {
        let native_config = sys_pcf::ffi::create_rtc_configuration(config.into());

        unsafe {
            let observer = Arc::new(imp_pc::PeerObserver::default());
            let native_observer = sys_pc::ffi::create_native_peer_connection_observer(
                self.runtime.clone().sys_handle,
                Box::new(sys_pc::PeerConnectionObserverWrapper::new(observer.clone())),
            );

            let res = self
                .sys_handle
                .create_peer_connection(native_config, &*native_observer as *const _ as *mut _);

            match res {
                Ok(sys_handle) => Ok(PeerConnection {
                    handle: imp_pc::PeerConnection::configure(
                        sys_handle,
                        observer,
                        native_observer,
                    ),
                }),
                Err(e) => Err(sys_err::ffi::RTCError::from(e.what()).into()),
            }
        }
    }

    pub fn create_video_track(&self, label: &str, source: NativeVideoSource) -> RtcVideoTrack {
        RtcVideoTrack {
            handle: imp_ms::RtcVideoTrack {
                sys_handle: self
                    .sys_handle
                    .create_video_track(label.to_string(), source.handle.sys_handle()),
            },
        }
    }

    pub fn get_rtp_sender_capabilities(&self, media_type: MediaType) -> RtpCapabilities {
        self.sys_handle
            .get_rtp_sender_capabilities(media_type.into())
            .into()
    }

    pub fn get_rtp_receiver_capabilities(&self, media_type: MediaType) -> RtpCapabilities {
        self.sys_handle
            .get_rtp_receiver_capabilities(media_type.into())
            .into()
    }
}
