use crate::proto;
use lazy_static::lazy_static;
use livekit::prelude::*;
use livekit::webrtc::media_stream::OnFrameHandler;
use parking_lot::{Mutex, RwLock};
use prost::Message;
use std::any::Any;
use std::collections::HashMap;
use std::panic;
use std::slice;
use std::sync::atomic::AtomicU64;
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

pub type FFIHandleId = usize;
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
    //
    // NOTE: For VideoBuffers, we always store the enum type VideoFrameBuffer
    ffi_owned: RwLock<HashMap<FFIHandleId, FFIHandle>>,
    next_handle_id: AtomicU64, // FFIHandleId
    next_async_id: AtomicU64,

    rooms: RwLock<HashMap<RoomSid, Room>>,
    async_runtime: tokio::runtime::Runtime,
    initialized: AtomicBool,
    config: Mutex<Option<FFIConfig>>,
}

impl Default for FFIServer {
    fn default() -> Self {
        Self {
            ffi_owned: RwLock::new(HashMap::new()),
            next_handle_id: AtomicU64::new(1), // 0 is considered invalid
            next_async_id: AtomicU64::new(1),
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
        self.next_handle_id.fetch_add(1, Ordering::SeqCst) as FFIHandleId
    }

    pub fn next_async_id(&self) -> u64 {
        self.next_async_id.fetch_add(1, Ordering::SeqCst)
    }

    pub fn insert_handle(&self, handle_id: FFIHandleId, handle: FFIHandle) {
        self.ffi_owned.write().insert(handle_id, handle);
    }

    pub fn release_handle(&self, handle_id: FFIHandleId) -> Option<FFIHandle> {
        self.ffi_owned.write().remove(&handle_id)
    }

    pub fn send_event(
        &self,
        message: proto::ffi_event::Message,
        async_id: Option<u64>,
    ) -> Result<(), FFIError> {
        if !self.initialized.load(Ordering::SeqCst) {
            Err(FFIError::NotConfigured)?
        }

        let message = proto::FfiEvent {
            async_id,
            message: Some(message),
        }
        .encode_to_vec();

        let callback_fn = self.config.lock().as_ref().unwrap().callback_fn;
        if let Err(err) = panic::catch_unwind(|| unsafe {
            callback_fn(message.as_ptr(), message.len());
        }) {
            eprintln!("panic when sending ffi event: {:?}", err);
            Err(FFIError::CallbackFailed)?
        }

        Ok(())
    }

    pub fn handle_request(&self, message: proto::ffi_request::Message) -> proto::FfiResponse {
        const NOOP: proto::FfiResponse = proto::FfiResponse {
            async_id: None,
            message: None,
        };

        if let proto::ffi_request::Message::Configure(ref init) = message {
            self.initialized.store(true, Ordering::SeqCst);
            *self.config.lock() = Some(FFIConfig {
                callback_fn: unsafe { std::mem::transmute(init.event_callback_ptr) },
            });
        }

        if !self.initialized.load(Ordering::SeqCst) {
            eprintln!("The FFIServer isn't initialized, we can't handle the request.");
            return NOOP;
        }

        match message {
            proto::ffi_request::Message::AsyncConnect(connect) => {
                let async_id = self.next_async_id();
                self.async_runtime.spawn(room_task(async_id, connect));
                return proto::FfiResponse {
                    async_id: Some(async_id),
                    message: None,
                };
            }
            proto::ffi_request::Message::ToI420(to_i420) => {
                let mut handle_id = 0; // Invalid handle
                let buffer = self.release_handle(to_i420.buffer.unwrap().id as FFIHandleId);
                if let Some(buffer) = buffer {
                    if let Ok(buffer) = buffer.downcast::<VideoFrameBuffer>() {
                        handle_id = self.next_handle_id();
                        self.insert_handle(handle_id, Box::new(buffer.to_i420()));
                    }
                }

                let res = proto::ToI420Response {
                    new_buffer: Some(proto::FfiHandleId {
                        id: handle_id as u64,
                    }),
                };

                return proto::FfiResponse {
                    async_id: None,
                    message: Some(proto::ffi_response::Message::ToI420(res)),
                };
            }
            proto::ffi_request::Message::ToArgb(to_argb) => {
                let ffi_owned = self.ffi_owned.read();
                let buffer = ffi_owned.get(&(to_argb.buffer.unwrap().id as FFIHandleId));

                if let Some(buffer) = buffer {
                    if let Some(buffer) = buffer.downcast_ref::<VideoFrameBuffer>() {
                        let dst_buf = unsafe {
                            slice::from_raw_parts_mut(
                                to_argb.dst_ptr as *mut u8,
                                (to_argb.dst_stride * to_argb.dst_height) as usize,
                            )
                        };

                        if let Err(err) = buffer.to_argb(
                            proto::VideoFormatType::from_i32(to_argb.dst_format)
                                .unwrap()
                                .into(),
                            dst_buf,
                            to_argb.dst_stride,
                            to_argb.dst_width,
                            to_argb.dst_height,
                        ) {
                            eprintln!("failed to convert videoframe to argb: {:?}", err);
                        }
                    }
                }
            }
            _ => {}
        }

        NOOP
    }
}

/// This function is threadsafe, this is useful to run synchronous requests in another thread
#[no_mangle]
pub extern "C" fn livekit_ffi_request(
    data: *const u8,
    len: usize,
    data_ptr: *mut *const u8,
    data_len: *mut usize,
) -> FFIHandleId {
    let data = unsafe { slice::from_raw_parts(data, len) };
    let res = proto::FfiRequest::decode(data);
    if let Err(ref err) = res {
        eprintln!("failed to decode FfiRequest: {:?}", err);
        return 0;
    }

    let res = FFI_SERVER.handle_request(res.unwrap().message.unwrap());

    let buf = res.encode_to_vec();
    unsafe {
        *data_ptr = buf.as_ptr();
        *data_len = buf.len();
    }

    let handle_id = FFI_SERVER.next_handle_id();
    FFI_SERVER.insert_handle(handle_id, Box::new(buf));
    handle_id
}

// Free memory
#[no_mangle]
pub extern "C" fn livekit_ffi_drop_handle(handle_id: FFIHandleId) -> bool {
    FFI_SERVER.release_handle(handle_id).is_some()
}

// Connect a listen to Room events
async fn room_task(async_id: u64, connect: proto::ConnectRequest) {
    let res = Room::connect(&connect.url, &connect.token).await;

    if res.is_err() {
        let _ = FFI_SERVER.send_event(
            proto::ffi_event::Message::ConnectEvent(proto::ConnectEvent {
                success: false,
                room: None,
            }),
            Some(async_id),
        );
        return;
    }

    // Send connect response before listening to events
    let (room, mut events) = res.unwrap();
    let session = room.session();

    let _ = FFI_SERVER.send_event(
        proto::ffi_event::Message::ConnectEvent(proto::ConnectEvent {
            success: true,
            room: Some(proto::RoomInfo {
                sid: session.sid(),
                name: session.name(),
                metadata: session.metadata(),
                local_participant: Some((&room.session().local_participant()).into()),
                participants: room
                    .session()
                    .participants()
                    .iter()
                    .map(|(_, p)| p.into())
                    .collect(),
            }),
        }),
        Some(async_id),
    );

    // Listen to events
    tokio::spawn(participant_task(Participant::Local(
        session.local_participant(),
    )));

    while let Some(event) = events.recv().await {
        if let Some(event) = proto::RoomEvent::from(session.sid(), event.clone()) {
            let _ = FFI_SERVER.send_event(proto::ffi_event::Message::RoomEvent(event), None);
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
    // TODO(theomonnom): Should I use VideoSinkInfo here?
    Box::new(move |frame, buffer| {
        let handle_id = FFI_SERVER.next_handle_id();
        let proto_buffer = proto::VideoFrameBufferInfo::from(handle_id, &buffer);
        FFI_SERVER.insert_handle(handle_id, Box::new(buffer));

        let _ = FFI_SERVER.send_event(
            proto::ffi_event::Message::TrackEvent(proto::TrackEvent {
                track_sid: track_sid.to_string(),
                message: Some(proto::track_event::Message::FrameReceived(
                    proto::FrameReceived {
                        frame: Some(frame.into()),
                        frame_buffer: Some(proto_buffer),
                    },
                )),
            }),
            None,
        );
    })
}
