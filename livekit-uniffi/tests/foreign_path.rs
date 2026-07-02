// Copyright 2025 LiveKit, Inc.
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

use livekit_net::{Header, HttpResponse, PlatformConnectResult, PlatformTransport, TransportError};
use std::sync::Arc;

struct T;

#[async_trait::async_trait]
impl PlatformTransport for T {
    async fn connect(&self, _u: String, _h: Vec<Header>, _timeout_ms: u64)
        -> Result<PlatformConnectResult, TransportError> { Err(TransportError::Closed) }
    async fn http_get(&self, _u: String, _h: Vec<Header>)
        -> Result<HttpResponse, TransportError> { Err(TransportError::Closed) }
}

#[test]
fn set_platform_transport_registers() {
    livekit_uniffi::set_platform_transport(Arc::new(T));
    assert!(livekit_net::transport().is_some());
}
