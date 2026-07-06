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

//! API tests against the shared mock LiveKit API server (livekit/livekit
//! cmd/test-server). Point them at a running instance with LK_TEST_SERVER_URL
//! (default http://127.0.0.1:9999); they no-op when no server is reachable. In
//! CI the server is booted as a Docker container.
//!
//! See cmd/test-server/README.md for the X-Lk-Mock JSON control protocol. The
//! mock enforces the same per-method grants as the real server (secret
//! "secret"), so a call that succeeds also proves the SDK attached the right
//! grants. The failover tests drive TwirpClient directly because failover
//! relies on internal test-only knobs the service clients don't expose; the
//! rest drive the public LiveKitApi, injecting directives as a default X-Lk-Mock
//! header (the service methods don't take per-call headers).

use std::time::Duration;

use http::header::{HeaderMap, HeaderName, HeaderValue, AUTHORIZATION};
use livekit_protocol as proto;

use super::egress::{EgressListOptions, EgressOutput};
use super::failover::FailoverConfig;
use super::sip::CreateSIPParticipantOptions;
use super::twirp_client::{ServerError, ServerResult, TwirpClient};
use super::{LiveKitApi, ServiceError, SipCallError, LIVEKIT_PACKAGE};
use crate::access_token::{AccessToken, VideoGrants};

fn base_url() -> String {
    std::env::var("LK_TEST_SERVER_URL").unwrap_or_else(|_| "http://127.0.0.1:9999".to_owned())
}

async fn reachable(base: &str) -> bool {
    reqwest::Client::new()
        .get(format!("{base}/settings/regions"))
        .send()
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}

macro_rules! skip_if_offline {
    ($base:expr) => {
        if !reachable(&$base).await {
            eprintln!("skipping: mock test server not reachable at {}", $base);
            return;
        }
    };
}

// -- failover: drives TwirpClient directly for the test-only force/backoff knobs

// `force` bypasses the cloud-host check (the mock is on 127.0.0.1) and the tiny
// backoff keeps tests fast — both are internal, test-only knobs.
fn config(enabled: bool, force: bool) -> FailoverConfig {
    FailoverConfig { enabled, force, backoff_base: Duration::from_millis(1) }
}

// Issues a CreateRoom through the failover machinery with the given X-Lk-Mock
// directives. These tests exercise failover, not authz, so they skip the mock's
// permission check.
async fn call(base: &str, cfg: FailoverConfig, mock: &str) -> ServerResult<proto::Room> {
    let client = TwirpClient::new(base, LIVEKIT_PACKAGE, None).with_failover_config(cfg);
    let mut headers = HeaderMap::new();
    headers.insert(AUTHORIZATION, HeaderValue::from_static("Bearer test-token"));
    headers.insert(HeaderName::from_static("x-lk-mock"), HeaderValue::from_str(mock).unwrap());
    client
        .request::<proto::CreateRoomRequest, proto::Room>(
            "RoomService",
            "CreateRoom",
            proto::CreateRoomRequest::default(),
            headers,
        )
        .await
}

#[tokio::test]
async fn healthy() {
    let base = base_url();
    skip_if_offline!(base);
    call(&base, config(true, true), r#"{"skipAuth":true}"#)
        .await
        .expect("healthy request should succeed");
}

#[tokio::test]
async fn primary_unavailable() {
    let base = base_url();
    skip_if_offline!(base);
    call(&base, config(true, true), r#"{"skipAuth":true,"failRegions":[0]}"#)
        .await
        .expect("should fail over to a healthy region");
}

#[tokio::test]
async fn two_regions_unavailable() {
    let base = base_url();
    skip_if_offline!(base);
    call(&base, config(true, true), r#"{"skipAuth":true,"failRegions":[0,1]}"#)
        .await
        .expect("should fail over to region 2 on the 3rd attempt");
}

#[tokio::test]
async fn all_unavailable() {
    let base = base_url();
    skip_if_offline!(base);
    let err = call(&base, config(true, true), r#"{"skipAuth":true,"failRegions":[0,1,2,3]}"#)
        .await
        .expect_err("all regions down should surface an error");
    assert!(matches!(err, ServerError::Response(_)));
}

#[tokio::test]
async fn client_error_not_retried() {
    let base = base_url();
    skip_if_offline!(base);
    let err =
        call(&base, config(true, true), r#"{"skipAuth":true,"failRegions":[0],"failStatus":400}"#)
            .await
            .expect_err("a 4xx must be returned without failover");
    match err {
        ServerError::Response(code) => assert_eq!(code.code, "invalid_argument"),
        other => panic!("expected a twirp error, got {other:?}"),
    }
}

#[tokio::test]
async fn transport_error_failover() {
    let base = base_url();
    skip_if_offline!(base);
    call(&base, config(true, true), r#"{"skipAuth":true,"failRegions":[0],"failMode":"drop"}"#)
        .await
        .expect("a dropped connection should fail over to a healthy region");
}

#[tokio::test]
async fn region_discovery_unreachable() {
    let base = base_url();
    skip_if_offline!(base);
    call(&base, config(true, true), r#"{"skipAuth":true,"failRegions":[0],"regionsStatus":500}"#)
        .await
        .expect_err("no fallback hosts means the original 5xx is surfaced");
}

#[tokio::test]
async fn not_cloud_host() {
    let base = base_url();
    skip_if_offline!(base);
    // Enabled but not forced; 127.0.0.1 is not a cloud host, so no failover.
    call(&base, config(true, false), r#"{"skipAuth":true,"failRegions":[0]}"#)
        .await
        .expect_err("failover should be cloud-gated for a non-cloud host");
}

#[tokio::test]
async fn disabled() {
    let base = base_url();
    skip_if_offline!(base);
    call(&base, config(false, true), r#"{"skipAuth":true,"failRegions":[0]}"#)
        .await
        .expect_err("disabled failover should not retry");
}

// Pure unit test (no mock server): cloud-gating and the thundering-herd guard.
#[test]
fn attempts_gating_and_timeout_guard() {
    use super::failover::{DEFAULT_REQUEST_TIMEOUT, MAX_ATTEMPTS, MIN_FAILOVER_TIMEOUT};

    let cloud = "myproject.livekit.cloud";
    let ok = DEFAULT_REQUEST_TIMEOUT; // comfortably above the guard threshold

    // Enabled (the default): only *.livekit.cloud project domains fail over.
    assert_eq!(config(true, false).attempts(Some(cloud), ok), MAX_ATTEMPTS);
    assert_eq!(
        config(true, false).attempts(Some("myproject.region.livekit.cloud"), ok),
        MAX_ATTEMPTS
    );
    assert_eq!(config(true, false).attempts(Some("myproject.livekit.io"), ok), 1);
    assert_eq!(config(true, false).attempts(Some("example.com"), ok), 1);
    assert_eq!(config(true, false).attempts(Some("127.0.0.1"), ok), 1);
    assert_eq!(config(true, false).attempts(Some("notlivekit.cloud"), ok), 1);

    // force bypasses the cloud-host check; disabled never fails over.
    assert_eq!(config(true, true).attempts(Some("127.0.0.1"), ok), MAX_ATTEMPTS);
    assert_eq!(config(false, true).attempts(Some(cloud), ok), 1);
    assert_eq!(config(false, false).attempts(Some(cloud), ok), 1);

    // Thundering-herd guard: a sub-threshold per-attempt timeout collapses to a
    // single attempt even on a cloud host; exactly the threshold still fails over.
    let below = MIN_FAILOVER_TIMEOUT - Duration::from_millis(1);
    assert_eq!(config(true, true).attempts(Some(cloud), below), 1);
    assert_eq!(config(true, true).attempts(Some(cloud), MIN_FAILOVER_TIMEOUT), MAX_ATTEMPTS);
}

// -- LiveKitApi: smoke calls across every service, plus token auth and SIP errors

fn api(mock: Option<&str>) -> LiveKitApi {
    let api = LiveKitApi::with_api_key(&base_url(), "devkey", "secret");
    match mock {
        Some(m) => api.with_mock(m),
        None => api,
    }
}

#[tokio::test]
async fn room_smoke() {
    let base = base_url();
    skip_if_offline!(base);
    let api = api(None);
    let room = api.room();
    room.create_room(
        "test-room",
        super::room::CreateRoomOptions {
            empty_timeout: 300,
            max_participants: 50,
            metadata: "{}".to_owned(),
            ..Default::default()
        },
    )
    .await
    .expect("create_room");
    room.list_rooms(vec!["test-room".to_owned(), "lobby".to_owned()]).await.expect("list_rooms");
    room.update_room_metadata("test-room", "{}").await.expect("update_room_metadata");
    room.list_participants("test-room").await.expect("list_participants");
    room.get_participant("test-room", "participant-42").await.expect("get_participant");
    room.remove_participant("test-room", "participant-42").await.expect("remove_participant");
    room.forward_participant("test-room", "participant-42", "overflow-room")
        .await
        .expect("forward_participant");
    room.move_participant("test-room", "participant-42", "breakout-room")
        .await
        .expect("move_participant");
    room.update_subscriptions("test-room", "participant-42", vec!["TR_video1".to_owned()], true)
        .await
        .expect("update_subscriptions");
    room.send_data(
        "test-room",
        b"hello".to_vec(),
        super::room::SendDataOptions {
            kind: proto::data_packet::Kind::Reliable,
            destination_identities: vec!["participant-42".to_owned()],
            topic: Some("chat".to_owned()),
            ..Default::default()
        },
    )
    .await
    .expect("send_data");
    room.update_participant(
        "test-room",
        "participant-42",
        super::room::UpdateParticipantOptions {
            metadata: "{}".to_owned(),
            name: "Alice".to_owned(),
            ..Default::default()
        },
    )
    .await
    .expect("update_participant");
    room.remove_participant_with_options(
        "test-room",
        "participant-42",
        super::room::RemoveParticipantOptions { revoke_token_ts: 1_700_000_000_000 },
    )
    .await
    .expect("remove_participant_with_options");
    room.delete_room("test-room").await.expect("delete_room");
}

// MutePublishedTrack returns the muted track; the mock has no live tracks, so
// supply one via the `response` directive to exercise the SDK's decode path.
#[tokio::test]
async fn room_mute_track() {
    let base = base_url();
    skip_if_offline!(base);
    let api = api(Some(r#"{"response":{"track":{"sid":"TR_video1"}}}"#));
    api.room()
        .mute_published_track("test-room", "participant-42", "TR_video1", true)
        .await
        .expect("mute_published_track");
}

#[tokio::test]
async fn egress_smoke() {
    let base = base_url();
    skip_if_offline!(base);
    let api = api(None);
    let egress = api.egress();
    let file = proto::EncodedFileOutput {
        file_type: proto::EncodedFileType::Mp4 as i32,
        filepath: "room.mp4".to_owned(),
        ..Default::default()
    };
    egress
        .start_room_composite_egress(
            "test-room",
            vec![EgressOutput::File(file)],
            super::egress::RoomCompositeOptions { layout: "grid".to_owned(), ..Default::default() },
        )
        .await
        .expect("start_room_composite_egress");
    egress
        .start_web_egress(
            "https://example.com/scene",
            vec![EgressOutput::File(proto::EncodedFileOutput {
                file_type: proto::EncodedFileType::Mp4 as i32,
                filepath: "web.mp4".to_owned(),
                ..Default::default()
            })],
            Default::default(),
        )
        .await
        .expect("start_web_egress");
    egress
        .start_participant_egress(
            "test-room",
            "participant-42",
            vec![EgressOutput::File(proto::EncodedFileOutput {
                file_type: proto::EncodedFileType::Mp4 as i32,
                filepath: "participant.mp4".to_owned(),
                ..Default::default()
            })],
            Default::default(),
        )
        .await
        .expect("start_participant_egress");
    egress
        .start_track_composite_egress(
            "test-room",
            vec![EgressOutput::File(proto::EncodedFileOutput {
                file_type: proto::EncodedFileType::Mp4 as i32,
                filepath: "track-composite.mp4".to_owned(),
                ..Default::default()
            })],
            super::egress::TrackCompositeOptions {
                audio_track_id: "TR_audio1".to_owned(),
                video_track_id: "TR_video1".to_owned(),
                ..Default::default()
            },
        )
        .await
        .expect("start_track_composite_egress");
    egress
        .start_track_egress(
            "test-room",
            super::egress::TrackEgressOutput::WebSocket("wss://example.com/ws".to_owned()),
            "TR_video1",
        )
        .await
        .expect("start_track_egress");
    egress.update_layout("EG_abc123", "speaker").await.expect("update_layout");
    egress
        .update_stream("EG_abc123", vec!["rtmps://b.example.com/live/key".to_owned()], vec![])
        .await
        .expect("update_stream");
    egress
        .list_egress(EgressListOptions {
            filter: super::egress::EgressListFilter::Room("test-room".to_owned()),
            active: true,
            ..Default::default()
        })
        .await
        .expect("list_egress");
    egress.stop_egress("EG_abc123").await.expect("stop_egress");
}

#[tokio::test]
async fn ingress_smoke() {
    let base = base_url();
    skip_if_offline!(base);
    let api = api(None);
    let ingress = api.ingress();
    ingress
        .create_ingress(
            proto::IngressInput::RtmpInput,
            super::ingress::CreateIngressOptions {
                name: "stream-input".to_owned(),
                room_name: "test-room".to_owned(),
                participant_identity: "ingress-bot".to_owned(),
                participant_name: "Live Stream".to_owned(),
                enable_transcoding: Some(true),
                ..Default::default()
            },
        )
        .await
        .expect("create_ingress");
    ingress
        .update_ingress(
            "IN_abc123",
            super::ingress::UpdateIngressOptions {
                name: "stream-input-v2".to_owned(),
                room_name: "test-room".to_owned(),
                ..Default::default()
            },
        )
        .await
        .expect("update_ingress");
    ingress
        .list_ingress(super::ingress::IngressListFilter::Room("test-room".to_owned()))
        .await
        .expect("list_ingress");
    ingress.delete_ingress("IN_abc123").await.expect("delete_ingress");
}

#[tokio::test]
async fn sip_smoke() {
    let base = base_url();
    skip_if_offline!(base);
    let api = api(None);
    let sip = api.sip();
    sip.create_sip_inbound_trunk(
        "inbound".to_owned(),
        vec!["+15105550100".to_owned()],
        Default::default(),
    )
    .await
    .expect("create_sip_inbound_trunk");
    sip.create_sip_outbound_trunk(
        "outbound".to_owned(),
        "sip.telco.example.com".to_owned(),
        vec!["+15105550100".to_owned()],
        Default::default(),
    )
    .await
    .expect("create_sip_outbound_trunk");
    sip.list_sip_inbound_trunk(super::sip::ListSIPInboundTrunkFilter::All)
        .await
        .expect("list_sip_inbound_trunk");
    sip.list_sip_outbound_trunk(super::sip::ListSIPOutboundTrunkFilter::All)
        .await
        .expect("list_sip_outbound_trunk");
    sip.create_sip_dispatch_rule(
        proto::sip_dispatch_rule::Rule::DispatchRuleDirect(proto::SipDispatchRuleDirect {
            room_name: "support".to_owned(),
            pin: "1234".to_owned(),
        }),
        Default::default(),
    )
    .await
    .expect("create_sip_dispatch_rule");
    sip.list_sip_dispatch_rule(super::sip::ListSIPDispatchRuleFilter::All)
        .await
        .expect("list_sip_dispatch_rule");
    sip.update_sip_inbound_trunk(
        "ST_abc123".to_owned(),
        proto::SipInboundTrunkUpdate { metadata: Some("{}".to_owned()), ..Default::default() },
    )
    .await
    .expect("update_sip_inbound_trunk");
    sip.update_sip_inbound_trunk_replace(
        "ST_abc123".to_owned(),
        proto::SipInboundTrunkInfo { name: "inbound".to_owned(), ..Default::default() },
    )
    .await
    .expect("update_sip_inbound_trunk_replace");
    sip.update_sip_outbound_trunk(
        "ST_abc123".to_owned(),
        proto::SipOutboundTrunkUpdate { metadata: Some("{}".to_owned()), ..Default::default() },
    )
    .await
    .expect("update_sip_outbound_trunk");
    sip.update_sip_outbound_trunk_replace(
        "ST_abc123".to_owned(),
        proto::SipOutboundTrunkInfo {
            name: "outbound".to_owned(),
            address: "sip.telco.example.com".to_owned(),
            ..Default::default()
        },
    )
    .await
    .expect("update_sip_outbound_trunk_replace");
    sip.update_sip_dispatch_rule(
        "SDR_abc123".to_owned(),
        proto::SipDispatchRuleUpdate { name: Some("rule".to_owned()), ..Default::default() },
    )
    .await
    .expect("update_sip_dispatch_rule");
    sip.update_sip_dispatch_rule_replace(
        "SDR_abc123".to_owned(),
        proto::SipDispatchRuleInfo { name: "rule".to_owned(), ..Default::default() },
    )
    .await
    .expect("update_sip_dispatch_rule_replace");
    sip.delete_sip_dispatch_rule("SDR_abc123").await.expect("delete_sip_dispatch_rule");
    sip.delete_sip_trunk("ST_abc123").await.expect("delete_sip_trunk");
}

// TransferSIPParticipant blocks until the REFER completes; delayMs:0 skips the wait.
#[tokio::test]
async fn sip_transfer() {
    let base = base_url();
    skip_if_offline!(base);
    api(Some(r#"{"delayMs":0}"#))
        .sip()
        .transfer_sip_participant(
            "test-room".to_owned(),
            "sip-caller".to_owned(),
            "tel:+15105550122".to_owned(),
            super::sip::TransferSIPParticipantOptions {
                ringing_timeout: Some(Duration::from_secs(2)),
                ..Default::default()
            },
        )
        .await
        .expect("transfer_sip_participant");
}

#[tokio::test]
async fn connector_smoke() {
    let base = base_url();
    skip_if_offline!(base);
    let api = api(None);
    let connector = api.connector();
    connector
        .dial_whatsapp_call(
            "123456789012345",
            "+15105550100",
            "wa-secret-key",
            "23.0",
            Default::default(),
        )
        .await
        .expect("dial_whatsapp_call");
    connector
        .connect_twilio_call(
            proto::connect_twilio_call_request::TwilioCallDirection::Inbound,
            "test-room",
            Default::default(),
        )
        .await
        .expect("connect_twilio_call");
    let offer = proto::SessionDescription {
        r#type: "offer".to_owned(),
        sdp: "v=0\r\n".to_owned(),
        ..Default::default()
    };
    connector
        .connect_whatsapp_call("wacid.HBg", offer.clone())
        .await
        .expect("connect_whatsapp_call");
    connector
        .accept_whatsapp_call(
            "123456789012345",
            "wa-secret-key",
            "23.0",
            "wacid.HBg",
            offer,
            Default::default(),
        )
        .await
        .expect("accept_whatsapp_call");
    connector
        .disconnect_whatsapp_call("wacid.HBg", "wa-secret-key")
        .await
        .expect("disconnect_whatsapp_call");
    connector
        .disconnect_whatsapp_call_with_reason(
            "wacid.HBg",
            "wa-secret-key",
            proto::disconnect_whats_app_call_request::DisconnectReason::UserInitiated,
        )
        .await
        .expect("disconnect_whatsapp_call_with_reason");
}

#[tokio::test]
async fn agent_dispatch_smoke() {
    let base = base_url();
    skip_if_offline!(base);
    let api = api(None);
    let ad = api.agent_dispatch();
    ad.create_dispatch(proto::CreateAgentDispatchRequest {
        room: "test-room".to_owned(),
        agent_name: "inbound-agent".to_owned(),
        metadata: "{}".to_owned(),
        ..Default::default()
    })
    .await
    .expect("create_dispatch");
    ad.list_dispatch("test-room").await.expect("list_dispatch");
    ad.get_dispatch("AD_abc123", "test-room").await.expect("get_dispatch");
    ad.delete_dispatch("AD_abc123", "test-room").await.expect("delete_dispatch");
}

// -- deep: create_room round-trips its fields ------------------------------------

#[tokio::test]
async fn create_room_echoes_fields() {
    let base = base_url();
    skip_if_offline!(base);
    let room = api(None)
        .room()
        .create_room(
            "echo-room",
            super::room::CreateRoomOptions {
                metadata: "{\"scene\":\"lobby\"}".to_owned(),
                empty_timeout: 300,
                max_participants: 50,
                ..Default::default()
            },
        )
        .await
        .expect("create_room");
    assert_eq!(room.name, "echo-room");
    assert_eq!(room.metadata, "{\"scene\":\"lobby\"}");
    assert_eq!(room.empty_timeout, 300);
    assert_eq!(room.max_participants, 50);
    assert!(!room.sid.is_empty()); // placeholder assigned by the mock
}

// -- deep: SIP participant (delayMs:0 skips the mock's answer wait) ---------------

#[tokio::test]
async fn sip_participant() {
    let base = base_url();
    skip_if_offline!(base);
    let p = api(Some(r#"{"delayMs":0}"#))
        .sip()
        .create_sip_participant(
            "ST_abc123".to_owned(),
            "+15105550100".to_owned(),
            "test-room".to_owned(),
            CreateSIPParticipantOptions {
                participant_identity: "sip-caller".to_owned(),
                participant_name: Some("SIP Caller".to_owned()),
                display_name: Some("Support".to_owned()),
                dtmf: Some("*123#".to_owned()),
                play_dialtone: Some(true),
                wait_until_answered: Some(true),
                ringing_timeout: Some(Duration::from_secs(2)),
                ..Default::default()
            },
            None,
        )
        .await
        .expect("create_sip_participant");
    assert_eq!(p.room_name, "test-room");
    assert_eq!(p.participant_identity, "sip-caller");
}

// -- cross-cutting: token auth ---------------------------------------------------

#[tokio::test]
async fn token_auth() {
    let base = base_url();
    skip_if_offline!(base);
    let token = AccessToken::with_api_key("devkey", "secret")
        .with_grants(VideoGrants { room_create: true, ..Default::default() })
        .to_jwt()
        .expect("sign token");
    let room = LiveKitApi::with_token(&base, &token)
        .room()
        .create_room("token-room", Default::default())
        .await
        .expect("create_room with token");
    assert_eq!(room.name, "token-room");
}

// -- cross-cutting: SIP call errors parse into SipCallError ----------------------

async fn sip_error(sip_status: &str) -> ServiceError {
    api(Some(&format!(r#"{{"delayMs":0,"sipStatus":{sip_status}}}"#)))
        .sip()
        .create_sip_participant(
            "ST_abc123".to_owned(),
            "+15105550100".to_owned(),
            "test-room".to_owned(),
            Default::default(),
            None,
        )
        .await
        .expect_err("a SIP status should surface as an error")
}

#[tokio::test]
async fn sip_busy() {
    let base = base_url();
    skip_if_offline!(base);
    let err = sip_error(r#"{"code":486,"status":"Busy Here"}"#).await;
    let e = SipCallError::from_error(&err).expect("should decode a SipCallError");
    assert_eq!(e.code(), "resource_exhausted");
    assert_eq!(e.sip_status_code(), Some(486));
    assert_eq!(e.sip_status(), Some("Busy Here"));
    let s = e.to_string();
    assert!(s.contains("486") && s.contains("Busy Here"), "{s}");
}

#[tokio::test]
async fn sip_declined() {
    let base = base_url();
    skip_if_offline!(base);
    let err = sip_error(r#"{"code":603,"status":"Decline"}"#).await;
    let e = SipCallError::from_error(&err).expect("should decode a SipCallError");
    assert_eq!(e.code(), "permission_denied");
    assert_eq!(e.sip_status_code(), Some(603));
}

#[tokio::test]
async fn sip_no_answer() {
    let base = base_url();
    skip_if_offline!(base);
    let err = sip_error(r#"{"code":408,"status":"Request Timeout"}"#).await;
    let e = SipCallError::from_error(&err).expect("should decode a SipCallError");
    assert_eq!(e.code(), "deadline_exceeded");
    assert_eq!(e.sip_status_code(), Some(408));
}

// A non-SIP error is not a SipCallError.
#[tokio::test]
async fn non_sip_error_not_decoded() {
    let base = base_url();
    skip_if_offline!(base);
    let err = api(Some(r#"{"failRegions":[0],"failStatus":400}"#))
        .room()
        .create_room("test-room", Default::default())
        .await
        .expect_err("failStatus should surface an error");
    assert!(SipCallError::from_error(&err).is_none());
}

// -- cross-cutting: client-side dial timeout -------------------------------------

#[tokio::test]
async fn sip_dial_timeout() {
    let base = base_url();
    skip_if_offline!(base);
    // ringing_timeout 1s -> ~3s dial budget; the mock delays the answer past it.
    let err = api(Some(r#"{"delayMs":4000}"#))
        .sip()
        .create_sip_participant(
            "ST_abc123".to_owned(),
            "+15105550100".to_owned(),
            "test-room".to_owned(),
            CreateSIPParticipantOptions {
                participant_identity: "sip-caller".to_owned(),
                wait_until_answered: Some(true),
                ringing_timeout: Some(Duration::from_secs(1)),
                ..Default::default()
            },
            None,
        )
        .await
        .expect_err("the dial should abort before the mock answers");
    assert!(matches!(err, ServiceError::Server(ServerError::Request(_))), "{err:?}");
}
