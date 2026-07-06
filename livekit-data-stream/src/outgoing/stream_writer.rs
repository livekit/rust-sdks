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

use std::sync::Arc;
use tokio::sync::Mutex;

use crate::info::{ByteStreamInfo, TextStreamInfo};
use crate::outgoing::{constants::STREAM_CHUNK_SIZE_BYTES, RawStream};
use crate::utf8_chunk::Utf8AwareChunkExt;
use crate::utils::StreamResult;

/// Writer for an open data stream.
pub trait StreamWriter<'a> {
    /// Type of input this writer accepts.
    type Input: 'a;

    /// Information about the underlying data stream.
    type Info;

    /// Returns a reference to the stream info.
    fn info(&self) -> &Self::Info;

    /// Writes to the stream.
    fn write(
        &self,
        input: Self::Input,
    ) -> impl std::future::Future<Output = StreamResult<()>> + Send;

    /// Closes the stream normally.
    fn close(self) -> impl std::future::Future<Output = StreamResult<()>> + Send;

    /// Closes the stream abnormally, specifying the reason for closure.
    fn close_with_reason(
        self,
        reason: &str,
    ) -> impl std::future::Future<Output = StreamResult<()>> + Send;
}

#[derive(Clone)]
/// Writer for an open byte data stream.
pub struct ByteStreamWriter {
    info: Arc<ByteStreamInfo>,
    stream: Arc<Mutex<RawStream>>,
}

impl ByteStreamWriter {
    pub(crate) fn new(info: Arc<ByteStreamInfo>, stream: Arc<Mutex<RawStream>>) -> Self {
        Self { info, stream }
    }
}

#[derive(Clone)]
/// Writer for an open text data stream.
pub struct TextStreamWriter {
    info: Arc<TextStreamInfo>,
    stream: Arc<Mutex<RawStream>>,
}

impl TextStreamWriter {
    pub(crate) fn new(info: Arc<TextStreamInfo>, stream: Arc<Mutex<RawStream>>) -> Self {
        Self { info, stream }
    }
}

impl<'a> StreamWriter<'a> for ByteStreamWriter {
    type Input = &'a [u8];
    type Info = ByteStreamInfo;

    fn info(&self) -> &Self::Info {
        &self.info
    }

    async fn write(&self, bytes: &'a [u8]) -> StreamResult<()> {
        let mut stream = self.stream.lock().await;
        for chunk in bytes.chunks(STREAM_CHUNK_SIZE_BYTES) {
            stream.write_chunk(chunk).await?;
        }
        Ok(())
    }

    async fn close(self) -> StreamResult<()> {
        self.stream.lock().await.close(None).await
    }

    async fn close_with_reason(self, reason: &str) -> StreamResult<()> {
        self.stream.lock().await.close(Some(reason)).await
    }
}

impl<'a> StreamWriter<'a> for TextStreamWriter {
    type Input = &'a str;
    type Info = TextStreamInfo;

    fn info(&self) -> &Self::Info {
        &self.info
    }

    async fn write(&self, text: &'a str) -> StreamResult<()> {
        let mut stream = self.stream.lock().await;
        for chunk in text.as_bytes().utf8_aware_chunks(STREAM_CHUNK_SIZE_BYTES) {
            stream.write_chunk(chunk).await?;
        }
        Ok(())
    }

    async fn close(self) -> StreamResult<()> {
        self.stream.lock().await.close(None).await
    }

    async fn close_with_reason(self, reason: &str) -> StreamResult<()> {
        self.stream.lock().await.close(Some(reason)).await
    }
}
