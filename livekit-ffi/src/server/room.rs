use crate::proto;
use crate::server::FFIServer;
use futures_util::stream::StreamExt;
use livekit::prelude::*;
use livekit::webrtc::video_stream::native::NativeVideoStream;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;

pub struct FFIRoom {
    room: Room,
    handle: JoinHandle<()>,
    close_tx: oneshot::Sender<()>,
}

impl FFIRoom {
    pub async fn connect(
        server: &'static FFIServer,
        connect: proto::ConnectRequest,
    ) -> Result<Self, RoomError> {
        let (room, events) = Room::connect(&connect.url, &connect.token).await?;
        let (close_tx, close_rx) = oneshot::channel();
        let handle = tokio::spawn(room_task(server, room.session(), events, close_rx));

        Ok(Self {
            room,
            handle,
            close_tx,
        })
    }

    pub async fn close(self) {
        self.room.close().await;
        let _ = self.close_tx.send(());
        let _ = self.handle.await;
    }

    pub fn session(&self) -> RoomSession {
        self.room.session()
    }
}

async fn room_task(
    server: &'static FFIServer,
    session: RoomSession,
    mut events: mpsc::UnboundedReceiver<livekit::RoomEvent>,
    mut close_rx: oneshot::Receiver<()>,
) {
    tokio::spawn(participant_task(Participant::Local(
        session.local_participant(),
    )));

    loop {
        tokio::select! {
            Some(event) = events.recv() => {
                if let Some(event) = proto::RoomEvent::from(session.sid(), event.clone()) {
                    let _ = server.send_event(proto::ffi_event::Message::RoomEvent(event));
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
}

async fn participant_task(participant: Participant) {
    let mut participant_events = participant.register_observer();
    while let Some(_event) = participant_events.recv().await {
        // TODO(theomonnom): convert event to proto
    }
}

async fn video_frame_task(
    server: &'static FFIServer,
    track_sid: TrackSid,
    mut stream: NativeVideoStream,
) {
    while let Some(frame) = stream.next().await {
        let handle_id = server.next_id();
        let frame_info = proto::VideoFrameInfo::from(&frame);
        let buffer_info = proto::VideoFrameBufferInfo::from(handle_id, &frame.buffer);

        server
            .ffi_handles()
            .write()
            .insert(handle_id, Box::new(frame.buffer));

        // Send the received frame to the FFI language.
        /*let _ = server.send_event(proto::ffi_event::Message::TrackEvent(proto::TrackEvent {
            track_sid: track_sid.to_string(),
            message: Some(proto::track_event::Message::FrameReceived(
                proto::FrameReceived {
                    frame: Some(frame_info),
                    buffer: Some(buffer_info),
                },
            )),
        }));*/
    }
}
