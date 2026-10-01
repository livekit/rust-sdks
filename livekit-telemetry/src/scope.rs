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

use std::{
    fmt,
    sync::{Arc, Mutex},
};

use crate::{Attribute, AttributeValue};

/// One session's identity: the trace id every one of its records carries, and the attributes
/// attached to them at export time (`lk.room.sid`, `lk.participant.identity`, …).
pub(crate) struct ScopeState {
    pub trace_id: [u8; 16],
    /// SDK-owned attributes (`lk.room.*`, `lk.participant.*`), attached at export: late-known
    /// identity (the room sid arrives after join) still reaches the records captured before.
    attributes: Mutex<Vec<Attribute>>,
    /// The app's correlation attributes, copied into each record when it is captured, so a
    /// later change never rewrites what is already queued.
    custom: Mutex<Vec<Attribute>>,
    /// The project host this session's batches go to (`Scope::set_server`); `None` until then.
    route: Mutex<Option<String>>,
    /// The last `(url, token)` handed over, so handing the same pair again costs a comparison;
    /// `None` again once disconnected (see [`ScopeState::in_call`]).
    server: Mutex<Option<(String, String)>>,
}

impl ScopeState {
    /// A fresh session: random, non-zero trace id (OTLP treats all-zero as absent).
    pub fn new() -> Arc<Self> {
        Self::with_trace_id(rand::random::<u128>().max(1).to_be_bytes())
    }

    pub fn with_trace_id(trace_id: [u8; 16]) -> Arc<Self> {
        Arc::new(Self {
            trace_id,
            attributes: Mutex::new(Vec::new()),
            custom: Mutex::new(Vec::new()),
            route: Mutex::new(None),
            server: Mutex::new(None),
        })
    }

    /// In a call: it has a server and has not disconnected since.
    pub fn in_call(&self) -> bool {
        self.server.lock().unwrap_or_else(|e| e.into_inner()).is_some()
    }

    /// Whether `(url, token)` is what this session already has; remembers it if not.
    pub fn same_server(&self, url: &str, token: &str) -> bool {
        let mut server = self.server.lock().unwrap_or_else(|e| e.into_inner());
        if server.as_ref().is_some_and(|(u, t)| u == url && t == token) {
            return true;
        }
        *server = Some((url.to_owned(), token.to_owned()));
        false
    }

    pub fn route(&self) -> Option<String> {
        self.route.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub fn set_route(&self, host: String) {
        *self.route.lock().unwrap_or_else(|e| e.into_inner()) = Some(host);
    }

    /// The trace id as 32 hex characters.
    pub fn hex(&self) -> String {
        format!("{:032x}", u128::from_be_bytes(self.trace_id))
    }

    pub fn set_attribute(&self, key: &str, value: Option<AttributeValue>) {
        let mut attributes = self.attributes.lock().unwrap_or_else(|e| e.into_inner());
        attributes.retain(|a| a.key != key);
        if let Some(value) = value {
            attributes.push(Attribute::new(key, value));
        }
    }

    /// Set or remove an app correlation attribute. `false` when rejected: over the limits, in
    /// the SDK's namespace, or one attribute too many.
    /// Whether `set_custom(key, value)` would be accepted: within the limits, outside the
    /// SDK's namespace, not one attribute too many.
    pub fn accepts_custom(&self, key: &str, value: Option<&AttributeValue>) -> bool {
        if !crate::event::valid_custom(key, value) {
            return false;
        }
        let custom = self.custom.lock().unwrap_or_else(|e| e.into_inner());
        value.is_none()
            || custom.iter().any(|a| a.key == key)
            || custom.len() < crate::event::MAX_CUSTOM_ATTRIBUTES
    }

    /// Set or remove an app correlation attribute; `false` when rejected (see
    /// [`accepts_custom`](Self::accepts_custom)).
    pub fn set_custom(&self, key: &str, value: Option<AttributeValue>) -> bool {
        if !self.accepts_custom(key, value.as_ref()) {
            return false;
        }
        let mut custom = self.custom.lock().unwrap_or_else(|e| e.into_inner());
        custom.retain(|a| a.key != key);
        if let Some(value) = value {
            custom.push(Attribute::new(key, value));
        }
        true
    }

    /// The app's correlation attributes right now.
    pub fn custom_snapshot(&self) -> Vec<Attribute> {
        self.custom.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Copy the app's correlation attributes into a record being captured; the record's own
    /// attributes win.
    pub fn snapshot_custom(&self, own: &mut Vec<Attribute>) {
        let custom = self.custom.lock().unwrap_or_else(|e| e.into_inner());
        for attribute in custom.iter() {
            if !own.iter().any(|a| a.key == attribute.key) {
                own.push(attribute.clone());
            }
        }
    }

    /// At export: the session's SDK-owned attributes win over anything the record carries
    /// (an app cannot spoof them), then the pipeline-wide ones (`global`) fill in, and
    /// `session.id` (OTel semconv) — the trace id, so a record can be joined to its session even
    /// where a backend drops trace ids from logs.
    pub fn decorate(&self, own: &mut Vec<Attribute>, global: &[Attribute]) {
        let session = self.attributes.lock().unwrap_or_else(|e| e.into_inner());
        for attribute in session.iter() {
            own.retain(|a| a.key != attribute.key);
            own.push(attribute.clone());
        }
        for attribute in global {
            if !own.iter().any(|a| a.key == attribute.key) {
                own.push(attribute.clone());
            }
        }
        own.retain(|a| a.key != "session.id");
        own.push(Attribute::new("session.id", self.hex()));
    }
}

impl PartialEq for ScopeState {
    fn eq(&self, other: &Self) -> bool {
        self.trace_id == other.trace_id
    }
}

impl fmt::Debug for ScopeState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Scope({})", self.hex())
    }
}
