use super::media_stream_track::impl_media_stream_track;
use crate::media_stream_track::RtcTrackState;
use cxx::SharedPtr;
use sys_vt::ffi::video_to_media;
use webrtc_sys::video_track as sys_vt;

#[derive(Clone)]
pub struct RtcVideoTrack {
    pub(crate) sys_handle: SharedPtr<sys_vt::ffi::VideoTrack>,
}

impl RtcVideoTrack {
    impl_media_stream_track!(video_to_media);

    pub fn sys_handle(&self) -> SharedPtr<sys_vt::ffi::MediaStreamTrack> {
        video_to_media(self.sys_handle.clone())
    }
}
