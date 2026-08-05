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

#[cfg(feature = "services-tokio")]
mod tokio {
    pub use reqwest::Client;
}

#[cfg(feature = "services-tokio")]
pub use tokio::*;

#[cfg(feature = "services-async")]
mod async_std {

    #[cfg(any(
        feature = "native-tls-vendored",
        feature = "rustls-tls-native-roots",
        feature = "rustls-tls-webpki-roots",
        feature = "__rustls-tls"
    ))]
    compile_error!("the async std compatible libraries do not support these features");

    use std::io;

    use isahc::AsyncReadResponseExt;

    // isahc vendors its own `http` 0.2, so the response wraps isahc's type and
    // its `http` 1.x-facing accessors (`status`) convert at the edge.
    pub struct Response(isahc::http::Response<isahc::AsyncBody>);

    impl Response {
        /// Status as the workspace's `http` 1.x `StatusCode` (what callers
        /// compare against), round-tripped from isahc's `http` 0.2 code.
        pub fn status(&self) -> http::StatusCode {
            http::StatusCode::from_u16(self.0.status().as_u16()).expect("valid status code")
        }

        /// Decodes a JSON body. Used by the server-API (twirp error) backend.
        pub async fn json<T: serde::de::DeserializeOwned + Unpin>(mut self) -> io::Result<T> {
            self.0.json().await.map_err(|e| io::Error::new(io::ErrorKind::Other, e))
        }

        /// Reads the raw protobuf body. Server-API (twirp) backend only.
        pub async fn bytes(mut self) -> io::Result<prost::bytes::Bytes> {
            Ok(self.0.bytes().await?.into())
        }
    }

    // Shared isahc HTTP client: the server-API backend POSTs Twirp requests.
    // Clone is cheap and shares the underlying connection pool (isahc::HttpClient
    // is reference-counted), so the unified client can hand one to every service.
    #[derive(Debug, Clone)]
    pub struct Client(isahc::HttpClient);

    impl Client {
        pub fn new() -> Self {
            Self(isahc::HttpClient::new().unwrap())
        }

        pub fn post(&self, url: url::Url) -> RequestBuilder {
            RequestBuilder {
                body: Vec::new(),
                builder: isahc::http::Request::post(url.as_str()),
                client: self.0.clone(),
            }
        }
    }

    pub struct RequestBuilder {
        builder: isahc::http::request::Builder,
        body: Vec<u8>,
        client: isahc::HttpClient,
    }

    impl RequestBuilder {
        pub fn headers(mut self, headers: http::HeaderMap) -> Self {
            // isahc vendors `http` 0.2, so rebuild the workspace's `http` 1.x
            // `HeaderMap` into isahc's own type. Names/values round-trip through
            // bytes to stay agnostic to the `http` version. `HeaderMap`'s iterator
            // yields `None` keys for repeated values, so carry the last name.
            let dst = self.builder.headers_mut().unwrap();
            let mut last_name: Option<isahc::http::HeaderName> = None;
            for (key, value) in headers {
                let name = match key {
                    Some(key) => {
                        let name = isahc::http::HeaderName::from_bytes(key.as_str().as_bytes())
                            .expect("valid header name");
                        last_name = Some(name.clone());
                        name
                    }
                    None => last_name.clone().expect("HeaderMap yielded a value before any key"),
                };
                let value = isahc::http::HeaderValue::from_bytes(value.as_bytes())
                    .expect("valid header value");
                dst.append(name, value);
            }
            self
        }

        pub fn body(mut self, body: Vec<u8>) -> Self {
            self.body = body;
            self
        }

        pub fn timeout(mut self, timeout: std::time::Duration) -> Self {
            use isahc::config::Configurable;
            self.builder = self.builder.timeout(timeout);
            self
        }

        pub async fn send(self) -> io::Result<Response> {
            let request = self.builder.body(self.body).unwrap();
            let response = self.client.send_async(request).await?;
            Ok(Response(response))
        }
    }
}

#[cfg(feature = "services-async")]
pub use async_std::*;
