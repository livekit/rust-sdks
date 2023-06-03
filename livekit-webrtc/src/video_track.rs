use crate::imp::video_track as imp_vt;
use crate::media_stream_track::media_stream_track;
use crate::media_stream_track::RtcTrackState;
use std::fmt::Debug;

#[derive(Clone)]
pub struct RtcVideoTrack {
    pub(crate) handle: imp_vt::RtcVideoTrack,
}

impl RtcVideoTrack {
    media_stream_track!();
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
