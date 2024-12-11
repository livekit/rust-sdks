use std::{
    collections::BTreeMap,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

use itertools::Itertools;
use livekit_runtime::Stream;
use parking_lot::Mutex;
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub struct DataStreamChunk {
    pub stream_id: String,
    pub chunk_index: u64,
    pub content: Vec<u8>,
    pub complete: bool,
    pub version: i32,
}

#[derive(Debug, Clone)]
pub struct FileStreamInfo {
    pub stream_id: String,
    pub timestamp: i64,
    pub topic: String,
    pub mime_type: String,
    pub total_length: Option<u64>,
    pub total_chunks: Option<u64>,
    pub file_name: String,
}
#[derive(Debug, Clone)]
pub struct FileStreamReader {
    update_rx: Arc<Mutex<mpsc::UnboundedReceiver<DataStreamChunk>>>,
    pub info: FileStreamInfo,
    is_closed: bool,
}

impl FileStreamReader {
    pub fn new(info: FileStreamInfo) -> (Self, FileStreamUpdater) {
        let (update_tx, update_rx) = mpsc::unbounded_channel();
        (
            Self { update_rx: Arc::new(Mutex::new(update_rx)), info, is_closed: false },
            FileStreamUpdater { update_tx },
        )
    }

    fn close(&mut self) {
        self.is_closed = true;
        self.update_rx.lock().close();
    }
}

impl Drop for FileStreamReader {
    fn drop(&mut self) {
        self.close();
    }
}

impl Stream for FileStreamReader {
    type Item = Vec<u8>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.is_closed {
            return Poll::Ready(None); // Stream is closed‚, stop yielding updates
        }
        let update_option = {
            let mut guarded = self.update_rx.lock();
            guarded.poll_recv(cx)
        };

        match update_option {
            Poll::Ready(Some(update)) => {
                if update.complete {
                    self.close();
                    Poll::Ready(None) // Close stream after receiving a complete update
                } else {
                    Poll::Ready(Some(update.content)) // Continue with data updates
                }
            }
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

/// Helper to send updates to the `FileStream`.
pub struct FileStreamUpdater {
    update_tx: mpsc::UnboundedSender<DataStreamChunk>,
}

impl FileStreamUpdater {
    /// Sends an update to the `FileStream`.
    pub fn send_update(
        &self,
        data: DataStreamChunk,
    ) -> Result<(), mpsc::error::SendError<DataStreamChunk>> {
        self.update_tx.send(data)
    }
}

#[derive(Debug, Clone)]
pub struct TextStreamInfo {
    pub stream_id: String,
    pub timestamp: i64,
    pub topic: String,
    pub mime_type: String,
    pub total_length: Option<u64>,
    pub total_chunks: Option<u64>,
    pub attachments: Vec<String>,
    pub version: i32,
}

#[derive(Debug, Clone)]
pub struct TextStreamChunk {
    pub collected: String,
    pub current: String,
    pub index: u64,
}
#[derive(Debug, Clone)]
pub struct TextStreamReader {
    update_rx: Arc<Mutex<mpsc::UnboundedReceiver<DataStreamChunk>>>,
    pub info: TextStreamInfo,
    is_closed: bool,
    chunks: BTreeMap<u64, DataStreamChunk>,
}

impl TextStreamReader {
    pub fn new(info: TextStreamInfo) -> (Self, TextStreamUpdater) {
        let (update_tx, update_rx) = mpsc::unbounded_channel();
        (
            Self {
                update_rx: Arc::new(Mutex::new(update_rx)),
                info,
                is_closed: false,
                chunks: BTreeMap::new(),
            },
            TextStreamUpdater { update_tx },
        )
    }

    fn close(&mut self) {
        self.is_closed = true;
        self.chunks.clear();
        self.update_rx.lock().close();
    }
}

impl Drop for TextStreamReader {
    fn drop(&mut self) {
        self.close();
    }
}

impl Stream for TextStreamReader {
    type Item = TextStreamChunk;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.is_closed {
            self.close();
            return Poll::Ready(None); // Stream is closed‚, stop yielding updates
        }

        let update_option = {
            let mut guarded = self.update_rx.lock();
            guarded.poll_recv(cx)
        };

        match update_option {
            Poll::Ready(Some(update)) => {
                if update.complete {
                    self.close();
                    Poll::Ready(None)
                } else {
                    let update_clone = update.clone();
                    let chunk_index = update.chunk_index;
                    let content = update.content.clone();

                    // Check for existing chunk version
                    if let Some(existing_chunk) = self.chunks.get(&chunk_index) {
                        if existing_chunk.version > update.version {
                            return Poll::Pending;
                        }
                    }
                    // Insert new chunk after immutable access
                    self.chunks.insert(chunk_index, update_clone);

                    // Collect chunks
                    let collected = self
                        .chunks
                        .values()
                        .map(|chunk| String::from_utf8(chunk.content.clone()).unwrap())
                        .join("");

                    Poll::Ready(Some(TextStreamChunk {
                        index: chunk_index,
                        current: String::from_utf8(content).unwrap(),
                        collected,
                    }))
                }
            }
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

/// Helper to send updates to the `FileStream`.
pub struct TextStreamUpdater {
    update_tx: mpsc::UnboundedSender<DataStreamChunk>,
}

impl TextStreamUpdater {
    /// Sends an update to the `FileStream`.
    pub fn send_update(
        &self,
        data: DataStreamChunk,
    ) -> Result<(), mpsc::error::SendError<DataStreamChunk>> {
        self.update_tx.send(data)
    }
}
