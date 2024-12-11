#[cfg(any(feature = "services-tokio", feature = "signal-client-tokio"))]
mod tokio {
    #[cfg(feature = "signal-client-tokio")]
    pub use reqwest::get;

    #[cfg(feature = "services-tokio")]
    pub use reqwest::Client;
}

#[cfg(any(feature = "services-tokio", feature = "signal-client-tokio"))]
pub use tokio::*;

#[cfg(all(
    any(feature = "signal-client-dispatcher", feature = "signal-client-async"),
    any(feature = "native-tls-vendored", feature = "rustls-tls-webpki-roots")
))]
compile_error!("The dispatcher and async signal clients do not support vendored or webpki roots");

#[cfg(any(feature = "services-dispatcher", feature = "signal-client-dispatcher"))]
mod dispatcher {
    use std::{future::Future, io, pin::Pin, sync::OnceLock};

    pub struct Response {
        pub body: Pin<Box<dyn futures_util::AsyncRead + Send>>,
        pub status: http::StatusCode,
    }

    pub trait HttpClient: 'static + Send + Sync {
        fn get(&self, url: &str) -> Pin<Box<dyn Future<Output = io::Result<Response>> + Send>>;
        fn send_async(
            &self,
            request: http::Request<Vec<u8>>,
        ) -> Pin<Box<dyn Future<Output = io::Result<Response>> + Send>>;
    }

    impl std::fmt::Debug for dyn HttpClient {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("dyn HttpClient").finish()
        }
    }

    static HTTP_CLIENT: OnceLock<&'static dyn HttpClient> = OnceLock::new();

    pub fn set_http_client(http_client: impl HttpClient) {
        let http_client = Box::leak(Box::new(http_client));
        HTTP_CLIENT.set(http_client).ok();
    }

    fn get_http_client() -> &'static dyn HttpClient {
        *HTTP_CLIENT.get().expect("Livekit requires a call to set_http_client()")
    }

    #[cfg(feature = "signal-client-dispatcher")]
    mod signal_client {
        use std::io;

        use futures_util::AsyncReadExt;

        use super::Response;

        impl Response {
            pub fn status(&self) -> http::StatusCode {
                self.status
            }

            pub async fn text(mut self) -> io::Result<String> {
                let mut string = String::new();
                self.body.read_to_string(&mut string).await?;
                Ok(string)
            }
        }

        pub async fn get(url: &str) -> io::Result<Response> {
            super::get_http_client().get(url).await
        }
    }

    #[cfg(feature = "signal-client-dispatcher")]
    pub use signal_client::*;

    #[cfg(feature = "services-dispatcher")]
    mod services {
        use std::io;

        use futures_util::AsyncReadExt;
        use prost::bytes::Bytes;

        use super::{get_http_client, HttpClient, Response};

        use url::Url;

        impl Response {
            pub async fn bytes(mut self) -> io::Result<Bytes> {
                let mut bytes = Vec::new();
                self.body.read_to_end(&mut bytes).await?;
                Ok(bytes.into())
            }

            pub async fn json<T: serde::de::DeserializeOwned + Unpin>(self) -> io::Result<T> {
                let bytes = self.bytes().await?;
                serde_json::from_slice::<T>(&bytes)
                    .map_err(|e| io::Error::new(io::ErrorKind::Other, e))
            }
        }

        #[derive(Debug)]
        pub struct Client {
            pub(crate) client: &'static dyn HttpClient,
        }

        impl Client {
            pub fn new() -> Self {
                Self { client: get_http_client() }
            }

            pub fn post(&self, url: Url) -> RequestBuilder {
                RequestBuilder {
                    body: Vec::new(),
                    builder: http::request::Builder::new().method("POST").uri(url.as_str()),
                    client: self.client,
                }
            }
        }

        pub struct RequestBuilder {
            pub(crate) builder: http::request::Builder,
            pub(crate) body: Vec<u8>,
            pub(crate) client: &'static dyn HttpClient,
        }
    }

    #[cfg(feature = "services-dispatcher")]
    pub use services::*;
}

#[cfg(feature = "signal-client-dispatcher")]
pub use dispatcher::*;

#[cfg(any(feature = "signal-client-async", feature = "services-async"))]
mod async_std {

    #[cfg(any(
        feature = "native-tls-vendored",
        feature = "rustls-tls-native-roots",
        feature = "rustls-tls-webpki-roots",
        feature = "__rustls-tls"
    ))]
    compile_error!("the async std compatible libraries do not support these features");

    #[cfg(any(feature = "signal-client-async", feature = "services-async"))]
    pub struct Response(http::Response<isahc::AsyncBody>);

    #[cfg(feature = "signal-client-async")]
    mod signal_client {
        use std::io;

        use isahc::AsyncReadResponseExt;

        use super::Response;

        impl Response {
            pub fn status(&self) -> http::StatusCode {
                self.0.status()
            }

            pub async fn text(mut self) -> io::Result<String> {
                self.0.text().await
            }
        }

        pub async fn get(url: &str) -> io::Result<Response> {
            let response = isahc::get_async(url).await?;
            Ok(Response(response))
        }
    }

    #[cfg(feature = "signal-client-async")]
    pub use signal_client::*;

    #[cfg(feature = "services-async")]
    mod services {
        use std::io;

        use isahc::AsyncReadResponseExt;
        use prost::bytes::Bytes;

        use super::Response;

        use url::Url;

        impl Response {
            pub async fn bytes(mut self) -> io::Result<Bytes> {
                Ok(self.0.bytes().await?.into())
            }

            pub async fn json<T: serde::de::DeserializeOwned + Unpin>(mut self) -> io::Result<T> {
                self.0.json().await.map_err(|e| io::Error::new(io::ErrorKind::Other, e))
            }
        }

        #[derive(Debug)]
        pub struct Client(isahc::HttpClient);

        impl Client {
            pub fn new() -> Self {
                Self(isahc::HttpClient::new().unwrap())
            }

            pub fn post(&self, url: Url) -> RequestBuilder {
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
    }

    #[cfg(feature = "services-async")]
    pub use services::*;
}

#[cfg(any(feature = "signal-client-async", feature = "services-async"))]
pub use async_std::*;

#[cfg(any(feature = "services-dispatcher", feature = "services-async"))]
impl RequestBuilder {
    pub fn headers(mut self, headers: http::HeaderMap) -> Self {
        use http::header::{Entry, OccupiedEntry};

        // Copied from: https://docs.rs/reqwest/0.11.24/src/reqwest/util.rs.html#62-89
        let self_headers = self.builder.headers_mut().unwrap();
        let mut prev_entry: Option<OccupiedEntry<_>> = None;
        for (key, value) in headers {
            match key {
                Some(key) => match self_headers.entry(key) {
                    Entry::Occupied(mut e) => {
                        e.insert(value);
                        prev_entry = Some(e);
                    }
                    Entry::Vacant(e) => {
                        let e = e.insert_entry(value);
                        prev_entry = Some(e);
                    }
                },
                None => match prev_entry {
                    Some(ref mut entry) => {
                        entry.append(value);
                    }
                    None => unreachable!("HeaderMap::into_iter yielded None first"),
                },
            }
        }
        self
    }

    pub fn body(mut self, body: Vec<u8>) -> Self {
        self.body = body;
        self
    }

    pub async fn send(self) -> std::io::Result<Response> {
        let request = self.builder.body(self.body).unwrap();
        let response = self.client.send_async(request).await?;
        Ok(response)
    }
}
