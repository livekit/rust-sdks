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

use crate::info::{AnyStreamInfo, ByteStreamInfo, TextStreamInfo};
use crate::utils::{StreamError, StreamProgress, StreamResult};
use bytes::{Bytes, BytesMut};
use futures_util::{Stream, StreamExt};
use std::{
    fmt::Debug,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use tokio::sync::watch;
use tokio_stream::wrappers::WatchStream;

/// Reader for an incoming data stream.
///
/// The stream being read from is kept open as long as its reader exists;
/// dropping the reader will close the stream.
///
pub trait StreamReader: Stream<Item = StreamResult<Self::Output>> {
    /// Type of output this reader produces.
    type Output;

    /// Information about the underlying data stream.
    type Info;

    /// Returns a reference to the stream info.
    fn info(&self) -> &Self::Info;

    /// Returns a stream of [`StreamProgress`] events as the stream is incrementally received from
    /// the sender participant.
    fn progress(&self) -> impl Stream<Item = StreamProgress>;

    /// Reads all incoming chunks from the byte stream, concatenating them
    /// into a single value which is returned once the stream closes normally.
    ///
    /// Returns the data consisting of all concatenated chunks.
    ///
    fn read_all(self) -> impl std::future::Future<Output = StreamResult<Self::Output>> + Send;
}

/// Reader for an incoming byte data stream.
pub struct ByteStreamReader {
    info: ByteStreamInfo,
    chunk_rx: UnboundedReceiver<StreamResult<Bytes>>,
    progress_rx: watch::Receiver<StreamProgress>,
}

/// Reader for an incoming text data stream.
pub struct TextStreamReader {
    info: TextStreamInfo,
    chunk_rx: UnboundedReceiver<StreamResult<Bytes>>,
    progress_rx: watch::Receiver<StreamProgress>,
}

impl StreamReader for ByteStreamReader {
    type Output = Bytes;
    type Info = ByteStreamInfo;

    fn info(&self) -> &ByteStreamInfo {
        &self.info
    }

    fn progress(&self) -> impl Stream<Item = StreamProgress> {
        WatchStream::new(self.progress_rx.clone())
    }

    async fn read_all(mut self) -> StreamResult<Bytes> {
        let mut buffer = BytesMut::new();
        while let Some(result) = self.next().await {
            match result {
                Ok(bytes) => buffer.extend_from_slice(&bytes),
                Err(e) => return Err(e),
            }
        }
        Ok(buffer.freeze())
    }
}

impl ByteStreamReader {
    /// Reads incoming chunks from the byte stream, writing them to a file as they are received.
    ///
    /// Parameters:
    ///   - directory: The directory to write the file in. The system temporary directory is used if not specified.
    ///   - name_override: The name to use for the written file, overriding stream name.
    ///
    /// Returns: The path of the written file on disk.
    ///
    pub async fn write_to_file(
        mut self,
        directory: Option<impl AsRef<std::path::Path>>,
        name_override: Option<&str>,
    ) -> StreamResult<std::path::PathBuf> {
        let directory =
            directory.map(|d| d.as_ref().to_path_buf()).unwrap_or_else(|| std::env::temp_dir());
        let name = name_override.unwrap_or_else(|| &self.info.name);
        let file_path = directory.join(name);

        let mut file = tokio::fs::File::create(&file_path).await.map_err(StreamError::Io)?;

        while let Some(result) = self.next().await {
            let bytes = result?;
            tokio::io::AsyncWriteExt::write_all(&mut file, &bytes)
                .await
                .map_err(StreamError::Io)?;
        }
        tokio::io::AsyncWriteExt::flush(&mut file).await.map_err(StreamError::Io)?;

        Ok(file_path)
    }
}

impl Stream for ByteStreamReader {
    type Item = StreamResult<Bytes>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        match Pin::new(&mut this.chunk_rx).poll_recv(cx) {
            Poll::Ready(Some(Ok(chunk))) => Poll::Ready(Some(Ok(chunk))),
            Poll::Ready(Some(Err(e))) => Poll::Ready(Some(Err(e))),
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

#[cfg(any(test, feature = "test-utils"))]
impl TextStreamReader {
    /// Create a TextStreamReader for testing purposes.
    ///
    /// Exposed under the `test-utils` feature so downstream crates (e.g. `livekit`'s RPC tests)
    /// can construct a reader directly.
    pub fn new_for_test(
        info: TextStreamInfo,
        chunk_rx: UnboundedReceiver<StreamResult<Bytes>>,
    ) -> Self {
        // The progress channel is unused by these tests; seed it and drop the sender so the
        // progress stream simply ends after the initial value.
        let (_, progress_rx) = watch::channel(StreamProgress::default());
        Self { info, chunk_rx, progress_rx }
    }
}

impl StreamReader for TextStreamReader {
    type Output = String;
    type Info = TextStreamInfo;

    fn info(&self) -> &TextStreamInfo {
        &self.info
    }

    fn progress(&self) -> impl Stream<Item = StreamProgress> {
        WatchStream::new(self.progress_rx.clone())
    }

    async fn read_all(mut self) -> StreamResult<String> {
        let mut result = String::new();
        while let Some(chunk) = self.next().await {
            match chunk {
                Ok(text) => result.push_str(&text),
                Err(e) => return Err(e),
            }
        }
        Ok(result)
    }
}

impl Stream for TextStreamReader {
    type Item = StreamResult<String>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        match Pin::new(&mut this.chunk_rx).poll_recv(cx) {
            Poll::Ready(Some(Ok(chunk))) => match String::from_utf8(chunk.into()) {
                Ok(content) => Poll::Ready(Some(Ok(content))),
                Err(e) => {
                    this.chunk_rx.close();
                    Poll::Ready(Some(Err(StreamError::from(e))))
                }
            },
            Poll::Ready(Some(Err(e))) => Poll::Ready(Some(Err(e))),
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl Debug for ByteStreamReader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ByteStreamReader")
            .field("id", &self.info.id())
            .field("topic", &self.info.topic)
            .finish()
    }
}

impl Debug for TextStreamReader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TextStreamReader")
            .field("id", &self.info.id())
            .field("topic", &self.info.topic)
            .finish()
    }
}

pub enum AnyStreamReader {
    Byte(ByteStreamReader),
    Text(TextStreamReader),
}

impl AnyStreamReader {
    /// Creates a stream reader for the stream with the given info.
    ///
    /// Returns the reader along with the sender halves the manager uses to feed it: the chunk
    /// channel for received content, and the progress channel for [`StreamProgress`] updates. The
    /// progress channel is seeded with the initial progress (0 bytes, plus the total length when
    /// the stream is finite).
    pub(super) fn from(
        info: AnyStreamInfo,
    ) -> (Self, UnboundedSender<StreamResult<Bytes>>, watch::Sender<StreamProgress>) {
        let (chunk_tx, chunk_rx) = mpsc::unbounded_channel();
        let (progress_tx, progress_rx) = watch::channel(StreamProgress {
            bytes_total: info.total_length(),
            ..Default::default()
        });
        let reader = match info {
            AnyStreamInfo::Byte(info) => {
                Self::Byte(ByteStreamReader { info, chunk_rx, progress_rx })
            }
            AnyStreamInfo::Text(info) => {
                Self::Text(TextStreamReader { info, chunk_rx, progress_rx })
            }
        };
        return (reader, chunk_tx, progress_tx);
    }
}
