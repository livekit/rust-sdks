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

//! Token sources for the LiveKit Rust SDK.
//!
//! A token source procures the credentials — server URL and participant
//! token — needed to join a LiveKit room. Construct one via the
//! [`literal`] / [`endpoint`] / [`development_token_server`] factory
//! functions, or implement [`TokenSourceFixed`] / [`TokenSourceConfigurable`]
//! to plug in a custom credential backend. Wrap a configurable source with
//! [`TokenSourceConfigurable::cached`] to reuse credentials until they expire.

mod caching;
mod error;
mod request;
mod response;
mod token_source;

pub use caching::TokenSourceCached;
pub use caching::TokenSourceInMemoryStore;
pub use caching::TokenSourceStore;
pub use error::TokenSourceError;
pub use request::TokenSourceFetchOptions;
pub use response::TokenSourceResponse;
pub use response::TokenSourceResult;
pub use token_source::development_token_server;
pub use token_source::endpoint;
pub use token_source::literal;
pub use token_source::TokenSourceConfigurable;
pub use token_source::TokenSourceDevelopmentTokenServer;
pub use token_source::TokenSourceEndpoint;
pub use token_source::TokenSourceFixed;
pub use token_source::TokenSourceLiteral;
