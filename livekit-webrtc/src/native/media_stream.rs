use cxx::SharedPtr;
use webrtc_sys::media_stream as sys_ms;

#[derive(Clone)]
pub struct MediaStream {
    pub(crate) sys_handle: SharedPtr<sys_ms::ffi::MediaStream>,
}

impl MediaStream {
    pub fn id(&self) -> String {
        self.sys_handle.id()
    }

    pub fn audio_tracks(&self) -> Vec<media_stream::RtcAudioTrack> {
        self.sys_handle
            .get_audio_tracks()
            .into_iter()
            .map(|t| media_stream::RtcAudioTrack {
                handle: RtcAudioTrack { sys_handle: t.ptr },
            })
            .collect()
    }

    pub fn video_tracks(&self) -> Vec<media_stream::RtcVideoTrack> {
        self.sys_handle
            .get_video_tracks()
            .into_iter()
            .map(|t| media_stream::RtcVideoTrack {
                handle: RtcVideoTrack { sys_handle: t.ptr },
            })
            .collect()
    }
}
