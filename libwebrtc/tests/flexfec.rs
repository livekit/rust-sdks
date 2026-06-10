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

//! FlexFEC negotiation test.
//!
//! Verifies that initializing the FlexFEC field trials before the peer
//! connection factory is created makes video/flexfec-03 appear in the sender
//! capabilities, and that a send-only video offer with flexfec in the codec
//! preferences contains the flexfec-03 codec and a FEC-FR ssrc-group.
//!
//! This lives in its own integration-test binary on purpose: field trials are
//! process-global and must be set before any other test creates the factory.

use libwebrtc::media_stream_track::MediaStreamTrack;
use libwebrtc::native::fec;
use libwebrtc::peer_connection::OfferOptions;
use libwebrtc::peer_connection_factory::{
    native::PeerConnectionFactoryExt, PeerConnectionFactory, RtcConfiguration,
};
use libwebrtc::rtp_transceiver::{RtpTransceiverDirection, RtpTransceiverInit};
use libwebrtc::video_source::native::NativeVideoSource;
use libwebrtc::video_source::VideoResolution;
use libwebrtc::MediaType;

#[tokio::test]
async fn flexfec_is_advertised_and_offered() {
    assert!(
        fec::init_field_trials(fec::FLEXFEC_FIELD_TRIALS),
        "field trials must not have been initialized before this test"
    );
    fec::set_fec_override(fec::FecOverrideConfig {
        fixed_fec_rate: Some(51), // ~20%
        mask_type: Some(fec::FecMaskType::Bursty),
        max_frames: Some(4),
    });

    let factory = PeerConnectionFactory::default();

    let capabilities = factory.get_rtp_sender_capabilities(MediaType::Video);
    let flexfec: Vec<_> = capabilities
        .codecs
        .iter()
        .filter(|codec| codec.mime_type.eq_ignore_ascii_case("video/flexfec-03"))
        .cloned()
        .collect();
    assert!(
        !flexfec.is_empty(),
        "video/flexfec-03 missing from sender capabilities; field trials were not applied"
    );

    let pc = factory.create_peer_connection(RtcConfiguration::default()).unwrap();
    let source = NativeVideoSource::new(VideoResolution { width: 640, height: 360 }, false);
    let track = factory.create_video_track("flexfec_test", source);
    let transceiver = pc
        .add_transceiver(
            MediaStreamTrack::Video(track),
            RtpTransceiverInit {
                direction: RtpTransceiverDirection::SendOnly,
                stream_ids: vec![],
                send_encodings: vec![],
            },
        )
        .unwrap();

    let mut preferences: Vec<_> = capabilities
        .codecs
        .iter()
        .filter(|codec| codec.mime_type.eq_ignore_ascii_case("video/vp8"))
        .cloned()
        .collect();
    assert!(!preferences.is_empty(), "video/vp8 missing from sender capabilities");
    preferences.extend(flexfec);
    transceiver.set_codec_preferences(preferences).unwrap();

    let offer = pc.create_offer(OfferOptions::default()).await.unwrap();
    let sdp = offer.to_string();
    assert!(sdp.contains("flexfec-03/90000"), "offer is missing flexfec-03:\n{sdp}");
    assert!(sdp.contains("a=ssrc-group:FEC-FR"), "offer is missing the FEC-FR ssrc-group:\n{sdp}");

    pc.close();
}
