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

//! Request-timeout handling shared by calls that may block until a call is
//! answered (SIP CreateSIPParticipant/TransferSIPParticipant, WhatsApp
//! AcceptWhatsAppCall). These take longer than a normal request, and the request
//! must outlast the wait or it would abort before the call can be answered.

use std::time::Duration;

/// Ring window assumed when a request doesn't set a ringing timeout; matches the
/// server default. A dialing request must outlast it.
pub(crate) const DEFAULT_RINGING_TIMEOUT: Duration = Duration::from_secs(30);

/// A dialing request must outlast the ringing window, or it would abort before
/// the call can be answered. Keep the request timeout at least this far above
/// the ringing timeout.
pub(crate) const RINGING_TIMEOUT_MARGIN: Duration = Duration::from_secs(2);

/// Request timeout for a phone-dialing call: the ring window plus a margin, so
/// the request doesn't abort before the call can be answered. The ring window is
/// `ringing_timeout` when set, else [`DEFAULT_RINGING_TIMEOUT`]. A longer caller
/// `timeout` is honored; a shorter one is raised to the floor.
pub(crate) fn dial_timeout(
    timeout: Option<Duration>,
    ringing_timeout: Option<Duration>,
) -> Duration {
    let ring = ringing_timeout.unwrap_or(DEFAULT_RINGING_TIMEOUT);
    let floor = ring + RINGING_TIMEOUT_MARGIN;
    timeout.unwrap_or(floor).max(floor)
}
