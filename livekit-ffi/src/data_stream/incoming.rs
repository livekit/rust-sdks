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

use bytes::{Bytes, BytesMut};
use futures_util::stream::{BoxStream, StreamExt};
use livekit_data_stream::api as ds_api;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use super::common::{ByteStreamInfo, DataStreamError, StreamProgress, TextStreamInfo};
use ds_api::StreamReader as _;

/// Reader for an incoming byte data stream.
#[derive(uniffi::Object)]
pub struct ByteStreamReader {
    /// Shares its attribute map with the core, which merges trailer attributes into it.
    info: ds_api::ByteStreamInfo,
    /// The core reader, or `None` once the reader was closed.
    inner: Arc<Mutex<Option<ds_api::ByteStreamReader>>>,
    progress: Mutex<BoxStream<'static, ds_api::StreamProgress>>,
    closed: CancellationToken,
    /// Ends the stream in the incoming manager, even while a read holds `inner`.
    closer: ds_api::StreamCloser,
    // After the migration is complete, this field can be removed.
    #[cfg(feature = "room-apis")]
    handle_id: std::sync::OnceLock<crate::FfiHandleId>,
}

impl ByteStreamReader {
    pub(crate) fn new(reader: ds_api::ByteStreamReader) -> Self {
        Self {
            info: reader.info().clone(),
            progress: Mutex::new(reader.progress_stream().boxed()),
            closer: reader.closer(),
            inner: Arc::new(Mutex::new(Some(reader))),
            closed: CancellationToken::new(),
            #[cfg(feature = "room-apis")]
            handle_id: std::sync::OnceLock::new(),
        }
    }
}

impl Drop for ByteStreamReader {
    /// A host that releases the reader without `close()` still ends the stream in the manager at
    /// once. After the stream ended, the manager ignores this.
    fn drop(&mut self) {
        if !self.closed.is_cancelled() {
            self.closer.close();
        }
    }
}

#[uniffi::export(async_runtime = "tokio")]
impl ByteStreamReader {
    /// Information about the underlying stream.
    ///
    /// Attributes from the stream's trailer are merged in once it arrives, before the read that
    /// ends the stream returns.
    pub fn info(&self) -> ByteStreamInfo {
        self.info.clone().into()
    }

    /// Waits for the next progress update, or returns `None` once the stream has ended.
    ///
    /// Progress counts the bytes received from the sender, after decompression, not the bytes
    /// the app has read. See [`TextStreamReader::next_progress`].
    pub async fn next_progress(&self) -> Option<StreamProgress> {
        next_progress(&self.progress, &self.closed).await
    }

    /// Returns the next chunk, or `None` once the stream has closed.
    pub async fn next(&self) -> Result<Option<Bytes>, DataStreamError> {
        next_chunk(&mut *self.inner.lock().await, &self.closed).await
    }

    /// Reads every chunk, concatenating them into a single buffer returned once the stream closes.
    pub async fn read_all(&self) -> Result<Bytes, DataStreamError> {
        let mut reader = self.inner.lock().await;
        let mut buffer = BytesMut::new();
        while let Some(chunk) = next_chunk(&mut reader, &self.closed).await? {
            buffer.extend_from_slice(&chunk);
        }
        Ok(buffer.freeze())
    }

    /// Stops reading the stream. See [`TextStreamReader::close`].
    pub fn close(&self) {
        close_reader(&self.inner, &self.closed, &self.closer);
    }
}

/// Reader for an incoming text data stream.
#[derive(uniffi::Object)]
pub struct TextStreamReader {
    /// Shares its attribute map with the core, which merges trailer attributes into it.
    info: ds_api::TextStreamInfo,
    /// The core reader, or `None` once the reader was closed.
    inner: Arc<Mutex<Option<ds_api::TextStreamReader>>>,
    progress: Mutex<BoxStream<'static, ds_api::StreamProgress>>,
    closed: CancellationToken,
    /// Ends the stream in the incoming manager, even while a read holds `inner`.
    closer: ds_api::StreamCloser,
    // After the migration is complete, this field can be removed.
    #[cfg(feature = "room-apis")]
    handle_id: std::sync::OnceLock<crate::FfiHandleId>,
}

impl TextStreamReader {
    pub(crate) fn new(reader: ds_api::TextStreamReader) -> Self {
        Self {
            info: reader.info().clone(),
            progress: Mutex::new(reader.progress_stream().boxed()),
            closer: reader.closer(),
            inner: Arc::new(Mutex::new(Some(reader))),
            closed: CancellationToken::new(),
            #[cfg(feature = "room-apis")]
            handle_id: std::sync::OnceLock::new(),
        }
    }
}

impl Drop for TextStreamReader {
    /// A host that releases the reader without `close()` still ends the stream in the manager at
    /// once. After the stream ended, the manager ignores this.
    fn drop(&mut self) {
        if !self.closed.is_cancelled() {
            self.closer.close();
        }
    }
}

#[uniffi::export(async_runtime = "tokio")]
impl TextStreamReader {
    /// Information about the underlying stream.
    ///
    /// Attributes from the stream's trailer are merged in once it arrives, before the read that
    /// ends the stream returns.
    pub fn info(&self) -> TextStreamInfo {
        self.info.clone().into()
    }

    /// Waits for the next progress update, or returns `None` once the stream has ended.
    ///
    /// Progress counts the bytes received from the sender, after decompression, not the bytes
    /// the app has read: a chunk counts as soon as it arrives, before `next()` returns it. The
    /// first call returns the current progress at once. Updates that arrive between two calls are
    /// merged into the latest one.
    pub async fn next_progress(&self) -> Option<StreamProgress> {
        next_progress(&self.progress, &self.closed).await
    }

    /// Returns the next chunk, or `None` once the stream has closed.
    pub async fn next(&self) -> Result<Option<String>, DataStreamError> {
        next_chunk(&mut *self.inner.lock().await, &self.closed).await
    }

    /// Reads every chunk, concatenating them into a single string returned once the stream closes.
    pub async fn read_all(&self) -> Result<String, DataStreamError> {
        let mut reader = self.inner.lock().await;
        let mut result = String::new();
        while let Some(chunk) = next_chunk(&mut reader, &self.closed).await? {
            result.push_str(&chunk);
        }
        Ok(result)
    }

    /// Stops reading the stream and releases the data it buffered.
    ///
    /// A pending or later `next()` or `read_all()` fails with
    /// [`DataStreamError::AlreadyClosed`], and `next_progress()` returns `None`. The incoming
    /// manager drops the stream at once, even if the sender sends nothing more, and reports it
    /// closed through `on_stream_closed` exactly once. Other streams, including a later one that
    /// reuses this stream's id, are not affected. Returns at once, so it is safe to call from a
    /// cancellation handler.
    pub fn close(&self) {
        close_reader(&self.inner, &self.closed, &self.closer);
    }
}

/// Awaits `operation`, or returns `None` once `closed` is cancelled. The close wins a tie.
async fn unless_closed<T>(
    closed: &CancellationToken,
    operation: impl std::future::Future<Output = T>,
) -> Option<T> {
    tokio::select! {
        biased;
        _ = closed.cancelled() => None,
        output = operation => Some(output),
    }
}

/// Reads the next chunk, unless the reader is closed first.
async fn next_chunk<R: ds_api::StreamReader + Unpin>(
    reader: &mut Option<R>,
    closed: &CancellationToken,
) -> Result<Option<R::Output>, DataStreamError> {
    let Some(reader) = reader.as_mut() else {
        return Err(DataStreamError::AlreadyClosed);
    };
    let chunk = unless_closed(closed, reader.next()).await.ok_or(DataStreamError::AlreadyClosed)?;
    Ok(chunk.transpose()?)
}

/// Waits for the next progress update, unless the reader is closed first.
async fn next_progress(
    progress: &Mutex<BoxStream<'static, ds_api::StreamProgress>>,
    closed: &CancellationToken,
) -> Option<StreamProgress> {
    let mut progress = progress.lock().await;
    unless_closed(closed, progress.next()).await.flatten().map(Into::into)
}

/// Ends the stream in the manager, wakes pending reads and drops the core reader. Does nothing
/// once the reader is closed, so a repeated close spawns no second task.
fn close_reader<R: Send + 'static>(
    reader: &Arc<Mutex<Option<R>>>,
    closed: &CancellationToken,
    closer: &ds_api::StreamCloser,
) {
    if closed.is_cancelled() {
        return;
    }
    closer.close();
    closed.cancel();
    // A read in progress holds the lock until it sees the cancellation. Drop the reader after it
    // in a task, because `close` must not block its caller.
    let reader = reader.clone();
    crate::runtime::runtime().spawn(async move {
        reader.lock().await.take();
    });
}

#[cfg(all(test, feature = "core-modules"))]
mod tests {
    use futures_util::FutureExt as _;

    use super::*;
    use crate::data_stream_manager::tests::{manager_with_events, open_text_stream, TIMEOUT};

    #[test]
    fn close_drops_the_core_reader_while_a_read_is_pending() {
        crate::runtime::runtime().block_on(async {
            let (manager, mut events) = manager_with_events();
            let reader = open_text_stream(&manager, &mut events, "s1").await;

            // A read that waits for the rest of the stream holds the lock on the core reader.
            let mut read = std::pin::pin!(reader.read_all());
            assert!(read.as_mut().now_or_never().is_none());
            reader.close();
            let result = tokio::time::timeout(TIMEOUT, read).await.expect("the read should end");
            assert!(matches!(result, Err(DataStreamError::AlreadyClosed)));

            let dropped = tokio::time::timeout(TIMEOUT, async {
                while reader.inner.lock().await.is_some() {
                    tokio::task::yield_now().await;
                }
            });
            dropped.await.expect("close should drop the core reader");
        });
    }
}
