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

use super::{
    AnyStreamInfo, ByteStreamInfo, StreamError, StreamProgress, StreamResult, TextStreamInfo,
};
use crate::id::ParticipantIdentity;
use crate::utils::handler::AsyncHandlerRegistry;
use bytes::{Bytes, BytesMut};
use futures_util::{Stream, StreamExt};
use livekit_protocol::data_stream as proto;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::error::Error;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};

/// Reader for an incoming data stream.
pub trait StreamReader: Stream<Item = StreamResult<(Self::Output, StreamProgress)>> {
    /// Type of output this reader produces.
    type Output;

    /// Information about the underlying data stream.
    type Info;

    /// Returns a reference to the stream info.
    fn info(&self) -> &Self::Info;

    /// Reads all incoming chunks from the byte stream, concatenating them
    /// into a single value which is returned once the stream closes normally.
    ///
    /// Returns the data consisting of all concatenated chunks.
    ///
    fn read_all(self) -> impl std::future::Future<Output = StreamResult<Self::Output>> + Send;
}

pub(super) type ChunkSender = UnboundedSender<StreamResult<IncomingChunk>>;
type ChunkReceiver = UnboundedReceiver<StreamResult<IncomingChunk>>;

/// Reader for an incoming byte data stream.
pub struct ByteStreamReader {
    info: ByteStreamInfo,
    rx: ChunkReceiver,
}

/// Reader for an incoming text data stream.
pub struct TextStreamReader {
    info: TextStreamInfo,
    rx: ChunkReceiver,
}

impl StreamReader for ByteStreamReader {
    type Output = Bytes;
    type Info = ByteStreamInfo;

    fn info(&self) -> &ByteStreamInfo {
        &self.info
    }

    async fn read_all(mut self) -> StreamResult<Bytes> {
        let mut buffer = BytesMut::new();
        while let Some(result) = self.next().await {
            match result {
                Ok((bytes, _)) => buffer.extend_from_slice(&bytes),
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
            let (bytes, _) = result?;
            tokio::io::AsyncWriteExt::write_all(&mut file, &bytes)
                .await
                .map_err(StreamError::Io)?;
        }
        tokio::io::AsyncWriteExt::flush(&mut file).await.map_err(StreamError::Io)?;

        Ok(file_path)
    }
}

impl Stream for ByteStreamReader {
    type Item = StreamResult<(Bytes, StreamProgress)>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        match Pin::new(&mut this.rx).poll_recv(cx) {
            Poll::Ready(Some(Ok(chunk))) => Poll::Ready(Some(Ok((chunk.content, chunk.progress)))),
            Poll::Ready(Some(Err(e))) => Poll::Ready(Some(Err(e))),
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl StreamReader for TextStreamReader {
    type Output = String;
    type Info = TextStreamInfo;

    fn info(&self) -> &TextStreamInfo {
        &self.info
    }

    async fn read_all(mut self) -> StreamResult<String> {
        let mut result = String::new();
        while let Some(chunk) = self.next().await {
            match chunk {
                Ok((text, _)) => result.push_str(&text),
                Err(e) => return Err(e),
            }
        }
        Ok(result)
    }
}

impl Stream for TextStreamReader {
    type Item = StreamResult<(String, StreamProgress)>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        match Pin::new(&mut this.rx).poll_recv(cx) {
            Poll::Ready(Some(Ok(chunk))) => match String::from_utf8(chunk.content.into()) {
                Ok(content) => Poll::Ready(Some(Ok((content, chunk.progress)))),
                Err(e) => {
                    this.rx.close();
                    Poll::Ready(Some(Err(StreamError::from(e))))
                }
            },
            Poll::Ready(Some(Err(e))) => Poll::Ready(Some(Err(e))),
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

pub enum AnyStreamReader {
    Byte(ByteStreamReader),
    Text(TextStreamReader),
}

impl AnyStreamReader {
    /// Creates a stream reader for the stream with the given info.
    pub(super) fn from(info: AnyStreamInfo) -> (Self, ChunkSender) {
        let (tx, rx) = mpsc::unbounded_channel();
        let reader = match info {
            AnyStreamInfo::Byte(info) => Self::Byte(ByteStreamReader { info, rx }),
            AnyStreamInfo::Text(info) => Self::Text(TextStreamReader { info, rx }),
        };
        return (reader, tx);
    }
}

pub struct IncomingChunk {
    pub progress: StreamProgress,
    pub content: Bytes,
}

struct Descriptor {
    progress: StreamProgress,
    chunk_tx: ChunkSender,
    // TODO: keep track of open time.
}

#[derive(Clone)]
pub(crate) struct IncomingStreamManager {
    inner: Arc<Mutex<ManagerInner>>,
}

#[derive(Default)]
struct ManagerInner {
    open_streams: HashMap<String, Descriptor>,
    handlers: Handlers,
}

impl IncomingStreamManager {
    pub fn new() -> Self {
        Self { inner: Arc::new(Mutex::new(Default::default())) }
    }

    /// Handles an incoming header packet.
    pub fn handle_header(&self, header: proto::Header, identity: String) {
        let Ok(info) =
            AnyStreamInfo::try_from(header).inspect_err(|e| log::error!("Invalid header: {}", e))
        else {
            return;
        };

        let id = info.id().to_owned();
        let bytes_total = info.total_length();

        let mut inner = self.inner.lock();
        if inner.open_streams.contains_key(&id) {
            log::error!("Stream '{}' already open", id);
            return;
        }

        let (reader, chunk_tx) = AnyStreamReader::from(info);
        inner.dispatch_reader(reader, ParticipantIdentity(identity));
        // TODO: log unhandled stream, once per topic

        let descriptor =
            Descriptor { progress: StreamProgress { bytes_total, ..Default::default() }, chunk_tx };
        inner.open_streams.insert(id, descriptor);
    }

    /// Handles an incoming chunk packet.
    pub fn handle_chunk(&self, chunk: proto::Chunk) {
        let id = chunk.stream_id;
        let mut inner = self.inner.lock();
        let Some(descriptor) = inner.open_streams.get_mut(&id) else {
            return;
        };

        if descriptor.progress.chunk_index != chunk.chunk_index {
            inner.close_stream_with_error(&id, StreamError::MissedChunk);
            return;
        }

        descriptor.progress.chunk_index += 1;
        descriptor.progress.bytes_processed += chunk.content.len() as u64;

        if match descriptor.progress.bytes_total {
            Some(total) => descriptor.progress.bytes_processed > total as u64,
            None => false,
        } {
            inner.close_stream_with_error(&id, StreamError::LengthExceeded);
            return;
        }

        let chunk =
            IncomingChunk { progress: descriptor.progress, content: Bytes::from(chunk.content) };
        inner.yield_chunk(&id, chunk);
    }

    /// Handles an incoming trailer packet.
    pub fn handle_trailer(&self, trailer: proto::Trailer) {
        let id = trailer.stream_id;
        let mut inner = self.inner.lock();
        let Some(descriptor) = inner.open_streams.get_mut(&id) else {
            return;
        };

        if !match descriptor.progress.bytes_total {
            Some(total) => descriptor.progress.bytes_processed >= total as u64,
            None => true,
        } {
            inner.close_stream_with_error(&id, StreamError::Incomplete);
            return;
        }
        if !trailer.reason.is_empty() {
            inner.close_stream_with_error(&id, StreamError::AbnormalEnd(trailer.reason));
            return;
        }
        inner.close_stream(&id);
    }

    pub fn preregister_text_topics(&self, topics: &[String]) {
        let mut inner = self.inner.lock();
        topics.into_iter().for_each(|topic| _ = inner.handlers.text.preregister(&topic));
    }

    pub fn preregister_byte_topics(&self, topics: &[String]) {
        let mut inner = self.inner.lock();
        topics.into_iter().for_each(|topic| _ = inner.handlers.byte.preregister(&topic));
    }

    pub fn register_text_handler(
        &self,
        topic: &str,
        handler: impl Fn(
                TextStreamReader,
                ParticipantIdentity,
            )
                -> Pin<Box<dyn Future<Output = Result<(), Box<dyn Error + Send + Sync>>> + Send>>
            + Send
            + Sync
            + 'static,
    ) -> StreamResult<()> {
        // TODO: apply feature fn_traits (29625) once stabilized.
        let mut inner = self.inner.lock();
        if !inner.handlers.text.register(topic, move |args| handler(args.0, args.1)) {
            Err(StreamError::HandlerAlreadyRegistered)?
        }
        Ok(())
    }

    pub fn register_byte_handler(
        &self,
        topic: &str,
        handler: impl Fn(
                ByteStreamReader,
                ParticipantIdentity,
            )
                -> Pin<Box<dyn Future<Output = Result<(), Box<dyn Error + Send + Sync>>> + Send>>
            + Send
            + Sync
            + 'static,
    ) -> StreamResult<()> {
        let mut inner = self.inner.lock();
        if !inner.handlers.byte.register(topic, move |args| handler(args.0, args.1)) {
            Err(StreamError::HandlerAlreadyRegistered)?
        }
        Ok(())
    }

    pub fn unregister_text_handler(&self, topic: &str) {
        self.inner.lock().handlers.text.unregister(topic);
    }

    pub fn unregister_byte_handler(&self, topic: &str) {
        self.inner.lock().handlers.byte.unregister(topic);
    }
}

impl ManagerInner {
    fn yield_chunk(&mut self, id: &str, chunk: IncomingChunk) {
        let Some(descriptor) = self.open_streams.get_mut(id) else {
            return;
        };
        if descriptor.chunk_tx.send(Ok(chunk)).is_err() {
            // Reader has been dropped, close the stream.
            self.close_stream(id);
        }
    }

    fn close_stream(&mut self, id: &str) {
        // Dropping the sender closes the channel.
        self.open_streams.remove(id);
    }

    fn close_stream_with_error(&mut self, id: &str, error: StreamError) {
        if let Some(descriptor) = self.open_streams.remove(id) {
            let _ = descriptor.chunk_tx.send(Err(error));
        }
    }

    fn dispatch_reader(&mut self, reader: AnyStreamReader, identity: ParticipantIdentity) -> bool {
        match reader {
            AnyStreamReader::Byte(reader) => {
                self.handlers.byte.dispatch(&reader.info().topic.to_string(), (reader, identity))
            }
            AnyStreamReader::Text(reader) => {
                self.handlers.text.dispatch(&reader.info().topic.to_string(), (reader, identity))
            }
        }
    }
}

type StreamHandlerArgs<R> = (R, ParticipantIdentity);
type StreamHandlerResult = Result<(), Box<dyn Error + Send + Sync>>;

#[derive(Default)]
pub struct Handlers {
    byte: AsyncHandlerRegistry<(ByteStreamReader, ParticipantIdentity), StreamHandlerResult>,
    text: AsyncHandlerRegistry<(TextStreamReader, ParticipantIdentity), StreamHandlerResult>,
}
