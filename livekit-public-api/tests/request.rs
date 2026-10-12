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

use livekit_public_api::PublicApi;
use livekit_token::Claims;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

#[tokio::test]
async fn get_thread_request() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base_url = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        let (mut conn, _) = listener.accept().await.unwrap();
        let mut buf = vec![0; 8192];
        let n = conn.read(&mut buf).await.unwrap();
        let body = "{}";
        let resp = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{body}",
            body.len()
        );
        conn.write_all(resp.as_bytes()).await.unwrap();
        String::from_utf8_lossy(&buf[..n]).into_owned()
    });

    let api = PublicApi::new(&base_url, "key", "secret");
    api.threads().get("AT_1").await.unwrap();

    let request = server.await.unwrap();
    assert!(request.starts_with("GET /v0/projects/p_/threads/AT_1 HTTP/1.1\r\n"), "{request}");
    let token = request
        .lines()
        .find_map(|l| l.strip_prefix("authorization: Bearer "))
        .expect("bearer token");
    let claims = Claims::from_unverified(token).unwrap();
    assert_eq!(claims.iss, "key");
    assert!(claims.agent.database_admin && claims.agent_thread.all);
}
