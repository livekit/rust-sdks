use crate::{proto, server, FFIError, FFIHandleId, FFIResult};
use futures_util::StreamExt;
use livekit::prelude::*;
use livekit::webrtc::audio_frame::AudioFrame;
use livekit::webrtc::audio_source::native::NativeAudioSource;
use livekit::webrtc::audio_stream::native::NativeAudioStream;
use livekit::webrtc::media_stream::MediaStreamTrack;
use log::warn;
use server::utils;
use tokio::sync::oneshot;

// ===== FFIAudioStream =====

pub struct FFIAudioStream {
    handle_id: FFIHandleId,
    stream_type: proto::AudioStreamType,
    track_sid: TrackSid,

    #[allow(dead_code)]
    close_tx: oneshot::Sender<()>, // Close the stream on drop
}

impl FFIAudioStream {
    /// Setup a new AudioStream and forward the audio data to the client/the foreign
    /// language.
    ///
    /// When FFIAudioStream is dropped (When the corresponding handle_id is dropped), the task
    /// is being closed.
    ///
    /// It is possible that the client receives an AudioFrame after the task is closed. The client
    /// musts ignore it.
    pub fn setup(
        server: &'static server::FFIServer,
        new_stream: proto::NewAudioStreamRequest,
    ) -> FFIResult<proto::AudioStreamInfo> {
        let (close_tx, close_rx) = oneshot::channel();
        let stream_type = proto::AudioStreamType::from_i32(new_stream.r#type).unwrap();
        let track_sid: TrackSid = new_stream.track_sid.into();

        let track = utils::find_remote_track(
            server,
            &track_sid,
            &new_stream.participant_sid.into(),
            &new_stream.room_sid.into(),
        )?
        .rtc_track();

        let MediaStreamTrack::Audio(track) = track else {
            return Err(FFIError::InvalidRequest("not an audio track"));
        };

        let audio_stream = match stream_type {
            proto::AudioStreamType::AudioStreamNative => {
                let audio_stream = Self {
                    handle_id: server.next_id(),
                    stream_type,
                    close_tx,
                    track_sid,
                };
                tokio::spawn(Self::native_audio_stream_task(
                    server,
                    audio_stream.handle_id,
                    NativeAudioStream::new(track),
                    close_rx,
                ));
                Ok::<FFIAudioStream, FFIError>(audio_stream)
            }
            // TODO(theomonnom): Support other stream types
            _ => return Err(FFIError::InvalidRequest("unsupported audio stream type")),
        }?;

        // Store the new audio stream and return the info
        let info = proto::AudioStreamInfo::from(&audio_stream);
        server
            .ffi_handles()
            .write()
            .insert(audio_stream.handle_id, Box::new(audio_stream));

        Ok(info)
    }

    pub fn handle_id(&self) -> FFIHandleId {
        self.handle_id
    }

    pub fn stream_type(&self) -> proto::AudioStreamType {
        self.stream_type
    }

    pub fn track_sid(&self) -> &TrackSid {
        &self.track_sid
    }

    async fn native_audio_stream_task(
        server: &'static server::FFIServer,
        stream_handle_id: FFIHandleId,
        mut native_stream: NativeAudioStream,
        mut close_rx: oneshot::Receiver<()>,
    ) {
        loop {
            tokio::select! {
                _ = &mut close_rx => {
                    break;
                }
                frame = native_stream.next() => {
                    let Some(frame) = frame else {
                        break;
                    };

                    let handle_id = server.next_id();
                    let buffer_info = proto::AudioFrameBufferInfo::from(handle_id, &frame);

                    server.ffi_handles().write().insert(handle_id, Box::new(frame));

                    if let Err(err) = server.send_event(proto::ffi_event::Message::AudioStreamEvent(
                        proto::AudioStreamEvent {
                            handle: Some(stream_handle_id.into()),
                            message: Some(proto::audio_stream_event::Message::FrameReceived(
                                proto::AudioFrameReceived {
                                    frame: Some(buffer_info),
                                },
                            )),
                        },
                    )) {
                        warn!("failed to send audio frame: {}", err);
                    }
                }
            }
        }
    }
}

// ===== FFIAudioSource =====

pub struct FFIAudioSource {
    handle_id: FFIHandleId,
    source_type: proto::AudioSourceType,
    source: AudioSource,
}

#[derive(Clone)]
pub enum AudioSource {
    Native(NativeAudioSource),
}

impl FFIAudioSource {
    pub fn setup(
        server: &'static server::FFIServer,
        new_source: proto::NewAudioSourceRequest,
    ) -> FFIResult<proto::AudioSourceInfo> {
        let source_type = proto::AudioSourceType::from_i32(new_source.r#type).unwrap();
        let source_inner = match source_type {
            proto::AudioSourceType::AudioSourceNative => {
                let audio_source = NativeAudioSource::default();
                Ok::<AudioSource, FFIError>(AudioSource::Native(audio_source))
            }
            _ => return Err(FFIError::InvalidRequest("unsupported audio source type")),
        }?;

        let audio_source = Self {
            handle_id: server.next_id(),
            source_type,
            source: source_inner,
        };
        let source_info = proto::AudioSourceInfo::from(&audio_source);

        server
            .ffi_handles()
            .write()
            .insert(audio_source.handle_id, Box::new(audio_source));

        Ok(source_info)
    }

    pub fn capture_frame(
        &self,
        server: &'static server::FFIServer,
        capture: proto::CaptureAudioFrameRequest,
    ) -> FFIResult<()> {
        match self.source {
            AudioSource::Native(ref source) => {
                let frame_info = capture
                    .frame
                    .ok_or(FFIError::InvalidRequest("frame is empty"))?;

                let ffi_handles = server.ffi_handles().read();
                let handle_id = frame_info
                    .handle
                    .ok_or(FFIError::InvalidRequest("handle is empty"))?
                    .id as FFIHandleId;

                let frame = ffi_handles
                    .get(&handle_id)
                    .ok_or(FFIError::InvalidRequest("handle not found"))?
                    .downcast_ref::<AudioFrame>()
                    .ok_or(FFIError::InvalidRequest("handle is not an audio frame"))?;

                source.capture_frame(frame);
            }
        }

        Ok(())
    }

    pub fn handle_id(&self) -> FFIHandleId {
        self.handle_id
    }

    pub fn source_type(&self) -> proto::AudioSourceType {
        self.source_type
    }

    pub fn inner_source(&self) -> &AudioSource {
        &self.source
    }
}
