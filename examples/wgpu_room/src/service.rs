use crate::{
    logo_track::LogoTrack,
    sine_track::{SineParameters, SineTrack},
};
use livekit::{
    e2ee::{key_provider::*, E2eeOptions, EncryptionType},
    prelude::*,
    SimulateScenario,
};
use parking_lot::Mutex;
use std::sync::Arc;
use tokio::sync::mpsc::{self, error::SendError};

#[derive(Debug)]
pub enum AsyncCmd {
    RoomConnect { url: String, token: String, auto_subscribe: bool, enable_e2ee: bool, key: String },
    RoomDisconnect,
    SimulateScenario { scenario: SimulateScenario },
    ToggleLogo,
    ToggleSine,
    SubscribeTrack { publication: RemoteTrackPublication },
    UnsubscribeTrack { publication: RemoteTrackPublication },
    E2eeKeyRatchet,
    LogStats,
}

#[derive(Debug)]
pub enum UiCmd {
    ConnectResult { result: RoomResult<()> },
    RoomEvent { event: RoomEvent },
}

/// AppService is the "asynchronous" part of our application, where we connect to a room and
/// handle events.
pub struct LkService {
    cmd_tx: mpsc::UnboundedSender<AsyncCmd>,
    ui_rx: mpsc::UnboundedReceiver<UiCmd>,
    handle: tokio::task::JoinHandle<()>,
    inner: Arc<ServiceInner>,
}

struct ServiceInner {
    ui_tx: mpsc::UnboundedSender<UiCmd>,
    room: Mutex<Option<Arc<Room>>>,
}

impl LkService {
    /// Create a new AppService and return a channel that informs the UI of events.
    pub fn new(async_handle: &tokio::runtime::Handle) -> Self {
        let (ui_tx, ui_rx) = mpsc::unbounded_channel();
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();

        let inner = Arc::new(ServiceInner { ui_tx, room: Default::default() });
        let handle = async_handle.spawn(service_task(inner.clone(), cmd_rx));

        Self { cmd_tx, ui_rx, handle, inner }
    }

    pub fn room(&self) -> Option<Arc<Room>> {
        self.inner.room.lock().clone()
    }

    pub fn send(&self, cmd: AsyncCmd) -> Result<(), SendError<AsyncCmd>> {
        self.cmd_tx.send(cmd)
    }

    pub fn try_recv(&mut self) -> Option<UiCmd> {
        self.ui_rx.try_recv().ok()
    }

    #[allow(dead_code)]
    pub async fn close(self) {
        drop(self.cmd_tx);
        let _ = self.handle.await;
    }
}

async fn service_task(inner: Arc<ServiceInner>, mut cmd_rx: mpsc::UnboundedReceiver<AsyncCmd>) {
    struct RunningState {
        room: Arc<Room>,
        logo_track: LogoTrack,
        sine_track: SineTrack,
    }

    let mut running_state = None;

    while let Some(event) = cmd_rx.recv().await {
        match event {
            AsyncCmd::RoomConnect { url, token, auto_subscribe, enable_e2ee, key } => {
                log::info!("connecting to room: {}", url);

                let key_provider =
                    KeyProvider::with_shared_key(KeyProviderOptions::default(), key.into_bytes());
                let e2ee = enable_e2ee
                    .then_some(E2eeOptions { encryption_type: EncryptionType::Gcm, key_provider });

                let mut options = RoomOptions::default();
                options.auto_subscribe = auto_subscribe;
                options.e2ee = e2ee;

                let res = Room::connect(&url, &token, options).await;

                if let Ok((new_room, events)) = res {
                    log::info!("connected to room: {}", new_room.name());
                    tokio::spawn(room_task(inner.clone(), events));

                    let new_room = Arc::new(new_room);
                    running_state = Some(RunningState {
                        room: new_room.clone(),
                        logo_track: LogoTrack::new(new_room.clone()),
                        sine_track: SineTrack::new(new_room.clone(), SineParameters::default()),
                    });

                    // Allow direct access to the room from the UI (Used for sync access)
                    inner.room.lock().replace(new_room);

                    let _ = inner.ui_tx.send(UiCmd::ConnectResult { result: Ok(()) });
                } else if let Err(err) = res {
                    log::error!("failed to connect to room: {:?}", err);
                    let _ = inner.ui_tx.send(UiCmd::ConnectResult { result: Err(err) });
                }
            }
            AsyncCmd::RoomDisconnect => {
                if let Some(state) = running_state.take() {
                    *inner.room.lock() = None;
                    if let Err(err) = state.room.close().await {
                        log::error!("failed to disconnect from room: {:?}", err);
                    }
                }
            }
            AsyncCmd::SimulateScenario { scenario } => {
                if let Some(state) = running_state.as_ref() {
                    if let Err(err) = state.room.simulate_scenario(scenario).await {
                        log::error!("failed to simulate scenario: {:?}", err);
                    }
                }
            }
            AsyncCmd::ToggleLogo => {
                if let Some(state) = running_state.as_mut() {
                    if state.logo_track.is_published() {
                        state.logo_track.unpublish().await.unwrap();
                    } else {
                        state.logo_track.publish().await.unwrap();
                    }
                }
            }
            AsyncCmd::ToggleSine => {
                if let Some(state) = running_state.as_mut() {
                    if state.sine_track.is_published() {
                        state.sine_track.unpublish().await.unwrap();
                    } else {
                        state.sine_track.publish().await.unwrap();
                    }
                }
            }
            AsyncCmd::SubscribeTrack { publication } => {
                publication.set_subscribed(true);
            }
            AsyncCmd::UnsubscribeTrack { publication } => {
                publication.set_subscribed(false);
            }
            AsyncCmd::E2eeKeyRatchet => {
                if let Some(state) = running_state.as_ref() {
                    let e2ee_manager = state.room.e2ee_manager();
                    if let Some(key_provider) = e2ee_manager.key_provider() {
                        key_provider.ratchet_shared_key(0);
                    }
                }
            }
            AsyncCmd::LogStats => {
                if let Some(state) = running_state.as_ref() {
                    for (_, publication) in state.room.local_participant().track_publications() {
                        if let Some(track) = publication.track() {
                            log::info!(
                                "track stats: LOCAL {:?} {:?}",
                                track.sid(),
                                track.get_stats().await,
                            );
                        }
                    }

                    for (_, participant) in state.room.remote_participants() {
                        for (_, publication) in participant.track_publications() {
                            if let Some(track) = publication.track() {
                                log::info!(
                                    "track stats: {:?} {:?} {:?}",
                                    participant.identity(),
                                    track.sid(),
                                    track.get_stats().await,
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Task basically used to forward room events to the UI.
/// It will automatically close when the room is disconnected.
async fn room_task(inner: Arc<ServiceInner>, mut events: mpsc::UnboundedReceiver<RoomEvent>) {
    while let Some(event) = events.recv().await {
        let _ = inner.ui_tx.send(UiCmd::RoomEvent { event });
    }
}
