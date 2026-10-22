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

//! D3D11 native buffers sent through a real encoder (Livenalt fork patch 3).
//! Two peer connections in one process; needs the native libwebrtc and a
//! Direct3D 11 device (WARP will do). Its own test binary, because it counts
//! the process-wide readbacks.

#![cfg(target_os = "windows")]

#[path = "support/d3d11.rs"]
mod d3d;

use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc, Mutex,
};
use std::time::{Duration, Instant};

use libwebrtc::{
    peer_connection_factory::native::PeerConnectionFactoryExt,
    prelude::*,
    stats::RtcStats,
    video_frame::native::{d3d11_readback_count, NativeBuffer},
    video_source::native::NativeVideoSource,
    video_stream::native::NativeVideoStream,
};
use tokio_stream::StreamExt;
use windows::core::Interface;

/// The Livenalt ladder: quarter, half and full size.
const LAYERS: [(&str, f64, u64); 3] =
    [("q", 4.0, 300_000), ("h", 2.0, 1_200_000), ("f", 1.0, 4_000_000)];

/// A sender with the three layers on VP8 and a receiver, connected.
async fn vp8_simulcast_loopback(
    factory: &PeerConnectionFactory,
    track: RtcVideoTrack,
) -> (PeerConnection, PeerConnection) {
    let sender = factory.create_peer_connection(RtcConfiguration::default()).unwrap();
    let receiver = factory.create_peer_connection(RtcConfiguration::default()).unwrap();
    let transceiver = sender
        .add_transceiver(
            MediaStreamTrack::Video(track),
            RtpTransceiverInit {
                direction: RtpTransceiverDirection::SendOnly,
                stream_ids: vec!["screen".into()],
                send_encodings: LAYERS
                    .iter()
                    .map(|&(rid, scale, max_bitrate)| RtpEncodingParameters {
                        rid: rid.into(),
                        scale_resolution_down_by: Some(scale),
                        max_bitrate: Some(max_bitrate),
                        ..Default::default()
                    })
                    .collect(),
            },
        )
        .unwrap();
    let vp8 = factory
        .get_rtp_sender_capabilities(MediaType::Video)
        .codecs
        .into_iter()
        .filter(|c| c.mime_type.eq_ignore_ascii_case("video/VP8"))
        .collect::<Vec<_>>();
    transceiver.set_codec_preferences(vp8).unwrap();

    let to_receiver: Arc<Mutex<Vec<IceCandidate>>> = Default::default();
    let to_sender: Arc<Mutex<Vec<IceCandidate>>> = Default::default();
    {
        let q = to_receiver.clone();
        sender.on_ice_candidate(Some(Box::new(move |c| q.lock().unwrap().push(c))));
        let q = to_sender.clone();
        receiver.on_ice_candidate(Some(Box::new(move |c| q.lock().unwrap().push(c))));
    }
    let offer = sender.create_offer(OfferOptions::default()).await.unwrap();
    sender.set_local_description(offer.clone()).await.unwrap();
    receiver.set_remote_description(offer).await.unwrap();
    let answer = receiver.create_answer(AnswerOptions::default()).await.unwrap();
    receiver.set_local_description(answer.clone()).await.unwrap();
    // libwebrtc's answerer declines simulcast; answer as an SFU does, with
    // every rid as recv, so the sender runs all three layers.
    let mut munged = String::new();
    for line in answer.to_string().lines().filter(|l| !l.is_empty()) {
        munged.push_str(line);
        munged.push_str("\r\n");
        if line.starts_with("a=mid:") {
            for (rid, _, _) in LAYERS {
                munged.push_str(&format!("a=rid:{rid} recv\r\n"));
            }
            munged.push_str("a=simulcast:recv q;h;f\r\n");
        }
    }
    let answer = SessionDescription::parse(&munged, SdpType::Answer).unwrap();
    sender.set_remote_description(answer).await.unwrap();

    let deadline = Instant::now() + Duration::from_secs(10);
    while sender.connection_state() != PeerConnectionState::Connected {
        assert!(Instant::now() < deadline, "loopback did not connect");
        for c in std::mem::take(&mut *to_receiver.lock().unwrap()) {
            receiver.add_ice_candidate(c).await.unwrap();
        }
        for c in std::mem::take(&mut *to_sender.lock().unwrap()) {
            sender.add_ice_candidate(c).await.unwrap();
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    // Start the estimate at the ladder's sum so every layer runs at once.
    sender
        .set_bitrate(BitrateSettings {
            min_bitrate_bps: None,
            start_bitrate_bps: Some(5_500_000),
            max_bitrate_bps: Some(11_000_000),
        })
        .unwrap();
    (sender, receiver)
}

/// The track hands its sinks the D3D11 buffer itself, and the VP8 encoder,
/// which wants memory, costs one readback per frame however many layers it
/// encodes from it.
#[tokio::test(flavor = "multi_thread")]
async fn d3d11_frames_reach_a_vp8_simulcast_encoder_with_one_readback_each() {
    let (width, height) = (1280u32, 720u32);
    let factory = PeerConnectionFactory::default();
    let source = NativeVideoSource::new(VideoResolution { width, height }, true);
    let track = factory.create_video_track("screen", source.clone());
    let (sender, receiver) = vp8_simulcast_loopback(&factory, track.clone()).await;

    // A capture pool's worth of textures, used in turn.
    let device = d3d::Device::new();
    let textures: Vec<_> = (0..3u8)
        .map(|i| {
            device.texture(width, height, move |x, y| [(x as u8) ^ (i * 80), y as u8, 128, 255])
        })
        .collect();
    let raw: Vec<usize> = textures.iter().map(|t| t.as_raw() as usize).collect();

    let mut local = NativeVideoStream::new(track.clone());
    let readbacks_before = d3d11_readback_count();
    let stop = Arc::new(AtomicBool::new(false));
    let captured = Arc::new(AtomicU64::new(0));
    let pump = {
        let (stop, captured, source, raw) =
            (stop.clone(), captured.clone(), source.clone(), raw.clone());
        std::thread::spawn(move || {
            let start = Instant::now();
            let mut n = 0u64;
            while !stop.load(Ordering::Relaxed) {
                let texture = raw[(n % 3) as usize] as *mut std::ffi::c_void;
                let buffer = unsafe {
                    NativeBuffer::from_d3d11_texture(texture, VideoResolution { width, height }, ())
                }
                .unwrap();
                source.capture_frame(&VideoFrame {
                    rotation: VideoRotation::VideoRotation0,
                    timestamp_us: start.elapsed().as_micros() as i64,
                    frame_metadata: None,
                    buffer,
                });
                n += 1;
                captured.store(n, Ordering::SeqCst);
                let next = start + Duration::from_micros(n * 33_333);
                if let Some(wait) = next.checked_duration_since(Instant::now()) {
                    std::thread::sleep(wait);
                }
            }
        })
    };

    // What the track's sinks get is the texture itself.
    let first = tokio::time::timeout(Duration::from_secs(5), local.next())
        .await
        .expect("a frame on the local track")
        .unwrap();
    let native = first.buffer.as_native().expect("the sink gets a native buffer");
    let texture = native.get_d3d11_texture().expect("a D3D11 buffer") as usize;
    assert!(raw.contains(&texture), "the sink's texture is one of ours");
    drop(first);
    drop(local);

    tokio::time::sleep(Duration::from_secs(4)).await;
    stop.store(true, Ordering::SeqCst);
    pump.join().unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;

    let readbacks = d3d11_readback_count() - readbacks_before;
    let captured = captured.load(Ordering::SeqCst);
    let mut layers: Vec<(String, u32, u32, u64, String)> = sender
        .get_stats()
        .await
        .unwrap()
        .into_iter()
        .filter_map(|s| match s {
            RtcStats::OutboundRtp(o) if o.stream.kind == "video" => Some((
                o.outbound.rid,
                o.outbound.frame_width,
                o.outbound.frame_height,
                o.outbound.frames_encoded as u64,
                o.outbound.encoder_implementation,
            )),
            _ => None,
        })
        .collect();
    layers.sort_by_key(|l| l.1);
    println!("captured {captured}, readbacks {readbacks}");
    for (rid, w, h, frames, encoder) in &layers {
        println!("  {rid} {w}x{h} {frames} frames encoded by {encoder}");
    }

    assert_eq!(layers.len(), 3, "three simulcast layers: {layers:?}");
    for (rid, w, h, frames, _) in &layers {
        assert!(*frames > 0, "layer {rid} ({w}x{h}) encoded nothing");
    }
    // The source also sends a few black I420 keepalive frames before the
    // first capture, which the encoder counts and nothing reads back; and the
    // adapter may drop a captured frame. So: about one readback per captured
    // frame, never more, and every layer encoded from the same one.
    let all: u64 = layers.iter().map(|l| l.3).sum();
    assert!(readbacks <= captured, "at most one readback per frame ({readbacks} > {captured})");
    assert!(
        readbacks * 10 >= captured * 9,
        "the encoder read the captured frames back ({readbacks} of {captured})"
    );
    assert!(all >= readbacks * 2, "the layers share each frame's readback ({all} vs {readbacks})");

    sender.close();
    receiver.close();
}
