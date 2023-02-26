use crate::imp::media_stream as imp_ms;

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
pub struct VideoTrack {
    pub(crate) handle: imp_ms::VideoTrack,
}

#[derive(Clone)]
pub struct AudioTrack {
    pub(crate) handle: imp_ms::AudioTrack,
}

pub(crate) mod internal {
    #[doc(hidden)]
    pub trait MediaStreamTrackInternal {
        #[cfg(not(target_arch = "wasm32"))]
        fn sys_handle(&self) -> cxx::SharedPtr<webrtc_sys::media_stream::ffi::MediaStreamTrack>;
    }

    impl MediaStreamTrackInternal for super::VideoTrack {
        #[cfg(not(target_arch = "wasm32"))]
        fn sys_handle(&self) -> cxx::SharedPtr<webrtc_sys::media_stream::ffi::MediaStreamTrack> {
            self.handle.sys_handle()
        }
    }

    impl MediaStreamTrackInternal for super::AudioTrack {
        #[cfg(not(target_arch = "wasm32"))]
        fn sys_handle(&self) -> cxx::SharedPtr<webrtc_sys::media_stream::ffi::MediaStreamTrack> {
            self.handle.sys_handle()
        }
    }
}

pub trait MediaStreamTrack: internal::MediaStreamTrackInternal {
    fn kind(&self) -> TrackKind;
    fn id(&self) -> String;
    fn enabled(&self) -> bool;
    fn set_enabled(&self, enabled: bool) -> bool;
    fn state(&self) -> TrackState;

    fn as_video_track(&self) -> Option<&VideoTrack> {
        None
    }

    fn as_audio_track(&self) -> Option<&AudioTrack> {
        None
    }
}

macro_rules! impl_media_stream_track {
    () => {
        fn kind(&self) -> TrackKind {
            self.handle.kind()
        }

        fn id(&self) -> String {
            self.handle.id()
        }

        fn enabled(&self) -> bool {
            self.handle.enabled()
        }

        fn set_enabled(&self, enabled: bool) -> bool {
            self.handle.set_enabled(enabled)
        }

        fn state(&self) -> TrackState {
            self.handle.state().into()
        }
    };
}

impl MediaStreamTrack for VideoTrack {
    impl_media_stream_track!();

    fn as_video_track(&self) -> Option<&VideoTrack> {
        Some(self)
    }
}

impl MediaStreamTrack for AudioTrack {
    impl_media_stream_track!();

    fn as_audio_track(&self) -> Option<&AudioTrack> {
        Some(self)
    }
}
