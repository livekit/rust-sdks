use crate::{
    proto, proto::ffi_request::Message as FFIRequest, proto::ffi_response::Message as FFIResponse,
};
use lazy_static::lazy_static;
use livekit::prelude::*;
use livekit::webrtc::media_stream::OnFrameHandler;
use parking_lot::{Mutex, RwLock};
use prost::Message;
use std::any::Any;
use std::collections::HashMap;
use std::panic;
use std::slice;
use std::sync::atomic::AtomicU32;
use std::sync::atomic::{AtomicBool, Ordering};
use thiserror::Error;

mod conversion;

#[derive(Error, Debug)]
pub enum FFIError {
    #[error("the FFIServer isn't configured")]
    NotConfigured,
    #[error("failed to execute the ffi callback")]
    CallbackFailed,
}

pub type FFIHandleId = u32;
pub type FFIHandle = Box<dyn Any + Send + Sync>;

type CallbackFn = unsafe extern "C" fn(*const u8, usize); // This "C" callback must be threadsafe

lazy_static! {
    static ref FFI_SERVER: FFIServer = FFIServer::default();
}

pub struct FFIConfig {
    callback_fn: CallbackFn,
}

/// To use the FFI, the foreign language and the FFI server must share
/// the same memory space
pub struct FFIServer {
    // Object owned by the foreign language
    // The foreign language is responsible for freeing this memory
    ffi_owned: RwLock<HashMap<FFIHandleId, FFIHandle>>,
    next_handle: AtomicU32, // FFIHandle

    rooms: RwLock<HashMap<RoomSid, Room>>,
    async_runtime: tokio::runtime::Runtime,
    initialized: AtomicBool,
    config: Mutex<Option<FFIConfig>>,
}

impl Default for FFIServer {
    fn default() -> Self {
        Self {
            ffi_owned: RwLock::new(HashMap::new()),
            next_handle: Default::default(),
            rooms: RwLock::new(HashMap::new()),
            async_runtime: tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .unwrap(),
            initialized: Default::default(),
            config: Default::default(),
        }
    }
}

impl FFIServer {
    pub fn next_handle_id(&self) -> FFIHandleId {
        self.next_handle.fetch_add(1, Ordering::SeqCst) as FFIHandleId
    }

    pub fn insert_handle(&self, handle_id: FFIHandleId, handle: FFIHandle) {
        self.ffi_owned.write().insert(handle_id, handle);
    }

    pub fn release_handle(&self, handle_id: FFIHandleId) -> Option<FFIHandle> {
        self.ffi_owned.write().remove(&handle_id)
    }

    pub fn send_response(&self, message: FFIResponse) -> Result<(), FFIError> {
        if !self.initialized.load(Ordering::SeqCst) {
            Err(FFIError::NotConfigured)?
        }

        let message = proto::FfiResponse {
            message: Some(message),
        }
        .encode_to_vec();

        let callback_fn = self.config.lock().as_ref().unwrap().callback_fn;
        if let Err(err) = panic::catch_unwind(|| unsafe {
            callback_fn(message.as_ptr(), message.len());
        }) {
            eprintln!("panic when sending ffi response: {:?}", err);
            Err(FFIError::CallbackFailed)?
        }

        Ok(())
    }

    pub fn on_request_received(&self, message: FFIRequest) -> Result<(), FFIError> {
        if let FFIRequest::Configure(ref init) = message {
            self.initialized.store(true, Ordering::SeqCst);
            *self.config.lock() = Some(FFIConfig {
                callback_fn: unsafe { std::mem::transmute(init.callback_ptr) },
            });
        }

        if !self.initialized.load(Ordering::SeqCst) {
            Err(FFIError::NotConfigured)?
        }

        match message {
            proto::ffi_request::Message::AsyncConnect(connect) => {
                self.async_runtime.spawn(room_task(connect));
            }
            _ => {}
        };

        Ok(())
    }
}

#[no_mangle]
pub extern "C" fn livekit_ffi_request(data: *const u8, len: usize) {
    let data = unsafe { slice::from_raw_parts(data, len) };
    let res = proto::FfiRequest::decode(data);
    if let Err(ref err) = res {
        eprintln!("failed to decode FfiRequest: {:?}", err);
    }

    let res = FFI_SERVER.on_request_received(res.unwrap().message.unwrap());
    if let Err(ref err) = res {
        eprintln!("failed to handle ffi request: {:?}", err);
    }
}

// Connect a listen to Room events
async fn room_task(connect: proto::ConnectRequest) {
    let res = Room::connect(&connect.url, &connect.token).await;

    if res.is_err() {
        let _ = FFI_SERVER.send_response(FFIResponse::AsyncConnect(proto::ConnectResponse {
            success: false,
            room: None,
        }));
        return;
    }

    // Send connect response before listening to events
    let (room, mut events) = res.unwrap();
    let session = room.session();

    let _ = FFI_SERVER.send_response(FFIResponse::AsyncConnect(proto::ConnectResponse {
        success: true,
        room: Some(proto::RoomInfo {
            sid: session.sid(),
            name: session.name(),
            local_participant: Some((&room.session().local_participant()).into()),
            participants: room
                .session()
                .participants()
                .iter()
                .map(|(_, p)| p.into())
                .collect(),
        }),
    }));

    // Listen to events
    tokio::spawn(participant_task(Participant::Local(
        session.local_participant(),
    )));

    while let Some(event) = events.recv().await {
        if let Some(event) = proto::RoomEvent::from(session.sid(), event.clone()) {
            let _ = FFI_SERVER.send_response(FFIResponse::RoomEvent(event));
        }

        match event {
            RoomEvent::ParticipantConnected(p) => {
                tokio::spawn(participant_task(Participant::Remote(p)));
            }
            RoomEvent::TrackSubscribed {
                track,
                publication,
                participant,
            } => {
                if let RemoteTrackHandle::Video(video_track) = track {
                    let rtc_track = video_track.rtc_track();
                    rtc_track.on_frame(on_video_frame(video_track.sid()));
                }
            }
            _ => {}
        }
    }
}

// Listen to participant events
async fn participant_task(participant: Participant) {
    let mut participant_events = participant.register_observer();
    while let Some(event) = participant_events.recv().await {
        // TODO convert event to proto
    }
}

fn on_video_frame(track_sid: TrackSid) -> OnFrameHandler {
    Box::new(move |frame, buffer| {
        let handle_id = FFI_SERVER.next_handle_id();
        let proto_buffer = proto::VideoFrameBuffer::from(handle_id, &buffer);
        FFI_SERVER.insert_handle(handle_id, Box::new(buffer));

        let _ = FFI_SERVER.send_response(FFIResponse::TrackEvent(proto::TrackEvent {
            track_sid: track_sid.to_string(),
            message: Some(proto::track_event::Message::FrameReceived(
                proto::FrameReceived {
                    frame: Some(frame.into()),
                    frame_buffer: Some(proto_buffer),
                },
            )),
        }));
    })
}
