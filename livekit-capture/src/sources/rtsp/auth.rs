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

//! RTSP authentication: Basic and Digest (MD5 with `qop=auth`).

use std::{
    collections::hash_map::RandomState,
    fmt,
    hash::{BuildHasher, Hasher},
};

use base64::{engine::general_purpose, Engine as _};
use md5::{Digest, Md5};

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
#[derive(Debug, Clone)]
pub(super) struct RtspAuthContext {
    credentials: Option<RtspCredentials>,
    challenge: Option<RtspAuthChallenge>,
    nonce_count: u32,
    cnonce: String,
}

impl RtspAuthContext {
    /// Creates an authentication context; without credentials, a challenge
    /// fails with [`RtspVideoSourceError::MissingCredentials`].
    pub(super) fn new(credentials: Option<RtspCredentials>) -> Self {
        Self { credentials, challenge: None, nonce_count: 0, cnonce: make_cnonce() }
    }

    /// Builds the `Authorization` header value for a request, once the server
    /// has issued a challenge.
    pub(super) fn header(
        &mut self,
        method: &str,
        uri: &str,
    ) -> Result<Option<String>, RtspVideoSourceError> {
        let Some(challenge) = self.challenge.clone() else {
            return Ok(None);
        };
        let credentials =
            self.credentials.as_ref().ok_or(RtspVideoSourceError::MissingCredentials)?;
        match challenge {
            RtspAuthChallenge::Basic => {
                let token = general_purpose::STANDARD
                    .encode(format!("{}:{}", credentials.username, credentials.password));
                Ok(Some(format!("Basic {token}")))
            }
            RtspAuthChallenge::Digest(challenge) => {
                self.nonce_count = self.nonce_count.saturating_add(1);
                Ok(Some(build_digest_authorization(
                    credentials,
                    &challenge,
                    method,
                    uri,
                    self.nonce_count,
                    &self.cnonce,
                )))
            }
        }
    }

    /// Ingests the challenge of a 401 response so the retry can authenticate.
    pub(super) fn update_from_unauthorized(
        &mut self,
        response: &RtspResponse,
    ) -> Result<(), RtspVideoSourceError> {
        if self.credentials.is_none() {
            return Err(RtspVideoSourceError::MissingCredentials);
        }
        self.challenge = Some(parse_authenticate_header(
            response.headers("www-authenticate").collect::<Vec<_>>().as_slice(),
        )?);
        self.nonce_count = 0;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum RtspAuthChallenge {
    Basic,
    Digest(DigestAuthChallenge),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DigestAuthChallenge {
    realm: String,
    nonce: String,
    opaque: Option<String>,
    qop: Option<String>,
}

fn parse_authenticate_header(
    headers: &[&str],
) -> Result<RtspAuthChallenge, RtspVideoSourceError> {
    for header in headers {
        if strip_auth_scheme(header, "Digest").is_some() {
            return parse_digest_challenge(header);
        }
    }
    for header in headers {
        if strip_auth_scheme(header, "Basic").is_some() {
            return Ok(RtspAuthChallenge::Basic);
        }
    }
    let scheme = headers
        .first()
        .and_then(|header| header.split_whitespace().next())
        .unwrap_or_default()
        .to_owned();
    Err(RtspVideoSourceError::UnsupportedAuthScheme(scheme))
}

fn parse_digest_challenge(header: &str) -> Result<RtspAuthChallenge, RtspVideoSourceError> {
    let params = parse_auth_params(
        strip_auth_scheme(header, "Digest").ok_or(RtspVideoSourceError::InvalidAuthChallenge)?,
    );
    let realm = params
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("realm"))
        .map(|(_, value)| value.to_owned())
        .ok_or(RtspVideoSourceError::InvalidAuthChallenge)?;
    let nonce = params
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("nonce"))
        .map(|(_, value)| value.to_owned())
        .ok_or(RtspVideoSourceError::InvalidAuthChallenge)?;
    if let Some((_, algorithm)) =
        params.iter().find(|(name, _)| name.eq_ignore_ascii_case("algorithm"))
    {
        if !algorithm.eq_ignore_ascii_case("MD5") {
            return Err(RtspVideoSourceError::UnsupportedDigestAlgorithm(algorithm.clone()));
        }
    }
    let qop = params
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("qop"))
        .and_then(|(_, value)| select_digest_qop(value));
    let opaque = params
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("opaque"))
        .map(|(_, value)| value.to_owned());

    Ok(RtspAuthChallenge::Digest(DigestAuthChallenge { realm, nonce, opaque, qop }))
}

fn strip_auth_scheme<'a>(header: &'a str, scheme: &str) -> Option<&'a str> {
    let header = header.trim_start();
    let rest = header.get(scheme.len()..)?;
    if !header[..scheme.len()].eq_ignore_ascii_case(scheme) {
        return None;
    }
    if rest.is_empty() {
        return Some(rest);
    }
    rest.strip_prefix(' ')
}

fn parse_auth_params(params: &str) -> Vec<(String, String)> {
    let mut parsed = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut escaped = false;
    for ch in params.chars() {
        if escaped {
            current.push(ch);
            escaped = false;
            continue;
        }
        match ch {
            '\\' if in_quotes => {
                escaped = true;
                current.push(ch);
            }
            '"' => {
                in_quotes = !in_quotes;
                current.push(ch);
            }
            ',' if !in_quotes => {
                push_auth_param(&mut parsed, &current);
                current.clear();
            }
            _ => current.push(ch),
        }
    }
    push_auth_param(&mut parsed, &current);
    parsed
}

fn push_auth_param(parsed: &mut Vec<(String, String)>, param: &str) {
    let Some((name, value)) = param.trim().split_once('=') else {
        return;
    };
    parsed.push((name.trim().to_owned(), unquote_auth_value(value.trim())));
}

fn unquote_auth_value(value: &str) -> String {
    let Some(value) = value.strip_prefix('"').and_then(|value| value.strip_suffix('"')) else {
        return value.to_owned();
    };
    let mut unquoted = String::new();
    let mut escaped = false;
    for ch in value.chars() {
        if escaped {
            unquoted.push(ch);
            escaped = false;
        } else if ch == '\\' {
            escaped = true;
        } else {
            unquoted.push(ch);
        }
    }
    unquoted
}

fn select_digest_qop(value: &str) -> Option<String> {
    value.split(',').map(str::trim).find(|qop| qop.eq_ignore_ascii_case("auth")).map(str::to_owned)
}

fn build_digest_authorization(
    credentials: &RtspCredentials,
    challenge: &DigestAuthChallenge,
    method: &str,
    uri: &str,
    nonce_count: u32,
    cnonce: &str,
) -> String {
    let ha1 =
        md5_hex(format!("{}:{}:{}", credentials.username, challenge.realm, credentials.password));
    let ha2 = md5_hex(format!("{method}:{uri}"));
    let response = if let Some(qop) = &challenge.qop {
        md5_hex(format!("{ha1}:{}:{nonce_count:08x}:{cnonce}:{qop}:{ha2}", challenge.nonce))
    } else {
        md5_hex(format!("{ha1}:{}:{ha2}", challenge.nonce))
    };

    let mut header = format!(
        "Digest username=\"{}\", realm=\"{}\", nonce=\"{}\", uri=\"{}\", response=\"{}\"",
        quote_auth_value(&credentials.username),
        quote_auth_value(&challenge.realm),
        quote_auth_value(&challenge.nonce),
        quote_auth_value(uri),
        response
    );
    if let Some(qop) = &challenge.qop {
        header.push_str(&format!(
            ", qop={}, nc={nonce_count:08x}, cnonce=\"{}\"",
            quote_auth_value(qop),
            quote_auth_value(cnonce)
        ));
    }
    if let Some(opaque) = &challenge.opaque {
        header.push_str(&format!(", opaque=\"{}\"", quote_auth_value(opaque)));
    }
    header
}

fn quote_auth_value(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn md5_hex(input: impl AsRef<[u8]>) -> String {
    format!("{:x}", Md5::digest(input))
}

/// Builds an unpredictable client nonce. Each [`RandomState`] draws fresh
/// OS-seeded keys, so chaining two independent states yields 128 bits of
/// entropy without adding an RNG dependency.
fn make_cnonce() -> String {
    let mut hasher = RandomState::new().build_hasher();
    hasher.write_u64(0x6c6b_7274_7370);
    let high = hasher.finish();
    let mut hasher = RandomState::new().build_hasher();
    hasher.write_u64(high);
    let low = hasher.finish();
    format!("{high:016x}{low:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_digest_authorization_with_qop_auth() {
        // RFC 2617 section 3.5 example values.
        let credentials = RtspCredentials {
            username: "Mufasa".to_owned(),
            password: "Circle Of Life".to_owned(),
        };
        let challenge = DigestAuthChallenge {
            realm: "testrealm@host.com".to_owned(),
            nonce: "dcd98b7102dd2f0e8b11d0f600bfb0c093".to_owned(),
            opaque: Some("5ccc069c403ebaf9f0171e9517f40e41".to_owned()),
            qop: Some("auth".to_owned()),
        };

        let authorization = build_digest_authorization(
            &credentials,
            &challenge,
            "GET",
            "/dir/index.html",
            1,
            "0a4f113b",
        );

        assert!(authorization.contains("response=\"6629fae49393a05397450978507c4ef1\""));
        assert!(authorization.contains("qop=auth"));
        assert!(authorization.contains("nc=00000001"));
        assert!(authorization.contains("opaque=\"5ccc069c403ebaf9f0171e9517f40e41\""));
    }

    #[test]
    fn parses_digest_challenge_with_quoted_values() {
        let challenge = parse_authenticate_header(&[
            "Digest realm=\"a, \\\"quoted\\\" realm\", nonce=\"abc\", qop=\"auth,auth-int\"",
        ])
        .unwrap();

        assert_eq!(
            challenge,
            RtspAuthChallenge::Digest(DigestAuthChallenge {
                realm: "a, \"quoted\" realm".to_owned(),
                nonce: "abc".to_owned(),
                opaque: None,
                qop: Some("auth".to_owned()),
            })
        );
    }

    #[test]
    fn prefers_digest_over_basic() {
        let challenge = parse_authenticate_header(&[
            "Basic realm=\"camera\"",
            "Digest realm=\"camera\", nonce=\"abc\"",
        ])
        .unwrap();
        assert!(matches!(challenge, RtspAuthChallenge::Digest(_)));
    }

    #[test]
    fn rejects_unsupported_auth_scheme() {
        let err = parse_authenticate_header(&["Bearer token=\"abc\""]).unwrap_err();
        match err {
            RtspVideoSourceError::UnsupportedAuthScheme(scheme) => assert_eq!(scheme, "Bearer"),
            other => panic!("expected unsupported auth scheme, got {other:?}"),
        }
    }

    #[test]
    fn rejects_unsupported_digest_algorithm() {
        let err = parse_authenticate_header(&[
            "Digest realm=\"camera\", nonce=\"abc\", algorithm=SHA-256",
        ])
        .unwrap_err();
        assert!(matches!(err, RtspVideoSourceError::UnsupportedDigestAlgorithm(algorithm) if algorithm == "SHA-256"));
    }

    #[test]
    fn debug_redacts_password() {
        let credentials =
            RtspCredentials { username: "admin".to_owned(), password: "secret".to_owned() };
        let debug = format!("{credentials:?}");
        assert!(debug.contains("admin"));
        assert!(!debug.contains("secret"));
    }

    #[test]
    fn cnonces_are_distinct() {
        assert_ne!(make_cnonce(), make_cnonce());
    }
}
