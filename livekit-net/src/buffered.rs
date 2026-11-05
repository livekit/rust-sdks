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

//! [`HttpStreamClient`] over a plain [`HttpClient`], for a host that registered
//! only the latter: the whole body, read up front and given as one chunk.

use std::sync::{Arc, Mutex};

use crate::{
    Header, HttpBodyStream, HttpClient, HttpMethod, HttpStreamClient, HttpStreamResponse,
    TransportError,
};

pub(crate) struct BufferedStreamClient(pub(crate) Arc<dyn HttpClient>);

#[async_trait::async_trait]
impl HttpStreamClient for BufferedStreamClient {
    async fn request_stream(
        &self,
        method: HttpMethod,
        url: String,
        headers: Vec<Header>,
        body: Option<Vec<u8>>,
    ) -> Result<HttpStreamResponse, TransportError> {
        let response = self.0.request(method, url, headers, body).await?;
        let chunk = (!response.body.is_empty()).then_some(response.body);
        Ok(HttpStreamResponse {
            status: response.status,
            headers: response.headers,
            body: Arc::new(BufferedBody(Mutex::new(chunk))),
        })
    }
}

/// The body as one chunk, then the end.
struct BufferedBody(Mutex<Option<Vec<u8>>>);

#[async_trait::async_trait]
impl HttpBodyStream for BufferedBody {
    async fn recv(&self) -> Result<Option<Vec<u8>>, TransportError> {
        // The guarded data is a plain `Option`, valid even if a holder panicked.
        Ok(self.0.lock().unwrap_or_else(|e| e.into_inner()).take())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::HttpResponse;

    struct Fixed(&'static [u8]);

    #[async_trait::async_trait]
    impl HttpClient for Fixed {
        async fn request(
            &self,
            _method: HttpMethod,
            _url: String,
            _headers: Vec<Header>,
            _body: Option<Vec<u8>>,
        ) -> Result<HttpResponse, TransportError> {
            let headers = vec![Header { name: "x-test".into(), value: "1".into() }];
            Ok(HttpResponse { status: 201, headers, body: self.0.to_vec() })
        }
    }

    async fn open(body: &'static [u8]) -> HttpStreamResponse {
        BufferedStreamClient(Arc::new(Fixed(body)))
            .request_stream(HttpMethod::Post, "http://x".into(), vec![], None)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn whole_body_is_one_chunk() {
        let response = open(b"hello").await;
        assert_eq!(response.status, 201);
        assert_eq!(response.headers[0].name, "x-test");
        assert_eq!(response.body.recv().await.unwrap(), Some(b"hello".to_vec()));
        assert_eq!(response.body.recv().await.unwrap(), None);
    }

    #[tokio::test]
    async fn empty_body_ends_at_once() {
        let response = open(b"").await;
        assert_eq!(response.body.recv().await.unwrap(), None);
    }
}
