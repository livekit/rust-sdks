// Copyright 2025 LiveKit, Inc.
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

//! HMAC-only `jsonwebtoken` crypto provider (HS256/384/512).
//!
//! LiveKit access tokens are HS256, so livekit-api links only the HMAC backend
//! rather than jsonwebtoken's `rust_crypto` bundle (which also pulls in RSA, EC
//! and EdDSA — ~275 KiB of code that's never reachable through this crate's API).
//! jsonwebtoken 10 resolves its backend through a process-global `CryptoProvider`;
//! [`ensure_installed`] registers this one on first use.

use std::sync::Once;

use hmac::{Hmac, Mac};
use jsonwebtoken::crypto::{CryptoProvider, JwkUtils, JwtSigner, JwtVerifier};
use jsonwebtoken::errors::{Error, ErrorKind, Result};
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey};
use sha2::{Sha256, Sha384, Sha512};
use signature::{Error as SignatureError, Signer, Verifier};

type HmacSha256 = Hmac<Sha256>;
type HmacSha384 = Hmac<Sha384>;
type HmacSha512 = Hmac<Sha512>;

macro_rules! hmac_signer {
    ($name:ident, $alg:expr, $hmac:ty) => {
        struct $name($hmac);

        impl $name {
            fn new(key: &EncodingKey) -> Result<Self> {
                <$hmac>::new_from_slice(key.try_get_hmac_secret()?)
                    .map(Self)
                    .map_err(|_| Error::from(ErrorKind::InvalidKeyFormat))
            }
        }

        impl Signer<Vec<u8>> for $name {
            fn try_sign(&self, msg: &[u8]) -> std::result::Result<Vec<u8>, SignatureError> {
                let mut mac = self.0.clone();
                mac.update(msg);
                Ok(mac.finalize().into_bytes().to_vec())
            }
        }

        impl JwtSigner for $name {
            fn algorithm(&self) -> Algorithm {
                $alg
            }
        }
    };
}

macro_rules! hmac_verifier {
    ($name:ident, $alg:expr, $hmac:ty) => {
        struct $name($hmac);

        impl $name {
            fn new(key: &DecodingKey) -> Result<Self> {
                <$hmac>::new_from_slice(key.try_get_hmac_secret()?)
                    .map(Self)
                    .map_err(|_| Error::from(ErrorKind::InvalidKeyFormat))
            }
        }

        impl Verifier<Vec<u8>> for $name {
            fn verify(
                &self,
                msg: &[u8],
                signature: &Vec<u8>,
            ) -> std::result::Result<(), SignatureError> {
                let mut mac = self.0.clone();
                mac.update(msg);
                // verify_slice is constant-time.
                mac.verify_slice(signature).map_err(SignatureError::from_source)
            }
        }

        impl JwtVerifier for $name {
            fn algorithm(&self) -> Algorithm {
                $alg
            }
        }
    };
}

hmac_signer!(Hs256Signer, Algorithm::HS256, HmacSha256);
hmac_signer!(Hs384Signer, Algorithm::HS384, HmacSha384);
hmac_signer!(Hs512Signer, Algorithm::HS512, HmacSha512);
hmac_verifier!(Hs256Verifier, Algorithm::HS256, HmacSha256);
hmac_verifier!(Hs384Verifier, Algorithm::HS384, HmacSha384);
hmac_verifier!(Hs512Verifier, Algorithm::HS512, HmacSha512);

fn signer_factory(alg: &Algorithm, key: &EncodingKey) -> Result<Box<dyn JwtSigner>> {
    match alg {
        Algorithm::HS256 => Ok(Box::new(Hs256Signer::new(key)?) as Box<dyn JwtSigner>),
        Algorithm::HS384 => Ok(Box::new(Hs384Signer::new(key)?) as Box<dyn JwtSigner>),
        Algorithm::HS512 => Ok(Box::new(Hs512Signer::new(key)?) as Box<dyn JwtSigner>),
        _ => Err(Error::from(ErrorKind::InvalidAlgorithm)),
    }
}

fn verifier_factory(alg: &Algorithm, key: &DecodingKey) -> Result<Box<dyn JwtVerifier>> {
    match alg {
        Algorithm::HS256 => Ok(Box::new(Hs256Verifier::new(key)?) as Box<dyn JwtVerifier>),
        Algorithm::HS384 => Ok(Box::new(Hs384Verifier::new(key)?) as Box<dyn JwtVerifier>),
        Algorithm::HS512 => Ok(Box::new(Hs512Verifier::new(key)?) as Box<dyn JwtVerifier>),
        _ => Err(Error::from(ErrorKind::InvalidAlgorithm)),
    }
}

static PROVIDER: CryptoProvider =
    CryptoProvider { signer_factory, verifier_factory, jwk_utils: JwkUtils::new_unimplemented() };

/// Register the HMAC-only provider as jsonwebtoken's process default.
///
/// Idempotent and cheap; call before any `encode`/`decode`. If another provider
/// was already installed in this process, this is a no-op and that one is used.
pub(crate) fn ensure_installed() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let _ = PROVIDER.install_default();
    });
}
