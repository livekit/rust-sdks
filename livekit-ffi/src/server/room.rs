use crate::proto::{self};
use crate::server::FFIServer;
use livekit::prelude::*;
use tokio::sync::{mpsc, oneshot};

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
                        if let RemoteTrackHandle::Video(video_track) = track {
                            let rtc_track = video_track.rtc_track();
                            rtc_track.on_frame(on_video_frame(video_track.sid()));
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
        // TODO convert event to proto
    }
}

fn on_video_frame(server: &'static FFIServer, track_sid: TrackSid) -> OnFrameHandler {
    // TODO(theomonnom): Should I use VideoSinkInfo here? (It'll help to have a more verbose
    // lifetime)

    Box::new(move |frame, buffer| {
        // Frame received, create a new FFIHandle from the video buffer.
        let handle_id = server.next_handle_id();
        let proto_buffer = proto::VideoFrameBufferInfo::from(handle_id, &buffer);
        server.insert_handle(handle_id, Box::new(buffer));

        // Send the received frame to the FFI language.
        let _ = server.send_event(
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
