use crate::audio_source::native::NativeAudioSource;
use crate::imp::media_stream as imp_ms;
use crate::imp::peer_connection as imp_pc;
use crate::media_stream::{RtcAudioTrack, RtcVideoTrack};
use crate::peer_connection::PeerConnection;
use crate::peer_connection_factory::{
    ContinualGatheringPolicy, IceServer, IceTransportsType, RtcConfiguration,
};
use crate::rtp_parameters::RtpCapabilities;
use crate::video_source::native::NativeVideoSource;
use crate::MediaType;
use crate::RtcError;
use cxx::SharedPtr;
use cxx::UniquePtr;
use lazy_static::lazy_static;
use parking_lot::Mutex;
use std::sync::{Arc, Weak};
use webrtc_sys::logsink as sys_ls;
use webrtc_sys::peer_connection as sys_pc;
use webrtc_sys::peer_connection_factory as sys_pcf;
use webrtc_sys::rtc_error as sys_err;

lazy_static! {
    static ref RTC_RUNTIME: Mutex<Weak<RtcRuntime>> = Mutex::new(Weak::new());
}

pub struct RtcRuntime {
    _logsink: UniquePtr<sys_ls::ffi::LogSink>,
}

impl RtcRuntime {
    pub fn instance() -> Arc<RtcRuntime> {
        let mut lk_runtime_ref = RTC_RUNTIME.lock();
        if let Some(lk_runtime) = lk_runtime_ref.upgrade() {
            lk_runtime
        } else {
            log::trace!("RtcRuntime::new()");
            let new_runtime = Arc::new(Self {
                _logsink: sys_ls::ffi::new_log_sink(|msg, severity| {
                    // Forward logs from webrtc to rust log crate
                    let msg = msg
                        .strip_suffix("\r\n")
                        .or(msg.strip_suffix("\n"))
                        .unwrap_or(&msg);

                    let lvl = match severity {
                        sys_ls::ffi::LoggingSeverity::Verbose => log::Level::Trace,
                        sys_ls::ffi::LoggingSeverity::Info => log::Level::Debug, // Translte webrtc
                        // info to debug log level to avoid polluting the user logs
                        sys_ls::ffi::LoggingSeverity::Warning => log::Level::Warn,
                        sys_ls::ffi::LoggingSeverity::Error => log::Level::Error,
                        _ => log::Level::Debug,
                    };

                    log::log!(target: "libwebrtc", lvl, "{}", msg);
                }),
            });
            *lk_runtime_ref = Arc::downgrade(&new_runtime);
            new_runtime
        }
    }
}

impl Drop for RtcRuntime {
    fn drop(&mut self) {
        log::trace!("RtcRuntime::drop()");
    }
}

#[derive(Clone)]
pub struct PeerConnectionFactory {
    sys_handle: SharedPtr<sys_pcf::ffi::PeerConnectionFactory>,

    #[allow(unused)]
    runtime: Arc<RtcRuntime>,
}

impl Default for PeerConnectionFactory {
    fn default() -> Self {
        let runtime = RtcRuntime::instance();
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
                self.runtime.sys_handle.clone(),
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
                Err(e) => Err(sys_err::ffi::RtcError::from(e.what()).into()),
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
            runtime: self.runtime.clone(),
        }
    }

    pub fn create_audio_track(&self, label: &str, source: NativeAudioSource) -> RtcAudioTrack {
        RtcAudioTrack {
            handle: imp_ms::RtcAudioTrack {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_peer_connection_factory() {
        let _ = env_logger::builder().is_test(true).try_init();

        let factory = PeerConnectionFactory::default();
        let source = NativeVideoSource::default();
        let _track = factory.create_video_track("test", source);
        drop(factory);
    }
}
