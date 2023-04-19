use crate::imp::media_stream as imp_ms;
use livekit_protocol::enum_dispatch;
use std::fmt::Debug;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum RtcTrackState {
    Live,
    Ended,
}

#[derive(Clone)]
pub struct MediaStream {
    pub(crate) handle: imp_ms::MediaStream,
}

impl MediaStream {
    pub fn id(&self) -> String {
        self.handle.id()
    }

    pub fn audio_tracks(&self) -> Vec<RtcAudioTrack> {
        self.handle.audio_tracks()
    }

    pub fn video_tracks(&self) -> Vec<RtcVideoTrack> {
        self.handle.video_tracks()
    }
}

impl Debug for MediaStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MediaStream")
            .field("id", &self.id())
            .field("audio_tracks", &self.audio_tracks())
            .field("video_tracks", &self.video_tracks())
            .finish()
    }
}

#[derive(Clone)]
pub struct RtcVideoTrack {
    pub(crate) handle: imp_ms::RtcVideoTrack,
}

#[derive(Clone)]
pub struct RtcAudioTrack {
    pub(crate) handle: imp_ms::RtcAudioTrack,
}

#[derive(Debug, Clone)]
pub enum MediaStreamTrack {
    Video(RtcVideoTrack),
    Audio(RtcAudioTrack),
}

#[cfg(not(target_arch = "wasm32"))]
impl MediaStreamTrack {
    enum_dispatch!(
        [Video, Audio];
        pub(crate) fn sys_handle(self: &Self) -> cxx::SharedPtr<webrtc_sys::media_stream::ffi::MediaStreamTrack>;
    );
}

impl MediaStreamTrack {
    enum_dispatch!(
        [Video, Audio];
        pub fn id(self: &Self) -> String;
        pub fn enabled(self: &Self) -> bool;
        pub fn set_enabled(self: &Self, enabled: bool) -> bool;
        pub fn state(self: &Self) -> RtcTrackState;
    );
}

macro_rules! media_stream_track {
    () => {
        pub fn id(&self) -> String {
            self.handle.id()
        }

        pub fn enabled(&self) -> bool {
            self.handle.enabled()
        }

        pub fn set_enabled(&self, enabled: bool) -> bool {
            self.handle.set_enabled(enabled)
        }

        pub fn state(&self) -> RtcTrackState {
            self.handle.state().into()
        }

        #[cfg(not(target_arch = "wasm32"))]
        pub(crate) fn sys_handle(
            &self,
        ) -> cxx::SharedPtr<webrtc_sys::media_stream::ffi::MediaStreamTrack> {
            self.handle.sys_handle()
        }
    };
}

impl RtcVideoTrack {
    media_stream_track!();
}

impl RtcAudioTrack {
    media_stream_track!();
}

impl Debug for RtcAudioTrack {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RtcAudioTrack")
            .field("id", &self.id())
            .field("enabled", &self.enabled())
            .field("state", &self.state())
            .finish()
    }
}

impl Debug for RtcVideoTrack {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RtcVideoTrack")
            .field("id", &self.id())
            .field("enabled", &self.enabled())
            .field("state", &self.state())
            .finish()
    }
}

impl From<RtcAudioTrack> for MediaStreamTrack {
    fn from(track: RtcAudioTrack) -> Self {
        Self::Audio(track)
    }
}

impl From<RtcVideoTrack> for MediaStreamTrack {
    fn from(track: RtcVideoTrack) -> Self {
        Self::Video(track)
    }
}
