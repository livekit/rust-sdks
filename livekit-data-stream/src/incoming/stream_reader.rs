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

use crate::{
    incoming::events::InputEvent,
    info::{AnyStreamInfo, ByteStreamInfo, TextStreamInfo},
    types::StreamId,
    utils::{StreamError, StreamProgress, StreamResult},
};
use bytes::{Bytes, BytesMut};
use futures_util::{Stream, StreamExt};
use std::{
    fmt::Debug,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::sync::{
    mpsc::{self, UnboundedReceiver, UnboundedSender, WeakUnboundedSender},
    watch,
};
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
    closer: StreamCloser,
}

/// Reader for an incoming text data stream.
pub struct TextStreamReader {
    info: TextStreamInfo,
    chunk_rx: UnboundedReceiver<StreamResult<Bytes>>,
    progress_rx: watch::Receiver<StreamProgress>,
    closer: StreamCloser,
}

/// Ends an incoming stream from outside its reader, for example while another task reads it.
///
/// Get one from [`ByteStreamReader::closer`] or [`TextStreamReader::closer`].
#[derive(Clone, Debug)]
pub struct StreamCloser {
    /// The incoming manager's input. Weak, so a closer does not keep the manager running.
    input: WeakUnboundedSender<InputEvent>,
    pub(crate) stream_id: StreamId,
    /// Tells this stream apart from a later stream that reuses its id.
    pub(crate) instance: u64,
}

impl StreamCloser {
    pub(crate) fn new(
        input: WeakUnboundedSender<InputEvent>,
        stream_id: StreamId,
        instance: u64,
    ) -> Self {
        Self { input, stream_id, instance }
    }

    /// A closer that does nothing, for readers made outside a manager.
    #[cfg(any(test, feature = "test-utils"))]
    fn detached() -> Self {
        let (input, _) = mpsc::unbounded_channel();
        Self::new(input.downgrade(), StreamId::from(""), 0)
    }

    /// Ends the stream on the receiving side. The incoming manager drops the stream's state and
    /// reports it closed, and a read in progress fails with [`StreamError::AlreadyClosed`] after
    /// the chunks that already arrived.
    ///
    /// Does nothing once the stream has ended, and never affects a later stream that reuses its
    /// id.
    pub fn close(&self) {
        if let Some(input) = self.input.upgrade() {
            let _ = input.send(InputEvent::CloseStream(self.clone()));
        }
    }
}

impl StreamReader for ByteStreamReader {
    type Output = Bytes;
    type Info = ByteStreamInfo;

    fn info(&self) -> &ByteStreamInfo {
        &self.info
    }

    fn progress(&self) -> impl Stream<Item = StreamProgress> {
        self.progress_stream()
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
    /// Like [`StreamReader::progress`], but the stream does not borrow the reader, so it can be
    /// polled while the reader is read or after the reader is consumed.
    pub fn progress_stream(&self) -> impl Stream<Item = StreamProgress> + Send + Unpin + 'static {
        WatchStream::new(self.progress_rx.clone())
    }

    /// Returns a handle that ends this stream from outside the reader. See [`StreamCloser`].
    pub fn closer(&self) -> StreamCloser {
        self.closer.clone()
    }

    /// Reads incoming chunks from the byte stream, writing them to a file as they are received.
    ///
    /// Parameters:
    ///   - directory: The directory to write the file in. The system temporary directory is used if not specified.
    ///   - name_override: The name to use for the written file, overriding stream name.
    ///
    /// Returns: The path of the written file on disk.
    ///
    /// Errors with [`StreamError::InvalidFileName`] if the file name (whether from the stream
    /// info or `name_override`) is not a plain file name, i.e. contains path separators, `..`,
    /// or is absolute.
    ///
    pub async fn write_to_file(
        mut self,
        directory: Option<impl AsRef<std::path::Path>>,
        name_override: Option<&str>,
    ) -> StreamResult<std::path::PathBuf> {
        let directory =
            directory.map(|d| d.as_ref().to_path_buf()).unwrap_or_else(std::env::temp_dir);
        let name = name_override.unwrap_or_else(|| &self.info.name);
        // The stream name comes from the remote sender: reject anything that isn't a plain
        // file name so it can't escape the target directory.
        let mut components = std::path::Path::new(name).components();
        if !matches!(components.next(), Some(std::path::Component::Normal(_)))
            || components.next().is_some()
        {
            return Err(StreamError::InvalidFileName);
        }
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

impl TextStreamReader {
    /// Like [`StreamReader::progress`], but the stream does not borrow the reader, so it can be
    /// polled while the reader is read or after the reader is consumed.
    pub fn progress_stream(&self) -> impl Stream<Item = StreamProgress> + Send + Unpin + 'static {
        WatchStream::new(self.progress_rx.clone())
    }

    /// Returns a handle that ends this stream from outside the reader. See [`StreamCloser`].
    pub fn closer(&self) -> StreamCloser {
        self.closer.clone()
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
        Self { info, chunk_rx, progress_rx, closer: StreamCloser::detached() }
    }
}

impl StreamReader for TextStreamReader {
    type Output = String;
    type Info = TextStreamInfo;

    fn info(&self) -> &TextStreamInfo {
        &self.info
    }

    fn progress(&self) -> impl Stream<Item = StreamProgress> {
        self.progress_stream()
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
    /// the stream is finite). `closer` is what the reader's [`StreamCloser`] sends.
    pub(super) fn from(
        info: AnyStreamInfo,
        closer: StreamCloser,
    ) -> (Self, UnboundedSender<StreamResult<Bytes>>, watch::Sender<StreamProgress>) {
        let (chunk_tx, chunk_rx) = mpsc::unbounded_channel();
        let (progress_tx, progress_rx) = watch::channel(StreamProgress {
            bytes_total: info.total_length(),
            ..Default::default()
        });
        let reader = match info {
            AnyStreamInfo::Byte(info) => {
                Self::Byte(ByteStreamReader { info, chunk_rx, progress_rx, closer })
            }
            AnyStreamInfo::Text(info) => {
                Self::Text(TextStreamReader { info, chunk_rx, progress_rx, closer })
            }
        };
        (reader, chunk_tx, progress_tx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ByteHeader, CompressionType, Header, StreamId};
    use std::collections::HashMap;

    /// Creates a byte stream reader whose stream info carries the given (potentially
    /// remote-controlled) file name.
    fn byte_reader(name: &str) -> (ByteStreamReader, UnboundedSender<StreamResult<Bytes>>) {
        let header = Header {
            stream_id: StreamId::from("stream-1"),
            timestamp: 0,
            topic: "topic".to_string(),
            mime_type: "application/octet-stream".to_string(),
            total_length: None,
            attributes: HashMap::new(),
            inline_content: None,
            compression: CompressionType::None,
            content_header: Some(ByteHeader { name: name.to_string() }.into()),
        };
        let AnyStreamInfo::Byte(info) = AnyStreamInfo::try_from(header).expect("valid header")
        else {
            panic!("expected a byte stream info");
        };
        let (chunk_tx, chunk_rx) = mpsc::unbounded_channel();
        let (_, progress_rx) = watch::channel(StreamProgress::default());
        (
            ByteStreamReader { info, chunk_rx, progress_rx, closer: StreamCloser::detached() },
            chunk_tx,
        )
    }

    #[tokio::test]
    async fn write_to_file_rejects_traversal_in_stream_name() {
        let (reader, _chunk_tx) = byte_reader("../evil.txt");
        let result = reader.write_to_file(None::<&std::path::Path>, None).await;
        assert!(matches!(result, Err(StreamError::InvalidFileName)));
    }

    #[tokio::test]
    async fn write_to_file_rejects_traversal_in_name_override() {
        let (reader, _chunk_tx) = byte_reader("safe.txt");
        let result = reader.write_to_file(None::<&std::path::Path>, Some("../evil.txt")).await;
        assert!(matches!(result, Err(StreamError::InvalidFileName)));
    }

    #[tokio::test]
    async fn write_to_file_rejects_absolute_path_in_stream_name() {
        let (reader, _chunk_tx) = byte_reader("/etc/evil.txt");
        let result = reader.write_to_file(None::<&std::path::Path>, None).await;
        assert!(matches!(result, Err(StreamError::InvalidFileName)));
    }

    #[tokio::test]
    async fn write_to_file_rejects_nested_path_in_stream_name() {
        let (reader, _chunk_tx) = byte_reader("nested/evil.txt");
        let result = reader.write_to_file(None::<&std::path::Path>, None).await;
        assert!(matches!(result, Err(StreamError::InvalidFileName)));
    }

    #[tokio::test]
    async fn write_to_file_rejects_empty_stream_name() {
        let (reader, _chunk_tx) = byte_reader("");
        let result = reader.write_to_file(None::<&std::path::Path>, None).await;
        assert!(matches!(result, Err(StreamError::InvalidFileName)));
    }

    #[tokio::test]
    async fn write_to_file_accepts_plain_name() {
        let directory =
            std::env::temp_dir().join(format!("lk-stream-test-{}", uuid::Uuid::new_v4()));
        tokio::fs::create_dir_all(&directory).await.expect("failed to create test directory");

        let (reader, chunk_tx) = byte_reader("file.txt");
        chunk_tx.send(Ok(Bytes::from_static(b"hello"))).expect("failed to send chunk");
        drop(chunk_tx);

        let path = reader
            .write_to_file(Some(&directory), None)
            .await
            .expect("write_to_file should succeed for a plain file name");
        assert_eq!(path, directory.join("file.txt"));
        assert_eq!(tokio::fs::read(&path).await.expect("failed to read file"), b"hello");

        tokio::fs::remove_dir_all(&directory).await.expect("failed to clean up test directory");
    }
}
