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

/// A single HTTP/WebSocket request header.
#[derive(Debug, Clone)]
pub struct Header {
    pub name: String,
    pub value: String,
}

/// The result of an HTTP request performed by the transport.
#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status: u16,
    /// Response headers, in receipt order. Lets callers read e.g. `Cache-Control`.
    pub headers: Vec<Header>,
    pub body: Vec<u8>,
}

/// Errors a transport implementation may return. Mapped onto `SignalError` by the caller.
#[derive(Debug, Clone)]
pub enum TransportError {
    Timeout,
    Connection(String),
    Http { status: u16 },
    Closed,
    Other(String),
}

impl fmt::Display for TransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TransportError::Timeout => write!(f, "transport timed out"),
            TransportError::Connection(m) => write!(f, "transport connection error: {m}"),
            TransportError::Http { status } => write!(f, "transport http error: status {status}"),
            TransportError::Closed => write!(f, "transport closed"),
            TransportError::Other(m) => write!(f, "transport error: {m}"),
        }
    }
}

impl std::error::Error for TransportError {}
