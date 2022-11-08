#[derive(Debug)]
pub enum TrackKind {
    Unknown,
    Audio,
    Video,
}

impl From<u8> for TrackKind {
    fn from(val: u8) -> Self {
        match val {
            1 => Self::Audio,
            2 => Self::Video,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug)]
pub enum StreamState {
    Unknown,
    Active,
    Paused,
}

#[derive(Debug)]
pub enum TrackSource {
    Unknown,
    Camera,
    Microphone,
    Screenshare,
    ScreenshareAudio,
}

impl From<u8> for TrackSource {
    fn from(val: u8) -> Self {
        match val {
            1 => Self::Camera,
            2 => Self::Microphone,
            3 => Self::Screenshare,
            4 => Self::ScreenshareAudio,
            _ => Self::Unknown,
        }
    }
}

pub struct LocalVideoTrack {}
pub struct RemoteVideoTrack {}
pub struct LocalAudioTrack {}


pub struct RemoteAudioTrack {


}

impl RemoteVideoTrack {
    pub(crate) fn new() -> Self {
        Self {}
    }
}

impl RemoteAudioTrack {
    pub(crate) fn new() -> Self {
        Self {}
    }
}

pub enum RemoteTrack {
    Audio(RemoteAudioTrack),
    Video(RemoteVideoTrack),
}

pub enum LocalTrack {
    Audio(LocalAudioTrack),
    Video(LocalVideoTrack),
}

pub enum VideoTrack {
    Local(LocalVideoTrack),
    Remote(RemoteVideoTrack),
}

pub enum AudioTrack {
    Local(LocalAudioTrack),
    Remote(RemoteAudioTrack),
}

pub enum Track {
    LocalVideo(LocalVideoTrack),
    LocalAudio(LocalAudioTrack),
    RemoteVideo(RemoteVideoTrack),
    RemoteAudio(RemoteAudioTrack),
}

impl From<VideoTrack> for Track {
    fn from(video_track: VideoTrack) -> Self {
        match video_track {
            VideoTrack::Local(local_video) => Self::LocalVideo(local_video),
            VideoTrack::Remote(remote_video) => Self::RemoteVideo(remote_video),
        }
    }
}

impl From<AudioTrack> for Track {
    fn from(audio_track: AudioTrack) -> Self {
        match audio_track {
            AudioTrack::Local(local_audio) => Self::LocalAudio(local_audio),
            AudioTrack::Remote(remote_audio) => Self::RemoteAudio(remote_audio),
        }
    }
}

impl From<LocalTrack> for Track {
    fn from(local_track: LocalTrack) -> Self {
        match local_track {
            LocalTrack::Audio(local_audio) => Self::LocalAudio(local_audio),
            LocalTrack::Video(local_video) => Self::LocalVideo(local_video),
        }
    }
}

impl From<RemoteTrack> for Track {
    fn from(remote_track: RemoteTrack) -> Self {
        match remote_track {
            RemoteTrack::Audio(remote_audio) => Self::RemoteAudio(remote_audio),
            RemoteTrack::Video(remote_video) => Self::RemoteVideo(remote_video),
        }
    }
}

impl TryFrom<Track> for VideoTrack {
    type Error = &'static str;

    fn try_from(track: Track) -> Result<Self, Self::Error> {
        match track {
            Track::LocalVideo(local_video) => Ok(Self::Local(local_video)),
            Track::RemoteVideo(remote_video) => Ok(Self::Remote(remote_video)),
            _ => Err("not a video track"),
        }
    }
}

impl TryFrom<Track> for AudioTrack {
    type Error = &'static str;

    fn try_from(track: Track) -> Result<Self, Self::Error> {
        match track {
            Track::LocalAudio(local_audio) => Ok(Self::Local(local_audio)),
            Track::RemoteAudio(remote_audio) => Ok(Self::Remote(remote_audio)),
            _ => Err("not a audio track"),
        }
    }
}

impl TryFrom<Track> for LocalTrack {
    type Error = &'static str;

    fn try_from(track: Track) -> Result<Self, Self::Error> {
        match track {
            Track::LocalAudio(local_audio) => Ok(Self::Audio(local_audio)),
            Track::LocalVideo(local_video) => Ok(Self::Video(local_video)),
            _ => Err("not a local track"),
        }
    }
}

impl TryFrom<Track> for RemoteTrack {
    type Error = &'static str;

    fn try_from(track: Track) -> Result<Self, Self::Error> {
        match track {
            Track::RemoteAudio(remote_audio) => Ok(Self::Audio(remote_audio)),
            Track::RemoteVideo(remote_video) => Ok(Self::Video(remote_video)),
            _ => Err("not a remote track"),
        }
    }
}
