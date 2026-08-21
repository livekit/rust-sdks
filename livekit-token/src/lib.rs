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

#![doc = include_str!("../README.md")]

mod access_token;
mod jwt_provider;

pub use access_token::*;

/// Reads the API key and secret from the `LIVEKIT_API_KEY` and
/// `LIVEKIT_API_SECRET` environment variables.
///
/// Used by [`AccessToken::new`] and [`TokenVerifier::new`], and by the server-API
/// service clients in `livekit-api` for their own env-based constructors.
#[doc(hidden)]
pub fn get_env_keys() -> Result<(String, String), std::env::VarError> {
    let api_key = std::env::var("LIVEKIT_API_KEY")?;
    let api_secret = std::env::var("LIVEKIT_API_SECRET")?;
    Ok((api_key, api_secret))
}
