use crate::audio_track::RtcAudioTrack;
use crate::imp::media_stream as imp_ms;
use crate::video_track::RtcVideoTrack;
use std::fmt::Debug;

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
