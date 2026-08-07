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

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::Engine as _;

use crate::error::TokenSourceError;

const DEFAULT_VALIDITY_TOLERANCE: Duration = Duration::from_secs(60);

/// The credentials returned by a token source: the server to connect to and
/// the participant token to authenticate with.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct TokenSourceResponse {
    pub server_url: String,
    pub participant_token: String,
}

impl TokenSourceResponse {
    /// Reports whether the participant token is currently usable, with the
    /// default tolerance of 60 seconds.
    ///
    /// See [`TokenSourceResponse::has_valid_token_with_tolerance`].
    pub fn has_valid_token(&self) -> bool {
        self.has_valid_token_with_tolerance(DEFAULT_VALIDITY_TOLERANCE)
    }

    /// Reports whether the participant token is currently usable: it decodes
    /// the token as a JWT **without verifying its signature** and checks the
    /// `nbf`/`exp` claims against the current time. The token is considered
    /// valid when it is already active (`now >= nbf`, if present) and does not
    /// expire within the given tolerance (`now + tolerance < exp`).
    ///
    /// A token that does not parse as a JWT or has no `exp` claim is treated
    /// as invalid.
    ///
    /// This is a client-side freshness heuristic only — since the signature is
    /// never verified, it must not be used as an authorization check.
    pub fn has_valid_token_with_tolerance(&self, tolerance: Duration) -> bool {
        let Some(claims) = decode_claims(&self.participant_token) else {
            return false;
        };
        let Ok(now) = SystemTime::now().duration_since(UNIX_EPOCH) else {
            return false;
        };
        let now = now.as_secs();

        let active = claims.nbf.is_none_or(|nbf| now >= nbf);
        let not_expired =
            claims.exp.is_some_and(|exp| now.saturating_add(tolerance.as_secs()) < exp);
        active && not_expired
    }
}

/// The JWT claims relevant for validity checking. LiveKit tokens use integer
/// `NumericDate` values, so fractional timestamps are rejected as unparseable.
#[derive(serde::Deserialize)]
struct Claims {
    exp: Option<u64>,
    nbf: Option<u64>,
}

/// Decodes the payload of a JWT without verifying its signature.
fn decode_claims(token: &str) -> Option<Claims> {
    let mut segments = token.split('.');
    let payload = match (segments.next(), segments.next(), segments.next(), segments.next()) {
        (Some(_), Some(payload), Some(_), None) => payload,
        _ => return None,
    };
    let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(payload).ok()?;
    serde_json::from_slice(&payload).ok()
}

/// Result alias used by all token source operations.
pub type TokenSourceResult<T> = Result<T, TokenSourceError>;

#[cfg(test)]
mod tests {
    use super::*;

    fn now_secs() -> u64 {
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs()
    }

    fn jwt(payload: &str) -> String {
        let engine = base64::engine::general_purpose::URL_SAFE_NO_PAD;
        format!(
            "{}.{}.{}",
            engine.encode(r#"{"alg":"HS256","typ":"JWT"}"#),
            engine.encode(payload),
            engine.encode("signature")
        )
    }

    fn response_with_token(participant_token: String) -> TokenSourceResponse {
        TokenSourceResponse { server_url: "wss://example.livekit.cloud".into(), participant_token }
    }

    #[test]
    fn future_exp_is_valid() {
        let response = response_with_token(jwt(&format!(r#"{{"exp":{}}}"#, now_secs() + 3600)));
        assert!(response.has_valid_token());
    }

    #[test]
    fn past_exp_is_invalid() {
        let response = response_with_token(jwt(&format!(r#"{{"exp":{}}}"#, now_secs() - 3600)));
        assert!(!response.has_valid_token());
    }

    #[test]
    fn exp_within_tolerance_is_invalid() {
        let response = response_with_token(jwt(&format!(r#"{{"exp":{}}}"#, now_secs() + 30)));
        assert!(!response.has_valid_token());
        assert!(response.has_valid_token_with_tolerance(Duration::ZERO));
    }

    #[test]
    fn future_nbf_is_invalid() {
        let response = response_with_token(jwt(&format!(
            r#"{{"nbf":{},"exp":{}}}"#,
            now_secs() + 600,
            now_secs() + 3600
        )));
        assert!(!response.has_valid_token());
    }

    #[test]
    fn past_nbf_with_future_exp_is_valid() {
        let response = response_with_token(jwt(&format!(
            r#"{{"nbf":{},"exp":{}}}"#,
            now_secs() - 600,
            now_secs() + 3600
        )));
        assert!(response.has_valid_token());
    }

    #[test]
    fn missing_exp_is_invalid() {
        let response = response_with_token(jwt(r#"{"sub":"identity"}"#));
        assert!(!response.has_valid_token());
    }

    #[test]
    fn malformed_tokens_are_invalid() {
        for token in ["", "opaque-token", "one.two", "one.two.three.four", "a.!!!not-base64.c"] {
            let response = response_with_token(token.into());
            assert!(!response.has_valid_token(), "expected {token:?} to be invalid");
        }
        let engine = base64::engine::general_purpose::URL_SAFE_NO_PAD;
        let bad_json = format!("{0}.{1}.{0}", engine.encode("header"), engine.encode("not json"));
        assert!(!response_with_token(bad_json).has_valid_token());
    }
}
