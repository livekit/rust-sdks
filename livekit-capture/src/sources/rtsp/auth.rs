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

//! RTSP authentication, backed by the `http-auth` crate: Basic and Digest
//! (RFC 7616, including SHA-256 and session variants).

use std::fmt;

use super::{client::RtspResponse, RtspVideoSourceError};

/// Username and password for RTSP authentication.
#[derive(Clone, PartialEq, Eq)]
pub(super) struct RtspCredentials {
    pub(super) username: String,
    pub(super) password: String,
}

// Manual so a `{:?}` of the source or its context never prints the password.
impl fmt::Debug for RtspCredentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RtspCredentials")
            .field("username", &self.username)
            .field("password", &"<redacted>")
            .finish()
    }
}

/// Tracks the server's authentication challenge across requests.
pub(super) struct RtspAuthContext {
    credentials: Option<RtspCredentials>,
    client: Option<http_auth::PasswordClient>,
}

impl RtspAuthContext {
    /// Creates an authentication context; without credentials, a challenge
    /// fails with [`RtspVideoSourceError::MissingCredentials`].
    pub(super) fn new(credentials: Option<RtspCredentials>) -> Self {
        Self { credentials, client: None }
    }

    /// Builds the `Authorization` header value for a request, once the server
    /// has issued a challenge.
    pub(super) fn header(
        &mut self,
        method: &str,
        uri: &str,
    ) -> Result<Option<String>, RtspVideoSourceError> {
        let Some(client) = self.client.as_mut() else {
            return Ok(None);
        };
        let credentials =
            self.credentials.as_ref().ok_or(RtspVideoSourceError::MissingCredentials)?;
        let authorization = client
            .respond(&http_auth::PasswordParams {
                username: &credentials.username,
                password: &credentials.password,
                uri,
                method,
                // RTSP requests carry no body relevant to `auth-int`.
                body: Some(&[]),
            })
            .map_err(|err| RtspVideoSourceError::Auth(super::sanitized(err)))?;
        Ok(Some(authorization))
    }

    /// Ingests the challenges of a 401 response so the retry can
    /// authenticate. Digest is preferred over Basic when both are offered.
    pub(super) fn update_from_unauthorized(
        &mut self,
        response: &RtspResponse,
    ) -> Result<(), RtspVideoSourceError> {
        if self.credentials.is_none() {
            return Err(RtspVideoSourceError::MissingCredentials);
        }
        let mut builder = http_auth::PasswordClient::builder();
        for challenges in response.headers("www-authenticate") {
            builder = builder.challenges(challenges);
        }
        // Build errors can quote the server's challenge bytes.
        self.client =
            Some(builder.build().map_err(|err| RtspVideoSourceError::Auth(super::sanitized(err)))?);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context_with_credentials() -> RtspAuthContext {
        RtspAuthContext::new(Some(RtspCredentials {
            username: "user".to_owned(),
            password: "pass".to_owned(),
        }))
    }

    fn unauthorized(challenges: &[&str]) -> RtspResponse {
        let mut bytes = b"RTSP/1.0 401 Unauthorized\r\nCSeq: 1\r\n".to_vec();
        for challenge in challenges {
            bytes.extend_from_slice(format!("WWW-Authenticate: {challenge}\r\n").as_bytes());
        }
        bytes.extend_from_slice(b"\r\n");
        RtspResponse::parse_for_tests(&bytes)
    }

    #[test]
    fn answers_basic_challenge() {
        let mut context = context_with_credentials();
        context.update_from_unauthorized(&unauthorized(&["Basic realm=\"camera\""])).unwrap();

        let header = context.header("DESCRIBE", "rtsp://camera.example/live").unwrap().unwrap();
        assert_eq!(header, "Basic dXNlcjpwYXNz");
    }

    #[test]
    fn prefers_digest_over_basic() {
        let mut context = context_with_credentials();
        context
            .update_from_unauthorized(&unauthorized(&[
                "Basic realm=\"camera\"",
                "Digest realm=\"camera\", nonce=\"abcdef\", qop=\"auth\"",
            ]))
            .unwrap();

        let header = context.header("DESCRIBE", "rtsp://camera.example/live").unwrap().unwrap();
        assert!(header.starts_with("Digest "), "unexpected header: {header}");
        assert!(header.contains("username=\"user\""));
        assert!(header.contains("nc=00000001"));
    }

    #[test]
    fn digest_nonce_count_increments_across_requests() {
        let mut context = context_with_credentials();
        context
            .update_from_unauthorized(&unauthorized(&[
                "Digest realm=\"camera\", nonce=\"abcdef\", qop=\"auth\"",
            ]))
            .unwrap();

        let _ = context.header("DESCRIBE", "rtsp://camera.example/live").unwrap().unwrap();
        let second = context.header("SETUP", "rtsp://camera.example/live").unwrap().unwrap();
        assert!(second.contains("nc=00000002"), "unexpected header: {second}");
    }

    #[test]
    fn rejects_unsupported_scheme() {
        let mut context = context_with_credentials();
        let err =
            context.update_from_unauthorized(&unauthorized(&["Bearer token=\"abc\""])).unwrap_err();
        assert!(matches!(err, RtspVideoSourceError::Auth(_)), "unexpected error: {err:?}");
    }

    #[test]
    fn escapes_control_characters_in_auth_errors() {
        let mut context = context_with_credentials();
        let err = context
            .update_from_unauthorized(&unauthorized(&["Bearer \u{7f}\u{1b}[31mfake"]))
            .unwrap_err();
        let message = err.to_string();
        assert!(
            !message.chars().any(char::is_control),
            "control characters in message: {message:?}"
        );
    }

    #[test]
    fn requires_credentials_for_challenges() {
        let mut context = RtspAuthContext::new(None);
        let err =
            context.update_from_unauthorized(&unauthorized(&["Basic realm=\"c\""])).unwrap_err();
        assert!(matches!(err, RtspVideoSourceError::MissingCredentials));
    }

    #[test]
    fn no_header_before_challenge() {
        let mut context = context_with_credentials();
        assert!(context.header("DESCRIBE", "rtsp://camera.example/live").unwrap().is_none());
    }

    #[test]
    fn debug_redacts_password() {
        let credentials =
            RtspCredentials { username: "admin".to_owned(), password: "secret".to_owned() };
        let debug = format!("{credentials:?}");
        assert!(debug.contains("admin"));
        assert!(!debug.contains("secret"));
    }
}
