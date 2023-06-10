use crate::audio_source::native::NativeAudioSource;
use crate::audio_track::RtcAudioTrack;
use crate::imp::audio_track as imp_at;
use crate::imp::peer_connection as imp_pc;
use crate::imp::video_track as imp_vt;
use crate::peer_connection::PeerConnection;
use crate::peer_connection_factory::{
    ContinualGatheringPolicy, IceServer, IceTransportsType, RtcConfiguration,
};
use crate::rtp_parameters::RtpCapabilities;
use crate::video_source::native::NativeVideoSource;
use crate::video_track::RtcVideoTrack;
use crate::MediaType;
use crate::RtcError;
use cxx::SharedPtr;
use cxx::UniquePtr;
use lazy_static::lazy_static;
use parking_lot::Mutex;
use std::sync::Arc;
use webrtc_sys::peer_connection as sys_pc;
use webrtc_sys::peer_connection_factory as sys_pcf;
use webrtc_sys::rtc_error as sys_err;
use webrtc_sys::webrtc as sys_rtc;

lazy_static! {
    static ref LOG_SINK: Mutex<Option<UniquePtr<sys_rtc::ffi::LogSink>>> = Default::default();
}

#[derive(Clone)]
pub struct PeerConnectionFactory {
    sys_handle: SharedPtr<sys_pcf::ffi::PeerConnectionFactory>,
}

impl Default for PeerConnectionFactory {
    fn default() -> Self {
        let mut log_sink = LOG_SINK.lock();
        if log_sink.is_none() {
            *log_sink = Some(sys_rtc::ffi::new_log_sink(|msg, severity| {
                let msg = msg
                    .strip_suffix("\r\n")
                    .or(msg.strip_suffix("\n"))
                    .unwrap_or(&msg);

                let lvl = match severity {
                    sys_rtc::ffi::LoggingSeverity::Verbose => log::Level::Trace,
                    sys_rtc::ffi::LoggingSeverity::Info => log::Level::Debug, // Translte webrtc
                    // info to debug log level to avoid polluting the user logs
                    sys_rtc::ffi::LoggingSeverity::Warning => log::Level::Warn,
                    sys_rtc::ffi::LoggingSeverity::Error => log::Level::Error,
                    _ => log::Level::Debug,
                };

                log::log!(target: "libwebrtc", lvl, "{}", msg);
            }));
        }

        Self {
            sys_handle: sys_pcf::ffi::create_peer_connection_factory(),
        }
    }
}

impl PeerConnectionFactory {
    pub fn create_peer_connection(
        &self,
        config: RtcConfiguration,
    ) -> Result<PeerConnection, RtcError> {
        let observer = Arc::new(imp_pc::PeerObserver::default());
        let native_observer = sys_pc::ffi::create_native_peer_connection_observer(Box::new(
            sys_pc::PeerConnectionObserverWrapper::new(observer.clone()),
        ));

        let res = self
            .sys_handle
            .create_peer_connection(config.into(), native_observer);

        match res {
            Ok(sys_handle) => Ok(PeerConnection {
                handle: imp_pc::PeerConnection::configure(sys_handle, observer),
            }),
            Err(e) => Err(unsafe { sys_err::ffi::RtcError::from(e.what()).into() }),
        }
    }

    pub fn create_video_track(&self, label: &str, source: NativeVideoSource) -> RtcVideoTrack {
        RtcVideoTrack {
            handle: imp_vt::RtcVideoTrack {
                sys_handle: self
                    .sys_handle
                    .create_video_track(label.to_string(), source.handle.sys_handle()),
            },
        }
    }

    pub fn create_audio_track(&self, label: &str, source: NativeAudioSource) -> RtcAudioTrack {
        RtcAudioTrack {
            handle: imp_at::RtcAudioTrack {
                sys_handle: self
                    .sys_handle
                    .create_audio_track(label.to_string(), source.handle.sys_handle()),
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

// Conversions
impl From<IceServer> for sys_pcf::ffi::IceServer {
    fn from(value: IceServer) -> Self {
        sys_pcf::ffi::IceServer {
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

impl From<RtcConfiguration> for sys_pcf::ffi::RtcConfiguration {
    fn from(value: RtcConfiguration) -> Self {
        Self {
            ice_servers: value.ice_servers.into_iter().map(Into::into).collect(),
            continual_gathering_policy: value.continual_gathering_policy.into(),
            ice_transport_type: value.ice_transport_type.into(),
        }
    }
}
