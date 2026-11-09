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

use std::collections::HashMap;

use bytes::Bytes;
use livekit_data_stream::api as ds_api;

use super::common::{ByteStreamInfo, DataStreamError, StreamProgress, TextStreamInfo};
use ds_api::StreamWriter as _;

/// Receives the progress of an outgoing send, implemented by the foreign side.
#[uniffi::export(with_foreign)]
pub trait StreamProgressDelegate: Send + Sync {
    /// Reports how much of the payload was sent. Called on the thread that polls the send, so it
    /// must return quickly.
    ///
    /// It must not throw. The method returns no error, so UniFFI turns an exception from a Kotlin
    /// or Dart implementation into a Rust panic, and the release profile aborts the process.
    fn on_progress(&self, progress: StreamProgress);
}

impl std::fmt::Debug for dyn StreamProgressDelegate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("StreamProgressDelegate")
    }
}

/// Writer for an open text data stream.
#[derive(uniffi::Object)]
pub struct TextStreamWriter {
    inner: ds_api::TextStreamWriter,
    // After the migration is complete, this field can be removed.
    #[cfg(feature = "room-apis")]
    handle_id: std::sync::OnceLock<crate::FfiHandleId>,
}

impl TextStreamWriter {
    pub(crate) fn new(inner: ds_api::TextStreamWriter) -> Self {
        Self {
            inner,
            #[cfg(feature = "room-apis")]
            handle_id: std::sync::OnceLock::new(),
        }
    }
}

#[uniffi::export(async_runtime = "tokio")]
impl TextStreamWriter {
    /// Information about the underlying stream.
    pub fn info(&self) -> TextStreamInfo {
        self.inner.info().clone().into()
    }

    /// Whether the stream is still open — false once it has been closed locally or a send has failed.
    pub async fn is_open(&self) -> bool {
        !self.inner.is_closed().await
    }

    /// Appends text to the stream.
    pub async fn write(&self, text: String) -> Result<(), DataStreamError> {
        Ok(self.inner.write(&text).await?)
    }

    /// Closes the stream. A `reason` closes it abnormally, and `attributes` go in its trailer,
    /// where the receiver merges them into the stream's attributes.
    #[uniffi::method(default(reason = None, attributes))]
    pub async fn close(
        &self,
        reason: Option<String>,
        attributes: HashMap<String, String>,
    ) -> Result<(), DataStreamError> {
        Ok(self.inner.clone().close_with_options(reason.as_deref(), Some(attributes)).await?)
    }

    /// Closes the stream abnormally with a reason.
    pub async fn close_with_reason(&self, reason: String) -> Result<(), DataStreamError> {
        Ok(self.inner.clone().close_with_reason(&reason).await?)
    }
}

/// Writer for an open byte data stream.
#[derive(uniffi::Object)]
pub struct ByteStreamWriter {
    inner: ds_api::ByteStreamWriter,
    // After the migration is complete, this field can be removed.
    #[cfg(feature = "room-apis")]
    handle_id: std::sync::OnceLock<crate::FfiHandleId>,
}

impl ByteStreamWriter {
    pub(crate) fn new(inner: ds_api::ByteStreamWriter) -> Self {
        Self {
            inner,
            #[cfg(feature = "room-apis")]
            handle_id: std::sync::OnceLock::new(),
        }
    }
}

#[uniffi::export(async_runtime = "tokio")]
impl ByteStreamWriter {
    /// Information about the underlying stream.
    pub fn info(&self) -> ByteStreamInfo {
        self.inner.info().clone().into()
    }

    /// Whether the stream is still open — false once it has been closed locally or a send has failed.
    pub async fn is_open(&self) -> bool {
        !self.inner.is_closed().await
    }

    /// Appends bytes to the stream.
    pub async fn write(&self, data: Bytes) -> Result<(), DataStreamError> {
        Ok(self.inner.write(data.as_ref()).await?)
    }

    /// Closes the stream. A `reason` closes it abnormally, and `attributes` go in its trailer,
    /// where the receiver merges them into the stream's attributes.
    #[uniffi::method(default(reason = None, attributes))]
    pub async fn close(
        &self,
        reason: Option<String>,
        attributes: HashMap<String, String>,
    ) -> Result<(), DataStreamError> {
        Ok(self.inner.clone().close_with_options(reason.as_deref(), Some(attributes)).await?)
    }

    /// Closes the stream abnormally with a reason.
    pub async fn close_with_reason(&self, reason: String) -> Result<(), DataStreamError> {
        Ok(self.inner.clone().close_with_reason(&reason).await?)
    }
}
