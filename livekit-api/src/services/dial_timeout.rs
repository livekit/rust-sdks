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

//! Request-timeout handling shared by calls that dial a phone and wait for an
//! answer (SIP CreateSIPParticipant/TransferSIPParticipant, WhatsApp
//! AcceptWhatsAppCall). These take longer than a normal request, and the request
//! must outlast ringing or it would abort before the call can be answered.

use std::time::Duration;

/// Default per-request timeout for a call that dials a phone and waits.
pub(crate) const DIAL_TIMEOUT: Duration = Duration::from_secs(30);

/// A dialing request must outlast the ringing window, or it would abort before
/// the call can be answered. Keep the request timeout at least this far above
/// the ringing timeout.
pub(crate) const RINGING_TIMEOUT_MARGIN: Duration = Duration::from_secs(2);

/// Request timeout for a phone-dialing call: the caller's `timeout` (or the dial
/// default) raised, when needed, to stay at least [`RINGING_TIMEOUT_MARGIN`]
/// above the ringing timeout.
pub(crate) fn dial_timeout(
    timeout: Option<Duration>,
    ringing_timeout: Option<Duration>,
) -> Duration {
    let mut effective = timeout.unwrap_or(DIAL_TIMEOUT);
    if let Some(ringing) = ringing_timeout {
        effective = effective.max(ringing + RINGING_TIMEOUT_MARGIN);
    }
    effective
}
