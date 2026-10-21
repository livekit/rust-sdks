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

// Server-API (services) HTTP client only. The signal client's transport and its
// region/validate HTTP calls now live in livekit-net, so nothing here is gated
// on the signal-client features.

pub use reqwest::Client;

pub fn client() -> Client {
    builder().build().expect("http client")
}

/// A reqwest builder on the crate's TLS feature. With rustls, the TLS config is built on ring,
/// the provider livekit-net's websocket uses: reqwest 0.13 defaults to aws-lc-rs, and a process
/// with both providers and no installed default fails every `ClientConfig::builder()`.
pub fn builder() -> reqwest::ClientBuilder {
    let builder = Client::builder();
    #[cfg(feature = "__rustls-tls")]
    let builder = builder.tls_backend_preconfigured(rustls_config());
    builder
}

#[cfg(feature = "__rustls-tls")]
fn rustls_config() -> rustls::ClientConfig {
    let mut roots = rustls::RootCertStore::empty();
    #[cfg(feature = "rustls-tls-webpki-roots")]
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    #[cfg(feature = "rustls-tls-native-roots")]
    roots.add_parsable_certificates(rustls_native_certs::load_native_certs().certs);
    let provider = std::sync::Arc::new(rustls::crypto::ring::default_provider());
    rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .expect("ring supports the default TLS versions")
        .with_root_certificates(roots)
        .with_no_client_auth()
}
