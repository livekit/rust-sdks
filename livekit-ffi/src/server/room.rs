use crate::server::FfiServer;
use crate::{proto, FfiHandleId, FfiResult};
use livekit::prelude::*;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;

pub struct FfiRoom {
    room: Arc<Room>,
    handle: JoinHandle<()>,
    close_tx: oneshot::Sender<()>,
}

impl FfiRoom {
    pub async fn connect(
        server: &'static FfiServer,
        connect: proto::ConnectRequest,
    ) -> FfiResult<proto::RoomInfo> {
        let (room, events) = Room::connect(&connect.url, &connect.token).await?;
        let room = Arc::new(room);
        let (close_tx, close_rx) = oneshot::channel();
        let next_id = server.next_id() as FfiHandleId;

        let handle =
            server
                .async_runtime
                .spawn(room_task(server, room.clone(), next_id, events, close_rx));
        let room_info = proto::RoomInfo::from_room(next_id, &room);

        let ffi_room = Self {
            room: room.clone(),
            handle,
            close_tx,
        };

        server.ffi_handles().insert(next_id, Box::new(ffi_room));
        server.rooms().lock().insert(room.sid(), next_id);

        Ok(room_info)
    }

    pub async fn close(self) {
        let _ = self.room.close().await;
        let _ = self.close_tx.send(());
        let _ = self.handle.await;
    }

    pub fn room(&self) -> &Arc<Room> {
        &self.room
    }
}

async fn room_task(
    server: &'static FfiServer,
    room: Arc<Room>,
    room_handle: FfiHandleId,
    mut events: mpsc::UnboundedReceiver<livekit::RoomEvent>,
    mut close_rx: oneshot::Receiver<()>,
) {
    server
        .async_runtime
        .spawn(participant_task(Participant::Local(
            room.local_participant(),
        )));

    loop {
        tokio::select! {
            Some(event) = events.recv() => {
                if let Some(event) = proto::RoomEvent::from(room_handle, event.clone()) {
                    let _ = server.send_event(proto::ffi_event::Message::RoomEvent(event));
                }

                match event {
                    RoomEvent::ParticipantConnected(p) => {
                        server.async_runtime.spawn(participant_task(Participant::Remote(p)));
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
