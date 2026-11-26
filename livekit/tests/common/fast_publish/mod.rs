// Copyright 2026 LiveKit, Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! A bounded, owned signaling proxy for deterministic publication sequencing tests.

use std::{collections::VecDeque, sync::Arc, time::Duration};

use anyhow::{anyhow, Context, Result};
use futures_util::{Sink, SinkExt, Stream, StreamExt};
use livekit_protocol as proto;
use parking_lot::Mutex;
use prost::Message;
use tokio::{
    net::{TcpListener, TcpStream},
    sync::{oneshot, watch},
    task::{JoinHandle, JoinSet},
    time::{self, Instant},
};
use tokio_tungstenite::{
    accept_hdr_async, connect_async,
    tungstenite::{
        self, client::IntoClientRequest, handshake::server::Request, Message as WsMessage,
    },
};

/// Per-test controls; no changes to the LiveKit server or public SDK API are needed.
#[derive(Clone, Default)]
pub struct ProxyOptions {
    pub fast_publish: bool,
    pub hold_ack: bool,
    pub reject_track: bool,
    pub clear_codecs: bool,
    pub one_way_delay: Duration,
}

/// A publication-related message observed on the signaling path.
#[derive(Clone, Debug)]
pub struct Observation {
    pub kind: &'static str,
    pub at: Instant,
    pub cid: String,
}

/// Owns the listener and all forwarding tasks; shutdown cancels and joins every task.
pub struct SignalProxy {
    pub url: String,
    observations: Arc<Mutex<Vec<Observation>>>,
    release: watch::Sender<bool>,
    stop: Option<oneshot::Sender<()>>,
    task: Option<JoinHandle<()>>,
}

impl SignalProxy {
    /// Starts a proxy to the local development server (or LIVEKIT_TEST_SERVER_URL).
    pub async fn start(options: ProxyOptions) -> Result<Self> {
        let target = std::env::var("LIVEKIT_TEST_SERVER_URL")
            .unwrap_or_else(|_| "ws://127.0.0.1:7880".into());
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let url = format!("ws://{}", listener.local_addr()?);
        let observations = Arc::new(Mutex::new(Vec::new()));
        let (release, release_rx) = watch::channel(!options.hold_ack);
        let (stop, mut stop_rx) = oneshot::channel();
        let state = observations.clone();
        let task = tokio::spawn(async move {
            let mut connections = JoinSet::new();
            loop {
                tokio::select! {
                    _ = &mut stop_rx => break,
                    accepted = listener.accept(), if connections.len() < 8 => {
                        let Ok((socket, _)) = accepted else { break };
                        let (target, options, state, release_rx) =
                            (target.clone(), options.clone(), state.clone(), release_rx.clone());
                        connections.spawn(async move {
                            if let Err(error) = forward(socket, &target, options, state, release_rx).await {
                                log::debug!("publication test proxy connection ended: {error}");
                            }
                        });
                    }
                    _ = connections.join_next(), if !connections.is_empty() => {}
                }
            }
            connections.shutdown().await;
        });
        Ok(Self { url, observations, release, stop: Some(stop), task: Some(task) })
    }

    /// Releases withheld publication responses without blocking SDP answers or ICE.
    pub fn release_ack(&self) {
        self.release.send_replace(true);
    }

    /// Returns the recorded messages for one track.
    pub fn observations(&self, cid: &str) -> Vec<Observation> {
        self.observations.lock().iter().filter(|event| event.cid == cid).cloned().collect()
    }

    /// Waits for an observed message, bounded independently from the SDK publish timeout.
    pub async fn wait_for(&self, cid: &str, kind: &str) -> Result<()> {
        time::timeout(Duration::from_secs(5), async {
            loop {
                if self.observations(cid).iter().any(|event| event.kind == kind) {
                    return;
                }
                time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .with_context(|| format!("did not observe {kind} for {cid}"))
    }

    /// Stops the listener, closes connections, and joins every owned forwarding task.
    pub async fn shutdown(mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(task) = self.task.take() {
            let _ = task.await;
        }
    }
}

impl Drop for SignalProxy {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        // Normal exits use shutdown; abort is a fallback for a panicking test.
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}

/// Records a bounded diagnostic trace, without retaining message bodies or credentials.
fn record(state: &Mutex<Vec<Observation>>, kind: &'static str, cid: &str) {
    let mut events = state.lock();
    assert!(events.len() < 4096, "test proxy trace exceeded its bound");
    events.push(Observation { kind, cid: cid.to_owned(), at: Instant::now() });
}

/// Connects both WebSockets and runs independent, ordered forwarding loops for each direction.
async fn forward(
    socket: TcpStream,
    target: &str,
    options: ProxyOptions,
    state: Arc<Mutex<Vec<Observation>>>,
    release: watch::Receiver<bool>,
) -> Result<()> {
    let mut original = None;
    let client = accept_hdr_async(socket, |request: &Request, response| {
        original = Some(request.clone());
        Ok(response)
    })
    .await?;
    let original = original.ok_or_else(|| anyhow!("missing upgrade request"))?;
    let path = original.uri().path_and_query().map_or("/rtc", |path| path.as_str());
    let mut request = format!("{}{path}", target.trim_end_matches('/')).into_client_request()?;
    if let Some(auth) = original.headers().get("authorization") {
        request.headers_mut().insert("authorization", auth.clone());
    }
    let (server, _) = connect_async(request).await?;
    let (client_sink, client_stream) = client.split();
    let (server_sink, server_stream) = server.split();
    tokio::try_join!(
        pipe(client_stream, server_sink, false, &options, &state, release.clone()),
        pipe(server_stream, client_sink, true, &options, &state, release),
    )?;
    Ok(())
}

/// Forwards messages with a bounded delay queue; withholding an ACK never blocks an answer.
async fn pipe<R, W>(
    mut source: R,
    mut sink: W,
    downstream: bool,
    options: &ProxyOptions,
    state: &Mutex<Vec<Observation>>,
    mut release: watch::Receiver<bool>,
) -> Result<()>
where
    R: Stream<Item = Result<WsMessage, tungstenite::Error>> + Unpin,
    W: Sink<WsMessage, Error = tungstenite::Error> + Unpin,
{
    let mut queued = VecDeque::new();
    let mut held = Vec::new();
    loop {
        let due = queued
            .front()
            .map_or_else(|| Instant::now() + Duration::from_secs(3600), |(due, _)| *due);
        tokio::select! {
            changed = release.changed(), if downstream && !held.is_empty() => {
                if changed.is_err() { return Ok(()) }
                if *release.borrow() {
                    for message in held.drain(..) {
                        queued.push_back((Instant::now() + options.one_way_delay, message));
                    }
                }
            }
            _ = time::sleep_until(due), if !queued.is_empty() => {
                let (_, message) = queued.pop_front().expect("the queue is nonempty");
                sink.send(message).await?;
            }
            message = source.next(), if queued.len() < 64 && held.len() < 16 => {
                let Some(message) = message else { return Ok(()) };
                let mut message = message?;
                if let WsMessage::Binary(bytes) = &message {
                    if downstream {
                        let mut response = proto::SignalResponse::decode(bytes.as_ref())?;
                        if let Some(proto::signal_response::Message::Join(join)) = &mut response.message {
                            join.fast_publish = options.fast_publish;
                            if options.clear_codecs { join.enabled_publish_codecs.clear(); }
                            message = WsMessage::binary(response.encode_to_vec());
                        } else if let Some(proto::signal_response::Message::TrackPublished(published)) = &response.message {
                            record(state, "ack", &published.cid);
                            if options.reject_track {
                                response.message = Some(proto::signal_response::Message::RequestResponse(proto::RequestResponse {
                                    reason: proto::request_response::Reason::NotAllowed as i32,
                                    message: "injected publication rejection".into(),
                                    request: Some(proto::request_response::Request::AddTrack(proto::AddTrackRequest {
                                        cid: published.cid.clone(), ..Default::default()
                                    })),
                                    ..Default::default()
                                }));
                                message = WsMessage::binary(response.encode_to_vec());
                            }
                            if options.hold_ack && !*release.borrow() {
                                held.push(message);
                                continue;
                            }
                        }
                    } else {
                        let request = proto::SignalRequest::decode(bytes.as_ref())?;
                        match request.message {
                            Some(proto::signal_request::Message::AddTrack(ref track)) => record(state, "add", &track.cid),
                            Some(proto::signal_request::Message::Offer(ref offer)) => {
                                let cids: Vec<_> = state.lock().iter().filter(|event| event.kind == "add" && offer.sdp.contains(&event.cid)).map(|event| event.cid.clone()).collect();
                                for cid in cids { record(state, "offer", &cid); }
                            }
                            _ => {}
                        }
                    }
                }
                queued.push_back((Instant::now() + options.one_way_delay, message));
            }
        }
    }
}
