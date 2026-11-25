use std::{
    collections::VecDeque, env, future::Future, io::ErrorKind, path::Path, sync::Arc,
    time::Duration,
};

use anyhow::{bail, ensure, Context, Result};
use clap::Args;
use futures_util::StreamExt;
use livekit::prelude::*;
use livekit_token::{AccessToken, VideoGrants};
use tokio::{
    signal,
    time::{self, Instant},
};

use crate::messages::{self, Receiver};
use crate::{
    codec::{Budget, DecodedFrame, Encoder, FrameRate},
    polygon::DisplayTriangle,
    timing::{self, now_us, Trace},
    video::{Sink, Source},
    viewer::Window,
    Meter, Publisher, Subscriber,
};

/// Room connection flags shared by the publisher and subscriber.
#[derive(Args)]
pub(crate) struct ConnectionOptions {
    /// LiveKit server URL.
    #[arg(long, env = "LIVEKIT_URL")]
    url: Option<String>,
    /// LiveKit API key.
    #[arg(long, env = "LIVEKIT_API_KEY", hide_env_values = true)]
    api_key: Option<String>,
    /// LiveKit API secret.
    #[arg(long, env = "LIVEKIT_API_SECRET", hide_env_values = true)]
    api_secret: Option<String>,
    /// Room used when signing a participant token.
    #[arg(long, default_value = "vector-video")]
    room_name: String,
    /// Participant identity. Defaults to robot-camera for publishing or operator for subscribing.
    #[arg(long)]
    identity: Option<String>,
}

/// Selects the participant identity default and Data Track grants.
pub(crate) enum Role {
    Publisher,
    Subscriber,
}

/// The server URL and participant token needed to join a room.
pub(crate) struct RoomConnection<'a> {
    url: &'a str,
    token: String,
}

impl ConnectionOptions {
    /// Signs a token with the configured key pair or uses an existing participant token.
    pub(crate) fn resolve(&self, role: Role) -> Result<RoomConnection<'_>> {
        let url = self
            .url
            .as_deref()
            .filter(|url| !url.is_empty())
            .context("Set --url or LIVEKIT_URL")?;
        let token = match (&self.api_key, &self.api_secret) {
            (Some(key), Some(secret)) => {
                ensure!(!key.is_empty(), "Set a nonempty --api-key or LIVEKIT_API_KEY");
                ensure!(!secret.is_empty(), "Set a nonempty --api-secret or LIVEKIT_API_SECRET");
                let (default_identity, publisher) = match role {
                    Role::Publisher => ("robot-camera", true),
                    Role::Subscriber => ("operator", false),
                };
                let identity = self.identity.as_deref().unwrap_or(default_identity);
                AccessToken::with_api_key(key, secret)
                    .with_identity(identity)
                    .with_grants(VideoGrants {
                        room_join: true,
                        room: self.room_name.clone(),
                        can_publish: Some(false),
                        can_publish_data: Some(true),
                        can_subscribe: Some(!publisher),
                        ..Default::default()
                    })
                    .to_jwt()
                    .context("Could not sign the LiveKit room token")?
            }
            (Some(_), None) => bail!("Set --api-secret or LIVEKIT_API_SECRET"),
            (None, Some(_)) => bail!("Set --api-key or LIVEKIT_API_KEY"),
            (None, None) => env::var("LIVEKIT_TOKEN")
                .ok()
                .filter(|token| !token.is_empty())
                .context("Set LIVEKIT_API_KEY and LIVEKIT_API_SECRET, or provide LIVEKIT_TOKEN")?,
        };
        Ok(RoomConnection { url, token })
    }
}

impl RoomConnection<'_> {
    /// Joins the room with manual subscription to the selected data track.
    async fn connect(self) -> Result<(Room, tokio::sync::mpsc::UnboundedReceiver<RoomEvent>)> {
        let mut options = RoomOptions::default();
        options.auto_subscribe = false;
        Ok(Room::connect(self.url, &self.token, options).await?)
    }
}

/// Joins the room before starting capture so connection setup cannot queue stale camera frames.
pub(crate) async fn publish(args: &Publisher, worker_prefix: &[&str]) -> Result<()> {
    let trace = Trace::open(args.latency_csv.as_deref())?;
    let connection = args.connection.resolve(Role::Publisher)?;
    let (room, _) = tokio::select! {
        result = connection.connect() => result?,
        result = signal::ctrl_c() => return Ok(result?),
    };
    let local = room.local_participant();
    local.register_rpc_method(timing::CLOCK_METHOD.into(), |_| {
        Box::pin(async { Ok(now_us().to_string()) })
    });
    let result = publish_source(&room, args, worker_prefix, &trace).await;
    local.unregister_rpc_method(timing::CLOCK_METHOD.into());
    let closed = room.close().await;
    result?;
    closed?;
    trace.finish()
}

/// Starts the source once the Data Track exists and reaps it on every exit.
async fn publish_source(
    room: &Room,
    args: &Publisher,
    worker_prefix: &[&str],
    trace: &Trace,
) -> Result<()> {
    let participant = room.local_participant();
    let track = tokio::select! {
        result = participant.publish_data_track(args.track.as_str()) => result?,
        result = signal::ctrl_c() => return Ok(result?),
    };
    eprintln!("Published '{}'", args.track);
    let encoder = args.rate.encoder(crate::layers::InferenceMode::Live)?;
    let Some((fps, mut source)) = args.open(worker_prefix).await? else { return Ok(()) };
    let result = async {
        let budget = args.rate.budget(fps)?;
        tokio::select! {
            result = publish_frames(&track, fps, &budget, &mut source, trace, args, encoder) => result,
            result = signal::ctrl_c() => result.map_err(Into::into),
        }
    }
    .await;
    let stopped = source.stop().await;
    result.and(stopped)
}

/// Paces Data Track messages while reading and analyzing at most one picture ahead.
async fn publish_frames(
    track: &LocalDataTrack,
    fps: FrameRate,
    budget: &Budget,
    source: &mut Source,
    trace: &Trace,
    args: &Publisher,
    mut encoder: Encoder,
) -> Result<()> {
    let mut packer = args.rate.packet_encoder(args.compression);
    let period = fps.duration(1);
    let camera = source.is_camera();
    let mut frame = source.read_frame(budget.format()).await?;
    let mut scheduled = Instant::now();
    let mut earliest_catchup = scheduled;
    let mut first_send = None;
    let mut last_send = scheduled;
    let mut frames_sent = 0;
    let mut rejected = 0;
    let mut encode_time = Duration::ZERO;
    let mut peak_encode_time = Duration::ZERO;
    let mut frames_over_five_ms = 0;
    let mut meter = Meter::new(budget.overhead);
    let mut next_report = Duration::from_secs(5);
    while let Some(current) = frame {
        let encode_started = Instant::now();
        let index = frames_sent as u32;
        let mut snapshot = encoder.encode(index, fps, budget, &current);
        packer.stabilize_outline_order(&mut snapshot);
        let data = packer.pack(snapshot)?;
        let elapsed = encode_started.elapsed() + current.analysis_time;
        encode_time += elapsed;
        peak_encode_time = peak_encode_time.max(elapsed);
        frames_over_five_ms += usize::from(elapsed > Duration::from_millis(5));
        let first = *first_send.get_or_insert_with(Instant::now);
        let exposure = current.exposure_us;
        trace.record(index, "capture", current.captured_us)?;
        if let Some(stamp) = exposure {
            trace.record(index, "exposure", stamp)?;
        }
        drop(current);
        let send = async {
            if !camera {
                let deadline = scheduled.max(earliest_catchup);
                if deadline > Instant::now() {
                    time::sleep_until(deadline).await;
                }
            }
            let now = Instant::now();
            let send_us = now_us();
            let parts = messages::split(index, data);
            // Spread multipart pictures over their source frame interval. Bursting them all
            // together can evict earlier parts from the Data Track's latest-frame send queue.
            let spacing = period / parts.len() as u32;
            let mut accepted = true;
            for (i, message) in parts.enumerate() {
                if i > 0 {
                    time::sleep_until(now + spacing * i as u32).await;
                }
                let bytes = message.len();
                let mut packet = DataTrackFrame::from(message);
                if let Some(stamp) = exposure {
                    packet = packet.with_user_timestamp(stamp);
                }
                if let Err(error) = track.try_push(packet) {
                    rejected += 1;
                    packer.reset();
                    eprintln!("Frame {frames_sent} was rejected: {error}");
                    accepted = false;
                    break;
                }
                meter.record(Instant::now() - first, bytes);
            }
            if accepted {
                trace.record(index, "send", send_us)?;
            }
            Ok::<_, anyhow::Error>(now)
        };
        let (now, next) = tokio::join!(biased; send, source.read_frame(budget.format()));
        let now = now?;
        last_send = now;
        // Keep file playback anchored to the source timeline. A late wake must not
        // shift every future deadline. The independent floor bounds catch-up bursts.
        scheduled += period;
        earliest_catchup = now + period / 2;
        frames_sent += 1;
        frame = next?;
        let elapsed = now - first;
        if elapsed >= next_report {
            meter.report(frames_sent, elapsed + period);
            eprintln!(
                "Encoder {:.1} ms per frame",
                encode_time.as_secs_f64() * 1000.0 / frames_sent as f64
            );
            next_report = elapsed + Duration::from_secs(5);
        }
    }
    if let Some(first) = first_send {
        // Leave time for the SDK's asynchronous send path before closing the room.
        let end = Instant::now();
        time::sleep_until(scheduled.max(end + Duration::from_millis(100))).await;
        meter.report(frames_sent, (last_send + period).max(end) - first);
        eprintln!(
            "Encoder {:.3} ms/frame mean, {:.3} ms peak, {frames_over_five_ms}/{frames_sent} over 5 ms",
            encode_time.as_secs_f64() * 1000.0 / frames_sent as f64,
            peak_encode_time.as_secs_f64() * 1000.0,
        );
        eprintln!("Rejected messages: {rejected}");
    }
    Ok(())
}

/// The native viewer replaces waiting pictures, while the recorder writes each reconstructed frame.
pub(crate) enum Output<'a> {
    Window(&'a Window),
    File(&'a Path),
}

/// Owns the room until EOF, user cancellation, or viewer shutdown.
pub(crate) async fn subscribe(
    args: &Subscriber,
    output: Output<'_>,
    trace: &Trace,
    cancel: impl Future<Output = ()>,
) -> Result<()> {
    let connection = args.connection.resolve(Role::Subscriber)?;
    tokio::pin!(cancel);
    let (room, events) = tokio::select! {
        result = connection.connect() => result?,
        _ = &mut cancel => return Ok(()),
        result = signal::ctrl_c() => return Ok(result?),
    };
    let local = room.local_participant();
    let result = tokio::select! {
        result = subscribe_to_track(&local, events, &args.track, args.publisher.as_deref(), output, trace) => result,
        _ = &mut cancel => Ok(()),
    };
    let closed = room.close().await;
    result?;
    Ok(closed?)
}

/// Waits for the selected publication, skipping room events for unrelated tracks.
async fn subscribe_to_track(
    local: &LocalParticipant,
    mut events: tokio::sync::mpsc::UnboundedReceiver<RoomEvent>,
    name: &str,
    publisher: Option<&str>,
    output: Output<'_>,
    trace: &Trace,
) -> Result<()> {
    eprintln!("Waiting for '{name}'");
    let stop = signal::ctrl_c();
    tokio::pin!(stop);
    let track = loop {
        let event = tokio::select! {
            event = events.recv() => event,
            result = &mut stop => return Ok(result?),
        };
        match event {
            Some(RoomEvent::DataTrackPublished(track))
                if track.info().name() == name
                    && publisher.is_none_or(|identity| track.publisher_identity() == identity) =>
            {
                break track
            }
            Some(RoomEvent::Disconnected { .. }) | None => {
                bail!("Room closed before '{name}' was published")
            }
            _ => {}
        }
    };
    // This receiver is unbounded. Drop it once the selected track is found.
    drop(events);
    let peer = track.publisher_identity();
    let probe = timing::probe_clock(local, peer, |sample, rtt| {
        eprintln!("Publisher RTT {:.1} ms", rtt.as_secs_f64() * 1000.0);
        if let Output::Window(window) = &output {
            window.clock_sample(sample, rtt);
        }
    });
    tokio::select! {
        result = play_track(&track, &output, trace) => result,
        () = probe => Ok(()),
    }
}

/// A reconstructed frame for the chosen sink, also used to count image changes.
#[derive(PartialEq)]
enum Reconstructed {
    Rgb(Vec<u8>),
    Triangles(Arc<Vec<DisplayTriangle>>),
}

/// Receives and plays video while the enclosing subscription measures the peer clock.
async fn play_track(track: &RemoteDataTrack, output: &Output<'_>, trace: &Trace) -> Result<()> {
    let stop = signal::ctrl_c();
    tokio::pin!(stop);
    let mut stream = tokio::select! {
        stream = track.subscribe() => stream?,
        result = &mut stop => return Ok(result?),
    };
    eprintln!("Subscribed to '{}' from '{}'", track.info().name(), track.publisher_identity());
    let mut decoder = Receiver::default();
    let first = loop {
        let frame = tokio::select! {
            frame = stream.next() => frame,
            result = &mut stop => return Ok(result?),
        };
        let Some(frame) = frame else { return Ok(()) };
        if let Some(batch) = decode_frame(&mut decoder, &frame, output, trace)? {
            break batch;
        }
    };
    let mut sink = match output {
        Output::File(file) => Some(Sink::open(file, first.fps, first.canvas, false)?),
        Output::Window(_) => None,
    };
    eprintln!("Source is {}×{} at {} fps", first.canvas.width, first.canvas.height, first.fps);
    let mut received = 1u64;
    let mut playout = Playout::new(first);
    let report_every = Duration::from_secs(5);
    let mut last_report = (Instant::now(), 0, 0, 0, 0);
    let mut last_display = (0, 0);
    let mut previous = None;
    let mut changed = 0;
    let mut report = time::interval_at(last_report.0 + report_every, report_every);
    report.set_missed_tick_behavior(time::MissedTickBehavior::Skip);
    let result = async {
        loop {
        if playout.finished() {
            break Ok(());
        }
        tokio::select! {
            result = &mut stop => break result.map_err(Into::into),
            result = async { match &mut sink {
                Some(file) => file.wait().await,
                None => std::future::pending().await,
            }} => break result,
            frame = stream.next(), if !playout.ended => {
                match frame {
                    Some(frame) => {
                        if let Some(batch) = decode_frame(&mut decoder, &frame, output, trace)? {
                            received += 1;
                            playout.queue(batch);
                        }
                    }
                    None => playout.ended = true,
                }
            }
            _ = report.tick() => {
                let now = Instant::now();
                let elapsed = (now - last_report.0).as_secs_f64();
                eprintln!("Received {:.1} fps, reconstructed {:.1} fps, {:.1} image changes/s, {} source frames skipped",
                    (received - last_report.1) as f64 / elapsed,
                    (playout.rendered - last_report.2) as f64 / elapsed,
                    (changed - last_report.4) as f64 / elapsed,
                    playout.skipped - last_report.3);
                if let Output::Window(window) = output {
                    let (painted, superseded) = window.counts();
                    eprintln!("Window {:.1} fps, {} superseded before repaint", (painted-last_display.0) as f64 / elapsed, superseded-last_display.1);
                    last_display = (painted, superseded);
                }
                last_report = (now, received, playout.rendered, playout.skipped, changed);
            }
            _ = std::future::ready(()), if playout.has_frame() => {
                let Some(batch) = playout.next() else { continue };
                let index = batch.start;
                let exposure = batch.exposure_us;
                let frame = match output {
                    Output::Window(_) => Reconstructed::Triangles(Arc::new(batch.display_triangles())),
                    Output::File(_) => Reconstructed::Rgb(batch.render()),
                };
                trace.record(index, "render", now_us())?;
                if let (Some(file), Reconstructed::Rgb(rgb)) = (&mut sink, &frame) {
                    let written = tokio::select! {
                        result = file.write(rgb) => result,
                        result = &mut stop => break result.map_err(Into::into),
                    };
                    if let Err(error) = written {
                        if error.kind() == ErrorKind::BrokenPipe { break file.wait().await; }
                        break Err(error.into());
                    }
                    trace.record(index, "file", now_us())?;
                } else if let (Output::Window(window), Reconstructed::Triangles(triangles)) = (output, &frame) {
                    window.offer(index, exposure, playout.canvas, Arc::clone(triangles));
                }
                playout.rendered += 1;
                changed += u64::from(previous.as_ref() != Some(&frame));
                previous = Some(frame);
            }
        }
        }
    }.await;
    let finished = if let Some(sink) = sink { sink.finish().await } else { Ok(()) };
    eprintln!("Rendered {} frames, skipped {} source frames", playout.rendered, playout.skipped);
    result.and(finished)
}

/// Drops malformed payloads without interrupting the subscription.
fn decode_frame(
    decoder: &mut Receiver,
    frame: &DataTrackFrame,
    output: &Output<'_>,
    trace: &Trace,
) -> Result<Option<DecodedFrame>> {
    let received = now_us();
    let payload = frame.payload();
    if let Output::Window(window) = output {
        window.record_payload(payload.len());
    }
    let mut batch = decoder
        .decode(&payload)
        .inspect_err(|error| eprintln!("Discarding payload: {error}"))
        .ok()
        .flatten();
    if let Some(batch) = &mut batch {
        // Old publishers used send-time milliseconds. Video user_timestamp uses Unix microseconds.
        batch.exposure_us = frame.user_timestamp().filter(|&v| v >= 946_684_800_000_000);
        trace.record(batch.start, "receive", received)?;
        if let Some(stamp) = batch.exposure_us {
            trace.record(batch.start, "exposure", stamp)?;
        }
    }
    Ok(batch)
}

const MAX_PENDING_FRAMES: usize = 2;

/// Plays one message at a time with a small queue for arrival jitter and player startup.
struct Playout {
    canvas: crate::video::Canvas,
    fps: FrameRate,
    pending: VecDeque<DecodedFrame>,
    expected: u32,
    ended: bool,
    rendered: u64,
    skipped: u64,
}

impl Playout {
    /// Starts playback using the first valid packet's frame rate.
    fn new(first: DecodedFrame) -> Self {
        Self {
            canvas: first.canvas,
            fps: first.fps,
            expected: first.start,
            pending: VecDeque::from([first]),
            ended: false,
            rendered: 0,
            skipped: 0,
        }
    }

    /// Indicates whether the next frame can be rendered.
    fn has_frame(&self) -> bool {
        !self.pending.is_empty()
    }

    /// Indicates when the closed subscription has played its last frame.
    fn finished(&self) -> bool {
        self.ended && !self.has_frame()
    }

    /// Queues pictures in order and drops the oldest waiting one if playback falls behind.
    fn queue(&mut self, batch: DecodedFrame) {
        let earliest =
            self.pending.back().map_or(self.expected, |pending| pending.start.wrapping_add(1));
        if batch.canvas != self.canvas
            || batch.fps != self.fps
            || batch.start.wrapping_sub(earliest) >= (1 << 31)
        {
            return;
        }
        if self.pending.len() == MAX_PENDING_FRAMES {
            self.pending.pop_front();
        }
        self.pending.push_back(batch);
    }

    /// Takes the next frame and counts any source frames missed before it.
    fn next(&mut self) -> Option<DecodedFrame> {
        let batch = self.pending.pop_front()?;
        let gap = batch.start.wrapping_sub(self.expected);
        if gap != 0 {
            self.skipped += u64::from(gap);
            eprintln!("Skipped {gap} source frames before frame {}", batch.start);
        }
        self.expected = batch.start.wrapping_add(1);
        Some(batch)
    }
}

#[cfg(test)]
mod tests {
    use livekit_token::TokenVerifier;

    use super::*;
    use crate::compression::{Compression, PacketEncoder};
    use crate::video::{AnalyzedFrame, Canvas};

    /// Retains two frames through player startup and discards the oldest on overflow.
    #[test]
    fn playout_buffers_a_short_burst() -> Result<()> {
        let fps = FrameRate::CAMERA_DEFAULT;
        let budget = Budget::for_layers(1, Canvas::DEFAULT, 144);
        let frame = AnalyzedFrame::new(vec![0; Canvas::DEFAULT.width * Canvas::DEFAULT.height * 3]);
        let mut encoder = Encoder::default();
        let mut batch = |start| DecodedFrame::decode(&encoder.encode(start, fps, &budget, &frame));

        let mut playout = Playout::new(batch(0)?);
        assert!(playout.next().is_some());
        playout.queue(batch(1)?);
        playout.queue(batch(2)?);
        playout.queue(batch(3)?);
        assert_eq!(playout.pending.iter().map(|batch| batch.start).collect::<Vec<_>>(), [2, 3]);
        assert_eq!(playout.next().map(|batch| batch.start), Some(2));
        assert_eq!(playout.skipped, 1);
        Ok(())
    }

    /// Exact sensor times survive compression, including missing stamps.
    #[test]
    fn playout_preserves_exposure() -> Result<()> {
        let fps = FrameRate::CAMERA_DEFAULT;
        let budget = Budget::for_layers(1, Canvas::DEFAULT, 144);
        let frame =
            AnalyzedFrame::new(vec![128; Canvas::DEFAULT.width * Canvas::DEFAULT.height * 3]);
        let mut encoder = Encoder::default();
        let mut packer = PacketEncoder::new(Compression::Predictive);
        let mut receiver = Receiver::default();
        let output = Output::File(Path::new("unused"));
        let t = 1_800_000_000_000_000;
        for (i, (stamp, expected)) in
            [(Some(t), Some(t)), (None, None), (Some(t / 1000), None)].into_iter().enumerate()
        {
            let raw = encoder.encode(i as u32, fps, &budget, &frame);
            let mut wire = DataTrackFrame::from(packer.pack(raw)?);
            if let Some(stamp) = stamp {
                wire = wire.with_user_timestamp(stamp);
            }
            let batch = decode_frame(&mut receiver, &wire, &output, &Trace::default())?
                .expect("valid frame");
            let mut playout = Playout::new(batch);
            assert_eq!(playout.next().expect("queued frame").exposure_us, expected);
        }
        Ok(())
    }

    /// Checks that each role receives its own identity and only the grants it needs.
    #[test]
    fn signs_room_tokens_with_role_grants() {
        let options = ConnectionOptions {
            url: Some("ws://localhost:7880".into()),
            api_key: Some("test-key".into()),
            api_secret: Some("test-secret".into()),
            room_name: "test-room".into(),
            identity: None,
        };
        let verifier = TokenVerifier::with_api_key("test-key", "test-secret");
        for (role, identity, publisher) in
            [(Role::Publisher, "robot-camera", true), (Role::Subscriber, "operator", false)]
        {
            let connection = options.resolve(role).unwrap();
            let claims = verifier.verify(&connection.token).unwrap();
            assert_eq!(claims.sub, identity);
            assert_eq!(claims.video.room, "test-room");
            assert!(claims.video.room_join);
            assert!(!claims.video.can_publish());
            assert!(claims.video.can_publish_data());
            assert_eq!(claims.video.can_subscribe(), !publisher);
        }
    }
}
