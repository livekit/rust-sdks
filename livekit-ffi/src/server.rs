use crate::FFIHandle;
use crate::{
    proto, proto::ffi_request::Message as RequestMessage,
    proto::ffi_response::Message as ResponseMessage,
};
use lazy_static::lazy_static;
use livekit::prelude::*;
use parking_lot::{Mutex, RwLock};
use prost::Message;
use std::any::Any;
use std::collections::HashMap;
use std::panic;
use std::slice;
use std::sync::atomic::{AtomicBool, Ordering};

/// The callback function must be thread safe
type CallbackFn = unsafe extern "C" fn(*const u8, usize);

struct FFIConfig {
    callback_fn: CallbackFn,
}

/// To use the FFI, the foreign language and the FFI server must share
/// the same memory space
struct FFIRuntime {
    // Object owned by the foreign language
    // The foreign language is responsible for freeing this memory
    ffi_owned: RwLock<HashMap<FFIHandle, Box<dyn Any + Send + Sync>>>,
    rooms: RwLock<HashMap<RoomSid, Room>>,
    async_runtime: tokio::runtime::Runtime,
    initialized: AtomicBool,
    config: Mutex<Option<FFIConfig>>,
}

impl Default for FFIRuntime {
    fn default() -> Self {
        Self {
            ffi_owned: RwLock::new(HashMap::new()),
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

lazy_static! {
    static ref RUNTIME: FFIRuntime = FFIRuntime::default();
}

#[no_mangle]
pub extern "C" fn livekit_ffi_init(callback_fn: CallbackFn) {
    RUNTIME.initialized.store(true, Ordering::SeqCst);
    *RUNTIME.config.lock() = Some(FFIConfig { callback_fn });
}

#[no_mangle]
pub extern "C" fn livekit_ffi_request(data: *const u8, len: usize) {
    assert!(
        RUNTIME.initialized.load(Ordering::SeqCst),
        "livekit_ffi_init() must be called first"
    );

    let data = unsafe { slice::from_raw_parts(data, len) };
    let request = proto::FfiRequest::decode(data)
        .expect("Failed to decode the FFIRequest, does the protocol version mismatch?");

    match request.message.unwrap() {
        proto::ffi_request::Message::Connect(connect) => {
            RUNTIME.async_runtime.spawn(room_task(connect))
        }
        _ => todo!(),
    };
}

fn send_response(message: proto::ffi_response::Message) {
    assert!(
        RUNTIME.initialized.load(Ordering::SeqCst),
        "livekit_ffi_init() must be called first"
    );

    let response = proto::FfiResponse {
        message: Some(message),
    };
    let buf = response.encode_to_vec(); // TODO(theomonnom): mb avoid allocation
    let res = panic::catch_unwind(|| unsafe {
        (RUNTIME.config.lock().as_ref().unwrap().callback_fn)(buf.as_ptr(), buf.len());
    });

    if let Err(err) = res {
        eprintln!("panic when sending ffi response: {:?}", err);
    }
}

// Connect a listen to Room events
async fn room_task(connect: proto::ConnectRequest) {
    let res = Room::connect(&connect.url, &connect.token).await;

    if res.is_err() {
        send_response(ResponseMessage::Connect(proto::ConnectResponse {
            success: false,
            room: None,
        }));
        return;
    }

    // Send connect response before listening to events
    let (room, mut events) = res.unwrap();
    let session = room.session();

    send_response(ResponseMessage::Connect(proto::ConnectResponse {
        success: true,
        room: Some(proto::RoomInfo {
            sid: session.sid(),
            name: session.name(),
            local_participant: Some(room.session().local_participant().into()),
            participants: room
                .session()
                .participants()
                .iter()
                .map(|(_, p)| p.clone().into())
                .collect(),
        }),
    }));

    // Listen to events
    tokio::spawn(participant_task(Participant::Local(
        session.local_participant(),
    )));

    while let Some(event) = events.recv().await {
        if let Some(event) = proto::RoomEvent::from(session.sid(), event.clone()) {
            send_response(ResponseMessage::RoomEvent(event));
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
                let track_sid = track.sid();
                match track {
                    RemoteTrackHandle::Video(video_track) => {
                        video_track
                            .rtc_track()
                            .on_frame(Box::new(move |frame, buffer| {
                                // Received a new VideoFrame
                                let handle: FFIHandle = 56;
                                send_response(ResponseMessage::TrackEvent(proto::TrackEvent {
                                    track_sid: track_sid.to_string(),
                                    message: Some(proto::track_event::Message::FrameReceived(
                                        proto::FrameReceived {
                                            frame: Some(frame.into()),
                                            frame_buffer: Some(proto::VideoFrameBuffer::from(
                                                handle, buffer,
                                            )),
                                        },
                                    )),
                                }));
                            }))
                    }
                    RemoteTrackHandle::Audio(audio_track) => {}
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
