use crate::proto;
use crate::server::FfiServer;
use livekit::prelude::*;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;

pub struct FfiRoom {
    room: Room,
    handle: JoinHandle<()>,
    close_tx: oneshot::Sender<()>,
}

impl FfiRoom {
    pub async fn connect(
        server: &'static FfiServer,
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
    server: &'static FfiServer,
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
