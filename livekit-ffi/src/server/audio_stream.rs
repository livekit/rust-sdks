use crate::{proto, server, FfiError, FfiHandleId, FfiResult};
use futures_util::StreamExt;
use livekit::prelude::*;
use livekit::webrtc::audio_stream::native::NativeAudioStream;
use livekit::webrtc::prelude::*;
use log::warn;
use tokio::sync::oneshot;

pub struct FfiAudioStream {
    handle_id: FfiHandleId,
    stream_type: proto::AudioStreamType,

    #[allow(dead_code)]
    close_tx: oneshot::Sender<()>, // Close the stream on drop
}

impl FfiAudioStream {
    /// Setup a new AudioStream and forward the audio data to the client/the foreign
    /// language.
    ///
    /// When FfiAudioStream is dropped (When the corresponding handle_id is dropped), the task
    /// is being closed.
    ///
    /// It is possible that the client receives an AudioFrame after the task is closed. The client
    /// musts ignore it.
    pub fn setup(
        server: &'static server::FfiServer,
        new_stream: proto::NewAudioStreamRequest,
    ) -> FfiResult<proto::AudioStreamInfo> {
        let (close_tx, close_rx) = oneshot::channel();
        let stream_type = proto::AudioStreamType::from_i32(new_stream.r#type).unwrap();

        let track = server
            .ffi_handles
            .get(&new_stream.track_handle)
            .ok_or(FfiError::InvalidRequest("track not found"))?;

        let track = track
            .downcast_ref::<Track>()
            .ok_or(FfiError::InvalidRequest("handle is not a Track"))?;

        let rtc_track = track.rtc_track();

        let MediaStreamTrack::Audio(rtc_track) = rtc_track else {
            return Err(FfiError::InvalidRequest("not an audio track"));
        };

        let audio_stream = match stream_type {
            #[cfg(not(target_arch = "wasm32"))]
            proto::AudioStreamType::AudioStreamNative => {
                let audio_stream = Self {
                    handle_id: server.next_id(),
                    stream_type,
                    close_tx,
                };

                let native_stream = NativeAudioStream::new(rtc_track);
                server.async_runtime.spawn(Self::native_audio_stream_task(
                    server,
                    audio_stream.handle_id,
                    native_stream,
                    close_rx,
                ));
                Ok::<FfiAudioStream, FfiError>(audio_stream)
            }
            _ => return Err(FfiError::InvalidRequest("unsupported audio stream type")),
        }?;

        // Store the new audio stream and return the info
        let info = proto::AudioStreamInfo::from(&audio_stream);
        server
            .ffi_handles
            .insert(audio_stream.handle_id, Box::new(audio_stream));

        Ok(info)
    }

    pub fn handle_id(&self) -> FfiHandleId {
        self.handle_id
    }

    pub fn stream_type(&self) -> proto::AudioStreamType {
        self.stream_type
    }

    async fn native_audio_stream_task(
        server: &'static server::FfiServer,
        stream_handle_id: FfiHandleId,
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

                    server.ffi_handles.insert(handle_id, Box::new(frame));

                    if let Err(err) = server.send_event(proto::ffi_event::Message::AudioStreamEvent(
                        proto::AudioStreamEvent {
                            source_handle: stream_handle_id,
                            message: Some(proto::audio_stream_event::Message::FrameReceived(
                                proto::AudioFrameReceived {
                                    frame: Some(buffer_info),
                                },
                            )),
                        },
                    )).await {
                        warn!("failed to send audio frame: {}", err);
                    }
                }
            }
        }
    }
}