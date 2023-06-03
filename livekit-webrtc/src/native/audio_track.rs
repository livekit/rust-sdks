use super::media_stream_track::impl_media_stream_track;
use crate::media_stream_track::RtcTrackState;
use cxx::SharedPtr;
use sys_at::ffi::audio_to_media;
use webrtc_sys::audio_track as sys_at;

#[derive(Clone)]
pub struct RtcAudioTrack {
    pub(crate) sys_handle: SharedPtr<sys_at::ffi::AudioTrack>,
}

impl RtcAudioTrack {
    impl_media_stream_track!(audio_to_media);

    pub fn sys_handle(&self) -> SharedPtr<sys_at::ffi::MediaStreamTrack> {
        audio_to_media(self.sys_handle.clone())
    }
}
