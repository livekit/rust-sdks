use std::{
    collections::HashMap,
    future::ready,
    sync::Arc,
    time::{Duration, Instant},
};

use futures::StreamExt;
use livekit::{
    prelude::*,
    webrtc::{
        audio_source::{native::NativeAudioSource, AudioSourceOptions},
        audio_stream::native::NativeAudioStream,
        prelude::{RtcAudioSource, RtcVideoSource, VideoResolution},
        video_source::native::NativeVideoSource,
        video_stream::native::NativeVideoStream,
    },
};
use load_tester::{
    media::{self, Ladder, Publishers, SAMPLE_RATE},
    record::{
        now_ms, EventRecord, JoinOutcome, JoinRecord, MediaKind, ParticipantEvent, ParticipantId,
        Record, Settings, SubscriptionRecord, UplinkRecord, WindowRecord,
    },
    window::{InboundTracker, Observation, StatsByTrack, UplinkTracker, VideoExpectation},
};
use tokio::{
    sync::{mpsc, watch},
    task::{self, JoinSet},
    time::{interval_at, timeout, MissedTickBehavior},
};

use crate::WINDOW;

const BASELINE_AFTER_FIRST_FRAME: Duration = Duration::from_millis(250);

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
    connected_at: Instant,
    published_at: HashMap<TrackSid, Instant>,
    subs: HashMap<TrackSid, Subscription>,
    uplinks: Vec<Uplink>,
    first_frames: JoinSet<Option<Instant>>,
}

struct Subscription {
    rtc_id: String,
    kind: MediaKind,
    publ: ParticipantId,
    tracker: InboundTracker,
    sink: task::Id,
}

struct Uplink {
    rtc_id: String,
    kind: MediaKind,
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
        let connected_at = Instant::now();
        let connect_ms = connected_at.duration_since(started).as_millis() as u32;
        let mut participant = Self {
            id,
            ctx: ctx.clone(),
            room,
            events,
            connected_at,
            published_at: HashMap::new(),
            subs: HashMap::new(),
            uplinks: Vec::new(),
            first_frames: JoinSet::new(),
        };
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
        let uplink = |rtc_id, kind| Uplink { rtc_id, kind, tracker: Default::default() };
        self.uplinks = vec![
            uplink(mic.rtc_track().id(), MediaKind::Audio),
            uplink(camera.rtc_track().id(), MediaKind::Video),
        ];
        let local = self.room.local_participant();
        let audio_options = media::audio_options(&self.ctx.settings.audio);
        local.publish_track(LocalTrack::Audio(mic), audio_options).await?;
        local.publish_track(LocalTrack::Video(camera), media::video_options(&profile)).await?;
        self.ctx.publishers.add(self.id, audio, video);
        Ok(())
    }

    async fn serve(mut self) {
        let mut stop = self.ctx.stop.clone();
        let mut tick = interval_at(tokio::time::Instant::now() + WINDOW, WINDOW);
        tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                _ = stop.changed() => break,
                event = self.events.recv() => match event {
                    Some(event) => if !self.on_event(event).await { break },
                    None => break,
                },
                Some(done) = self.first_frames.join_next_with_id() => {
                    if let Ok((sink, Some(frame))) = done {
                        if let Some(sub) = self.subs.values_mut().find(|sub| sub.sink == sink) {
                            sub.tracker.first_frame(frame);
                            tick.reset_after(BASELINE_AFTER_FIRST_FRAME);
                        }
                    }
                }
                _ = tick.tick() => self.sample().await,
            }
        }
        self.close().await;
    }

    async fn close(&mut self) {
        self.first_frames.shutdown().await;
        self.ctx.publishers.remove(self.id);
        let _ = self.room.close().await;
    }

    async fn on_event(&mut self, event: RoomEvent) -> bool {
        match event {
            RoomEvent::TrackPublished { publication, .. } => {
                self.published_at.insert(publication.sid(), Instant::now());
            }
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
        let sid = track.sid();
        let since = self.published_at.remove(&sid).unwrap_or(self.connected_at);
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
            MediaKind::Audio => InboundTracker::audio(since),
            MediaKind::Video => {
                let tile_height = settings.tile_height();
                if tile_height != settings.video.height {
                    let tile_width = tile_height * settings.video.width / settings.video.height;
                    publication.update_video_dimensions(TrackDimension(tile_width, tile_height));
                }
                let expect = VideoExpectation { ladder: self.ctx.ladder.clone(), tile_height };
                InboundTracker::video(since, expect)
            }
        };
        let wait = Duration::from_secs((settings.settle_s + settings.hold_s).into());
        let rtc_id = track.rtc_track().id();
        let sink = self.first_frames.spawn(first_frame(track, wait)).id();
        self.subs.insert(sid, Subscription { rtc_id, kind, publ, tracker, sink });
    }

    async fn sample(&mut self) {
        let Ok(stats) = self.room.get_stats().await else {
            return;
        };
        let tracks = StatsByTrack::new(&stats);
        let now = Instant::now();
        for (sid, sub) in &mut self.subs {
            let (at, track) = (now_ms(), sid.to_string());
            let record = match sub.tracker.observe(now, tracks.inbound(&sub.rtc_id)) {
                Observation::Nothing => continue,
                Observation::FirstMedia { frame, ttff_ms, decoder } => {
                    Record::Subscription(SubscriptionRecord {
                        at: at.saturating_sub(frame.elapsed().as_millis() as u64),
                        sub: self.id,
                        publ: sub.publ,
                        track,
                        kind: sub.kind,
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
        for uplink in &mut self.uplinks {
            let Some(w) = uplink.tracker.observe(now, tracks.outbound(&uplink.rtc_id)) else {
                continue;
            };
            let record = Record::Uplink(UplinkRecord {
                at: now_ms(),
                dur_ms: w.dur_ms,
                id: self.id,
                kind: uplink.kind,
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

async fn first_frame(track: RemoteTrack, wait: Duration) -> Option<Instant> {
    let frame = async {
        match &track {
            // playout pulls zeroed frames before the first decode, and the tester's tone is never all zeros
            RemoteTrack::Audio(audio) => {
                NativeAudioStream::new(audio.rtc_track(), SAMPLE_RATE as i32, 1)
                    .any(|frame| ready(frame.data.iter().any(|&s| s != 0)))
                    .await
            }
            RemoteTrack::Video(video) => {
                NativeVideoStream::new(video.rtc_track()).next().await.is_some()
            }
        }
    };
    let arrived = timeout(wait, frame).await.unwrap_or(false);
    arrived.then(Instant::now)
}
