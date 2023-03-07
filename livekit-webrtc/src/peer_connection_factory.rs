use crate::imp::peer_connection_factory as imp_pcf;
use crate::media_stream::VideoTrack;
use crate::peer_connection::PeerConnection;
use crate::RtcError;
use std::fmt::Debug;

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
    None,
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

#[derive(Clone, Default)]
pub struct PeerConnectionFactory {
    pub(crate) handle: imp_pcf::PeerConnectionFactory,
}

impl Debug for PeerConnectionFactory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PeerConnectionFactory").finish()
    }
}

impl PeerConnectionFactory {
    pub fn create_peer_connection(
        &self,
        config: RtcConfiguration,
    ) -> Result<PeerConnection, RtcError> {
        self.handle.create_peer_connection(config)
    }
}

pub mod native {
    use super::PeerConnectionFactory;
    use crate::media_stream::VideoTrack;
    use crate::video_source::native::NativeVideoSource;

    pub trait PeerConnectionFactoryExt {
        fn create_video_track(&self, label: &str, source: NativeVideoSource) -> VideoTrack;
    }

    impl PeerConnectionFactoryExt for PeerConnectionFactory {
        fn create_video_track(&self, label: &str, source: NativeVideoSource) -> VideoTrack {
            self.handle.create_video_track(label, source)
        }
    }
}
