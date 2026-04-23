// Copyright 2026 LiveKit, Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! FFI wrapper for the high-level `EncodedTcpIngest` helper.
//!
//! The Rust `EncodedTcpIngest` owns the publish + TCP loop; this layer
//! simply:
//!
//! 1. Converts protobuf options to the Rust options.
//! 2. Calls `EncodedTcpIngest::start` from the FFI async runtime.
//! 3. Stores the resulting handle and surfaces ingest-level callbacks
//!    through an [`IngestObserverBridge`] so C++ / Python / Swift clients
//!    see them as [`proto::EncodedTcpIngestEvent`].

use std::{net::SocketAddr, sync::Arc, time::Duration};

use livekit::{
    prelude::*,
    video_ingest::{EncodedIngestObserver, EncodedTcpIngest, EncodedTcpIngestOptions},
};

use super::{room::FfiRoom, FfiHandle};
use crate::{proto, server, FfiError, FfiHandleId, FfiResult};

/// Server-side owner of an [`EncodedTcpIngest`]. Stored behind an
/// [`FfiHandleId`] in the FFI handle table.
pub struct FfiEncodedTcpIngest {
    pub handle_id: FfiHandleId,
    pub ingest: Arc<tokio::sync::Mutex<Option<EncodedTcpIngest>>>,
}

impl FfiHandle for FfiEncodedTcpIngest {}

/// Kicks off an async `EncodedTcpIngest::start` and returns the async id
/// immediately. The result (or error) is dispatched as
/// [`proto::NewEncodedTcpIngestCallback`].
pub fn create(
    server: &'static server::FfiServer,
    req: proto::NewEncodedTcpIngestRequest,
) -> FfiResult<proto::NewEncodedTcpIngestResponse> {
    let async_id = server.resolve_async_id(req.request_async_id);
    let ffi_room = server.retrieve_handle::<FfiRoom>(req.room_handle)?.clone();

    let options = match options_from_proto(&req) {
        Ok(opts) => opts,
        Err(e) => {
            let _ = server.send_event(
                proto::NewEncodedTcpIngestCallback {
                    async_id,
                    message: Some(proto::new_encoded_tcp_ingest_callback::Message::Error(
                        e.to_string(),
                    )),
                }
                .into(),
            );
            return Ok(proto::NewEncodedTcpIngestResponse { async_id });
        }
    };

    let handle = server.async_runtime.spawn(async move {
        let participant = ffi_room.inner.room.local_participant();
        match EncodedTcpIngest::start(participant, options).await {
            Ok(ingest) => {
                let handle_id = server.next_id();
                let track_sid = ingest.track_sid();
                let track_name = ingest.track().name();
                ingest.set_observer(Arc::new(IngestObserverBridge {
                    server,
                    ingest_handle: handle_id,
                }));

                let info = proto::EncodedTcpIngestInfo {
                    track_sid: track_sid.to_string(),
                    track_name,
                };

                let ffi_ingest = FfiEncodedTcpIngest {
                    handle_id,
                    ingest: Arc::new(tokio::sync::Mutex::new(Some(ingest))),
                };
                server.store_handle(handle_id, ffi_ingest);

                let _ = server.send_event(
                    proto::NewEncodedTcpIngestCallback {
                        async_id,
                        message: Some(
                            proto::new_encoded_tcp_ingest_callback::Message::Ingest(
                                proto::OwnedEncodedTcpIngest {
                                    handle: proto::FfiOwnedHandle { id: handle_id },
                                    info,
                                },
                            ),
                        ),
                    }
                    .into(),
                );
            }
            Err(err) => {
                let _ = server.send_event(
                    proto::NewEncodedTcpIngestCallback {
                        async_id,
                        message: Some(proto::new_encoded_tcp_ingest_callback::Message::Error(
                            err.to_string(),
                        )),
                    }
                    .into(),
                );
            }
        }
    });
    server.watch_panic(handle);

    Ok(proto::NewEncodedTcpIngestResponse { async_id })
}

/// Stops a running ingest. Async because `EncodedTcpIngest::stop` awaits
/// the background task and optionally unpublishes the track.
pub fn stop(
    server: &'static server::FfiServer,
    req: proto::StopEncodedTcpIngestRequest,
) -> FfiResult<proto::StopEncodedTcpIngestResponse> {
    let async_id = server.resolve_async_id(req.request_async_id);
    let ingest_handle = req.ingest_handle;

    let ingest_slot = {
        let ffi_ingest = server.retrieve_handle::<FfiEncodedTcpIngest>(ingest_handle)?;
        ffi_ingest.ingest.clone()
    };

    let handle = server.async_runtime.spawn(async move {
        let taken = { ingest_slot.lock().await.take() };
        let error = match taken {
            Some(ingest) => {
                ingest.stop().await;
                None
            }
            None => Some("EncodedTcpIngest: already stopped".to_string()),
        };
        let _ = server.send_event(
            proto::StopEncodedTcpIngestCallback { async_id, error }.into(),
        );
    });
    server.watch_panic(handle);

    Ok(proto::StopEncodedTcpIngestResponse { async_id })
}

/// Pulls a stats snapshot synchronously.
pub fn get_stats(
    server: &'static server::FfiServer,
    req: proto::GetEncodedTcpIngestStatsRequest,
) -> FfiResult<proto::GetEncodedTcpIngestStatsResponse> {
    let ffi_ingest = server.retrieve_handle::<FfiEncodedTcpIngest>(req.ingest_handle)?;
    let guard = ffi_ingest.ingest.try_lock().map_err(|_| {
        FfiError::InvalidRequest("EncodedTcpIngest is busy (stop in progress?)".into())
    })?;
    let Some(ingest) = guard.as_ref() else {
        return Err(FfiError::InvalidRequest("EncodedTcpIngest is stopped".into()));
    };
    let stats = ingest.stats();
    Ok(proto::GetEncodedTcpIngestStatsResponse {
        stats: proto::EncodedTcpIngestStats {
            frames_accepted: stats.frames_accepted,
            frames_dropped: stats.frames_dropped,
            keyframes: stats.keyframes,
            tcp_reconnects: stats.tcp_reconnects,
        },
    })
}

fn options_from_proto(req: &proto::NewEncodedTcpIngestRequest) -> FfiResult<EncodedTcpIngestOptions> {
    let port = u16::try_from(req.port)
        .map_err(|_| FfiError::InvalidRequest("port must fit in u16".into()))?;
    let codec = video_codec_from_proto(req.codec());
    let track_source = req
        .track_source
        .and_then(|s| proto::TrackSource::try_from(s).ok())
        .map(TrackSource::from)
        .unwrap_or(TrackSource::Camera);

    let mut opts = EncodedTcpIngestOptions::new(port, codec, req.width, req.height);
    opts.host = req.host.clone();
    opts.track_name = req.track_name.clone();
    opts.track_source = track_source;
    opts.max_bitrate_bps = req.max_bitrate_bps;
    if let Some(fps) = req.max_framerate_fps {
        opts.max_framerate_fps = fps;
    }
    if let Some(ms) = req.reconnect_backoff_ms {
        opts.reconnect_backoff = Duration::from_millis(ms as u64);
    }
    if let Some(unpublish) = req.unpublish_on_stop {
        opts.unpublish_on_stop = unpublish;
    }
    Ok(opts)
}

fn video_codec_from_proto(codec: proto::VideoCodec) -> livekit::webrtc::video_source::VideoCodec {
    use livekit::webrtc::video_source::VideoCodec;
    match codec {
        proto::VideoCodec::H264 => VideoCodec::H264,
        proto::VideoCodec::H265 => VideoCodec::H265,
        proto::VideoCodec::Vp8 => VideoCodec::Vp8,
        proto::VideoCodec::Vp9 => VideoCodec::Vp9,
        proto::VideoCodec::Av1 => VideoCodec::Av1,
    }
}

/// Forwards ingest-level callbacks out to the FFI client as
/// [`proto::EncodedTcpIngestEvent`]s.
struct IngestObserverBridge {
    server: &'static server::FfiServer,
    ingest_handle: FfiHandleId,
}

impl IngestObserverBridge {
    fn emit(&self, message: proto::encoded_tcp_ingest_event::Message) {
        let _ = self.server.send_event(
            proto::EncodedTcpIngestEvent {
                ingest_handle: self.ingest_handle,
                message: Some(message),
            }
            .into(),
        );
    }
}

impl EncodedIngestObserver for IngestObserverBridge {
    fn on_connected(&self, peer: SocketAddr) {
        self.emit(proto::encoded_tcp_ingest_event::Message::Connected(
            proto::encoded_tcp_ingest_event::Connected { peer: peer.to_string() },
        ));
    }

    fn on_disconnected(&self, reason: &str) {
        self.emit(proto::encoded_tcp_ingest_event::Message::Disconnected(
            proto::encoded_tcp_ingest_event::Disconnected { reason: reason.to_string() },
        ));
    }

    fn on_keyframe_requested(&self) {
        self.emit(proto::encoded_tcp_ingest_event::Message::KeyframeRequested(
            proto::encoded_tcp_ingest_event::KeyframeRequested {},
        ));
    }

    fn on_target_bitrate(&self, bitrate_bps: u32, framerate_fps: f64) {
        self.emit(proto::encoded_tcp_ingest_event::Message::TargetBitrateChanged(
            proto::encoded_tcp_ingest_event::TargetBitrateChanged {
                bitrate_bps,
                framerate_fps,
            },
        ));
    }
}
