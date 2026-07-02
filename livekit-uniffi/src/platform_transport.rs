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

use livekit_net::PlatformTransport;
use std::sync::Arc;

/// Register the host-provided network transport. The host implements
/// `PlatformTransport` (and `PlatformConnection`) in Swift/Kotlin/Dart and calls
/// this once at startup, before the first connection.
#[uniffi::export]
pub fn set_platform_transport(transport: Arc<dyn PlatformTransport>) {
    livekit_net::set_transport(transport);
}
