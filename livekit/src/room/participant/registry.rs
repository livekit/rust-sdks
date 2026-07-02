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

use super::ClientCapability;
use crate::room::id::ParticipantIdentity;

/// Read access to remote participants' advertised protocol and capabilities.
///
/// Shared by the RPC transport (v1/v2 transport selection) and the data-stream send
/// path (inline / compression eligibility), so both consult a single abstraction over
/// the room's remote participants and both are unit-testable with a fake.
pub(crate) trait RemoteParticipantRegistry: Send + Sync {
    /// A remote participant's `client_protocol`, or `CLIENT_PROTOCOL_DEFAULT` (0) if unknown.
    fn remote_client_protocol(&self, identity: &ParticipantIdentity) -> i32;

    /// A remote participant's advertised capabilities, or empty if unknown.
    fn remote_capabilities(&self, identity: &ParticipantIdentity) -> Vec<ClientCapability>;

    /// The identities of every remote participant, used to resolve a broadcast send.
    fn remote_identities(&self) -> Vec<ParticipantIdentity>;
}
