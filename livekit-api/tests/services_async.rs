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

//! Runtime smoke for the isahc-based `services-async` backend (the non-tokio
//! server-API client). It drives the full request path — access-token auth,
//! isahc send, header/status conversion across isahc's vendored `http`, and
//! protobuf/JSON decode — against the mock LiveKit server (LK_TEST_SERVER_URL,
//! default http://127.0.0.1:9999). It no-ops when the server is unreachable.
//!
//! The in-crate `api_test` suite is tokio-only (it uses `#[tokio::test]` and
//! reqwest), so this integration test is the async backend's coverage; it runs
//! on `futures::executor::block_on`, which is runtime-agnostic like isahc.
#![cfg(all(feature = "services-async", feature = "access-token", not(feature = "services-tokio")))]

use livekit_api::services::room::CreateRoomOptions;
use livekit_api::services::LiveKitApi;

fn base_url() -> String {
    std::env::var("LK_TEST_SERVER_URL").unwrap_or_else(|_| "http://127.0.0.1:9999".to_owned())
}

/// Reachability probe kept separate from the request assertions: a TCP connect
/// to the mock's authority tells "server offline" (skip, local dev) apart from
/// "server up but the request failed" (a real regression — must fail the test).
/// The tokio suite does the same with `skip_if_offline!`.
fn server_up(base: &str) -> bool {
    let authority = base.split("://").nth(1).unwrap_or(base).trim_end_matches('/');
    std::net::TcpStream::connect(authority).is_ok()
}

#[test]
fn services_async_smoke() {
    let base = base_url();
    if !server_up(&base) {
        eprintln!("skipping services-async smoke: mock test server not reachable at {base}");
        return;
    }
    // Server is up, so every call below must succeed — no silent skips.
    futures::executor::block_on(async {
        let api = LiveKitApi::with_api_key(&base, "devkey", "secret");

        let room = api
            .room()
            .create_room(
                "async-smoke-room",
                CreateRoomOptions { metadata: "{}".to_owned(), ..Default::default() },
            )
            .await
            .expect("create_room");
        assert_eq!(room.name, "async-smoke-room");

        api.room().list_rooms(vec!["async-smoke-room".to_owned()]).await.expect("list_rooms");
        api.sip()
            .create_sip_inbound_trunk(
                "inbound".to_owned(),
                vec!["+15105550100".to_owned()],
                Default::default(),
            )
            .await
            .expect("create_sip_inbound_trunk");
        api.egress().list_egress(Default::default()).await.expect("list_egress");
        api.room().delete_room("async-smoke-room").await.expect("delete_room");
    });
}
