use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use livekit::{
    prelude::*,
    webrtc::{
        audio_source::{native::NativeAudioSource, AudioSourceOptions},
        prelude::{RtcAudioSource, RtcVideoSource, VideoResolution},
        video_source::native::NativeVideoSource,
    },
};
use load_tester::{
    media::{self, Ladder, Publishers, SAMPLE_RATE},
    record::{
        now_ms, EventRecord, JoinOutcome, JoinRecord, MediaKind, ParticipantEvent, ParticipantId,
        Record, Settings, SubscriptionRecord, UplinkRecord, WindowRecord,
    },
    window::{InboundTracker, Observation, UplinkTracker, VideoExpectation},
};
use tokio::{
    sync::{mpsc, watch},
    time::{interval, MissedTickBehavior},
};

use crate::WINDOW;

const PROBE: Duration = Duration::from_millis(500);
const PROBES_PER_WINDOW: u32 = (WINDOW.as_millis() / PROBE.as_millis()) as u32;

pub struct Ctx {
    pub settings: Settings,
    pub ladder: Ladder,
    pub publishers: Arc<Publishers>,
    pub out: mpsc::Sender<Record>,
    pub stop: watch::Receiver<bool>,
}

pub struct Participant {
    id: ParticipantId,
    ctx: Arc<Ctx>,
    room: Room,
    events: mpsc::UnboundedReceiver<RoomEvent>,
    subs: HashMap<TrackSid, Subscription>,
    uplinks: Vec<Uplink>,
}

struct Subscription {
    track: RemoteTrack,
    publ: ParticipantId,
    tracker: InboundTracker,
}

struct Uplink {
    track: LocalTrack,
    tracker: UplinkTracker,
}

impl Participant {
    pub async fn run(id: ParticipantId, token: String, ctx: Arc<Ctx>) {
        let joined = Self::join(id, &token, &ctx).await;
        let outcome = match &joined {
            Ok((_, connect_ms)) => JoinOutcome::Joined { connect_ms: *connect_ms },
            Err(e) => JoinOutcome::Failed { error: e.to_string() },
        };
        if ctx.out.send(Record::Join(JoinRecord { at: now_ms(), id, outcome })).await.is_err() {
            return;
        }
        if let Ok((participant, _)) = joined {
            participant.serve().await;
        }
    }

    async fn join(id: ParticipantId, token: &str, ctx: &Arc<Ctx>) -> anyhow::Result<(Self, u32)> {
        let mut options = RoomOptions::default();
        options.dynacast = true;
        let started = Instant::now();
        let (room, events) = Room::connect(&ctx.settings.url, token, options).await?;
        let connect_ms = started.elapsed().as_millis() as u32;
        let mut participant =
            Self { id, ctx: ctx.clone(), room, events, subs: HashMap::new(), uplinks: Vec::new() };
        if ctx.settings.is_publisher(id) {
            if let Err(e) = participant.publish().await {
                participant.close().await;
                return Err(e.into());
            }
        }
        Ok((participant, connect_ms))
    }

    async fn publish(&mut self) -> RoomResult<()> {
        let profile = self.ctx.settings.video;
        let audio = NativeAudioSource::new(AudioSourceOptions::default(), SAMPLE_RATE, 1, 0);
        let video = NativeVideoSource::new(
            VideoResolution { width: profile.width, height: profile.height },
            false,
        );
        let mic = LocalAudioTrack::create_audio_track("mic", RtcAudioSource::Native(audio.clone()));
        let camera =
            LocalVideoTrack::create_video_track("camera", RtcVideoSource::Native(video.clone()));
        let local = self.room.local_participant();
        let audio_options = media::audio_options(&self.ctx.settings.audio);
        local.publish_track(LocalTrack::Audio(mic.clone()), audio_options).await?;
        local
            .publish_track(LocalTrack::Video(camera.clone()), media::video_options(&profile))
            .await?;
        self.ctx.publishers.add(self.id, audio, video);
        self.uplinks = vec![
            Uplink { track: LocalTrack::Audio(mic), tracker: Default::default() },
            Uplink { track: LocalTrack::Video(camera), tracker: Default::default() },
        ];
        Ok(())
    }

    async fn serve(mut self) {
        let mut stop = self.ctx.stop.clone();
        let mut probe = interval(PROBE);
        probe.set_missed_tick_behavior(MissedTickBehavior::Delay);
        let mut probes = 0u32;
        loop {
            tokio::select! {
                _ = stop.changed() => break,
                event = self.events.recv() => match event {
                    Some(event) => if !self.on_event(event).await { break },
                    None => break,
                },
                _ = probe.tick() => {
                    probes += 1;
                    self.sample(probes.is_multiple_of(PROBES_PER_WINDOW)).await;
                }
            }
        }
        self.close().await;
    }

    async fn close(&mut self) {
        self.ctx.publishers.remove(self.id);
        let _ = self.room.close().await;
    }

    async fn on_event(&mut self, event: RoomEvent) -> bool {
        match event {
            RoomEvent::TrackSubscribed { track, publication, participant } => {
                self.subscribed(track, &publication, &participant);
            }
            RoomEvent::TrackUnsubscribed { track, .. } => {
                self.subs.remove(&track.sid());
            }
            RoomEvent::ConnectionQualityChanged { quality, participant } => {
                if let Some(about) = ParticipantId::parse(&participant.identity().0) {
                    let event = ParticipantEvent::ServerQuality { about, quality: quality.into() };
                    self.emit(event).await;
                }
            }
            RoomEvent::Reconnecting => self.emit(ParticipantEvent::Reconnecting).await,
            RoomEvent::Disconnected { reason } => {
                self.emit(ParticipantEvent::Disconnected { reason: format!("{reason:?}") }).await;
                return false;
            }
            _ => {}
        }
        true
    }

    fn subscribed(
        &mut self,
        track: RemoteTrack,
        publication: &RemoteTrackPublication,
        participant: &RemoteParticipant,
    ) {
        let settings = &self.ctx.settings;
        let kind = MediaKind::from(track.kind());
        let wanted = ParticipantId::parse(&participant.identity().0)
            .filter(|&publ| settings.wants(self.id, publ, kind));
        let Some(publ) = wanted else {
            if kind == MediaKind::Video {
                keep_subscribed_but_unforwarded(publication);
            }
            return;
        };
        let tracker = match kind {
            MediaKind::Audio => InboundTracker::audio(Instant::now()),
            MediaKind::Video => {
                let tile_height = settings.tile_height();
                if tile_height != settings.video.height {
                    let tile_width = tile_height * settings.video.width / settings.video.height;
                    publication.update_video_dimensions(TrackDimension(tile_width, tile_height));
                }
                let expect = VideoExpectation { ladder: self.ctx.ladder.clone(), tile_height };
                InboundTracker::video(Instant::now(), expect)
            }
        };
        self.subs.insert(track.sid(), Subscription { track, publ, tracker });
    }

    async fn sample(&mut self, everything: bool) {
        for (sid, sub) in &mut self.subs {
            if !everything && !sub.tracker.waiting() {
                continue;
            }
            let stats = sub.track.get_stats().await.unwrap_or_default();
            let (at, track) = (now_ms(), sid.to_string());
            let record = match sub.tracker.observe(Instant::now(), &stats) {
                Observation::Nothing => continue,
                Observation::FirstMedia { ttff_ms, decoder } => {
                    Record::Subscription(SubscriptionRecord {
                        at,
                        sub: self.id,
                        publ: sub.publ,
                        track,
                        kind: sub.track.kind().into(),
                        ttff_ms,
                        decoder,
                    })
                }
                Observation::Window { dur_ms, rtt_ms, media } => Record::Window(WindowRecord {
                    at,
                    dur_ms,
                    sub: self.id,
                    publ: sub.publ,
                    track,
                    rtt_ms,
                    media,
                }),
            };
            if self.ctx.out.send(record).await.is_err() {
                return;
            }
        }
        if !everything {
            return;
        }
        for uplink in &mut self.uplinks {
            let stats = uplink.track.get_stats().await.unwrap_or_default();
            let Some(w) = uplink.tracker.observe(Instant::now(), &stats) else {
                continue;
            };
            let record = Record::Uplink(UplinkRecord {
                at: now_ms(),
                dur_ms: w.dur_ms,
                id: self.id,
                kind: uplink.track.kind().into(),
                layers: w.layers,
            });
            if self.ctx.out.send(record).await.is_err() {
                return;
            }
        }
    }

    async fn emit(&self, event: ParticipantEvent) {
        let record = Record::Event(EventRecord { at: now_ms(), id: self.id, event });
        let _ = self.ctx.out.send(record).await;
    }
}

fn keep_subscribed_but_unforwarded(publication: &RemoteTrackPublication) {
    publication.set_enabled(false);
}
