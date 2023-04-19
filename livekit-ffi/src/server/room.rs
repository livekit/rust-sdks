use crate::server::FFIServer;
use futures_util::stream::StreamExt;
use livekit::prelude::*;
use livekit::webrtc::video_stream::native::NativeVideoStream;
use tokio::sync::{mpsc, oneshot};
use crate::proto;

pub async fn create_room(
    server: &'static FFIServer,
    async_id: u64,
    connect: proto::ConnectRequest,
) {
    let res = Room::connect(&connect.url, &connect.token).await;
    if let Err(err) = &res {
        // Failed to connect to the room
        let _ = server.send_event(
            proto::ffi_event::Message::ConnectEvent(proto::ConnectEvent {
                success: false,
                room: None,
            }),
            Some(async_id),
        );
        return;
    }

    let (room, events) = res.unwrap();
    let session = room.session();

    // Successfully connected to the room
    let _ = server.send_event(
        proto::ffi_event::Message::ConnectEvent(proto::ConnectEvent {
            success: true,
            room: Some((&session).into()),
        }),
        Some(async_id),
    );

    // Add the room to the server and listen to the incoming events
    let (close_tx, close_rx) = oneshot::channel();
    let room_handle = tokio::spawn(room_task(server, room, events, close_rx));
    server.add_room(session.sid(), (room_handle, close_tx));
}

async fn room_task(
    server: &'static FFIServer,
    room: Room,
    mut events: mpsc::UnboundedReceiver<livekit::RoomEvent>,
    mut close_rx: oneshot::Receiver<()>,
) {
    let session = room.session();

    tokio::spawn(participant_task(Participant::Local(
        session.local_participant(),
    )));

    loop {
        tokio::select! {
            Some(event) = events.recv() => {
                if let Some(event) = proto::RoomEvent::from(session.sid(), event.clone()) {
                    let _ = server.send_event(proto::ffi_event::Message::RoomEvent(event), None);
                }

                match event {
                    RoomEvent::ParticipantConnected(p) => {
                        tokio::spawn(participant_task(Participant::Remote(p)));
                    }
                    RoomEvent::TrackSubscribed {
                        track,
                        publication: _,
                        participant: _,
                    } => {
                        if let RemoteTrack::Video(video_track) = track {
                            let video_stream = NativeVideoStream::new(video_track.rtc_track());
                            tokio::spawn(video_frame_task(server, video_track.sid(), video_stream));
                        }
                    }
                    _ => {}
                }
            },
            _ = &mut close_rx => {
                break;
            }
        };
    }

    room.close().await;
}

async fn participant_task(participant: Participant) {
    let mut participant_events = participant.register_observer();
    while let Some(event) = participant_events.recv().await {
        // TODO(theomonnom): convert event to proto
    }
}

async fn video_frame_task(
    server: &'static FFIServer,
    track_sid: TrackSid,
    mut stream: NativeVideoStream,
) {
    while let Some(frame) = stream.next().await {
        let handle_id = server.next_handle_id();
        let frame_info = proto::VideoFrameInfo::from(&frame);
        let buffer_info = proto::VideoFrameBufferInfo::from(handle_id, &frame.buffer);
        server.insert_handle(handle_id, Box::new(frame.buffer));

        // Send the received frame to the FFI language.
        let _ = server.send_event(
            proto::ffi_event::Message::TrackEvent(proto::TrackEvent {
                track_sid: track_sid.to_string(),
                message: Some(proto::track_event::Message::FrameReceived(
                    proto::FrameReceived {
                        frame: Some(frame_info),
                        buffer: Some(buffer_info),
                    },
                )),
            }),
            None,
        );
    }
}
