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

use std::fmt;

#[cfg(feature = "native")]
use std::{sync::Arc, time::Duration};

#[cfg(feature = "native")]
use crate::{Header, TransportError, WsConnectResult};

/// A TCP (and, for `wss`, TLS) connection to a LiveKit server, opened ahead of
/// a connect so that the connect only performs the WebSocket upgrade.
///
/// The connection is used at most once. Clones share it, so whichever clone is
/// used first consumes it. Dropping the last clone, or calling
/// [`close`](Self::close), closes the socket.
///
/// Only the built-in native transport can use it: with a host-registered
/// [`WsClient`](crate::WsClient), a connect dials through that client instead.
#[derive(Clone)]
pub struct PrewarmedConnection {
    #[cfg(feature = "native")]
    inner: Arc<crate::native::prewarm::Warm>,
    // Without the native backend there is no way to create one.
    #[cfg(not(feature = "native"))]
    _unconstructible: std::convert::Infallible,
}

impl PrewarmedConnection {
    /// Open a connection to the server at `url`.
    ///
    /// `http(s)` URLs are treated as `ws(s)`, so either spelling matches a later
    /// connect to the same host and port. Fails if the connection cannot be
    /// opened, or if an `HTTP(S)_PROXY` is configured for the scheme.
    #[cfg(feature = "native")]
    pub async fn connect(url: &str) -> Result<Self, TransportError> {
        let inner = crate::native::prewarm::Warm::open(url).await?;
        Ok(Self { inner: Arc::new(inner) })
    }

    /// Whether the connection can still be used: it has not been used or
    /// closed, and the server has not closed it.
    pub fn is_alive(&self) -> bool {
        #[cfg(feature = "native")]
        return self.inner.is_alive();
        #[cfg(not(feature = "native"))]
        match self._unconstructible {}
    }

    /// Close the connection without using it.
    pub fn close(&self) {
        #[cfg(feature = "native")]
        self.inner.close();
        #[cfg(not(feature = "native"))]
        match self._unconstructible {}
    }

    /// Perform the WebSocket upgrade for `url` over this connection.
    ///
    /// Returns `Ok(None)` if the connection cannot be used: it was already used
    /// or closed, it was opened for a different server, or the upgrade failed
    /// or exceeded `timeout` (capped at 1.5 s, which only a silently dropped
    /// connection reaches; a close or reset fails at once). The caller should then
    /// dial a fresh connection. An HTTP error status from the server is
    /// returned as an error. The connection is consumed either way.
    #[cfg(feature = "native")]
    pub async fn upgrade(
        &self,
        url: &str,
        headers: &[Header],
        timeout: Duration,
    ) -> Result<Option<WsConnectResult>, TransportError> {
        if crate::has_ws_client() {
            self.close();
            return Ok(None);
        }
        self.inner.upgrade(url, headers, timeout).await
    }
}

impl fmt::Debug for PrewarmedConnection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PrewarmedConnection").field("alive", &self.is_alive()).finish()
    }
}
