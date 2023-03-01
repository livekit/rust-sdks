use crate::imp::media_stream as imp_ms;
use livekit_utils::enum_dispatch;
use std::fmt::Debug;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum TrackState {
    Live,
    Ended,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum TrackKind {
    Audio,
    Video,
}

#[derive(Clone)]
pub struct MediaStream {
    pub(crate) handle: imp_ms::MediaStream,
}

impl MediaStream {
    pub fn id(&self) -> String {
        self.handle.id()
    }

    pub fn audio_tracks(&self) -> Vec<AudioTrack> {
        self.handle.audio_tracks()
    }

    pub fn video_tracks(&self) -> Vec<VideoTrack> {
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
pub struct VideoTrack {
    pub(crate) handle: imp_ms::VideoTrack,
}

#[derive(Clone)]
pub struct AudioTrack {
    pub(crate) handle: imp_ms::AudioTrack,
}

#[derive(Debug, Clone)]
pub enum MediaStreamTrack {
    Video(VideoTrack),
    Audio(AudioTrack),
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
        pub fn kind(self: &Self) -> TrackKind;
        pub fn id(self: &Self) -> String;
        pub fn enabled(self: &Self) -> bool;
        pub fn set_enabled(self: &Self, enabled: bool) -> bool;
        pub fn state(self: &Self) -> TrackState;
    );
}

macro_rules! media_stream_track {
    () => {
        pub fn kind(&self) -> TrackKind {
            self.handle.kind()
        }

        pub fn id(&self) -> String {
            self.handle.id()
        }

        pub fn enabled(&self) -> bool {
            self.handle.enabled()
        }

        pub fn set_enabled(&self, enabled: bool) -> bool {
            self.handle.set_enabled(enabled)
        }

        pub fn state(&self) -> TrackState {
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

impl VideoTrack {
    media_stream_track!();
}

impl AudioTrack {
    media_stream_track!();
}

impl Debug for AudioTrack {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AudioTrack")
            .field("id", &self.id())
            .field("enabled", &self.enabled())
            .field("state", &self.state())
            .finish()
    }
}

impl Debug for VideoTrack {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VideoTrack")
            .field("id", &self.id())
            .field("enabled", &self.enabled())
            .field("state", &self.state())
            .finish()
    }
}
