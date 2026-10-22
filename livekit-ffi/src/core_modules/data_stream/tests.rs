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

//! Round-trip tests driving the FFI wrappers through mock foreign delegates on the global runtime.

use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    task::Poll,
    time::Duration,
};

use bytes::Bytes;
use livekit_protocol as proto;
use prost::Message as _;
use tokio::sync::{mpsc, oneshot};

use super::common::{
    ClientCapability, DataStreamError, EncryptionType, PacketDeliveryError, StreamByteOptions,
    StreamProgress, StreamTextOptions,
};
use super::incoming::{
    ByteStreamReader, IncomingDataStreamManager, IncomingDataStreamManagerDelegate,
    TextStreamReader,
};
use super::outgoing::{
    OutgoingDataStreamManager, OutgoingDataStreamManagerDelegate,
    RemoteParticipantRegistryDelegate, StreamProgressDelegate,
};

/// Builds an encoded v2 inline (single-packet) text `DataPacket` for stream `stream_id`.
fn inline_text_packet(stream_id: &str, identity: &str, topic: &str, text: &str) -> Bytes {
    let header = proto::data_stream::Header {
        stream_id: stream_id.to_string(),
        topic: topic.to_string(),
        mime_type: "text/plain".to_string(),
        timestamp: 0,
        total_length: Some(text.len() as u64),
        inline_content: Some(text.as_bytes().to_vec()),
        content_header: Some(proto::data_stream::header::ContentHeader::TextHeader(
            proto::data_stream::TextHeader::default(),
        )),
        ..Default::default()
    };
    let packet = proto::DataPacket {
        participant_identity: identity.to_string(),
        value: Some(proto::data_packet::Value::StreamHeader(header)),
        ..Default::default()
    };
    Bytes::from(packet.encode_to_vec())
}

/// Builds an encoded v1 multi-packet text stream header `DataPacket` (no inline content) for
/// stream `stream_id`.
fn multipacket_text_header_packet(
    stream_id: &str,
    identity: &str,
    topic: &str,
    total_length: u64,
) -> Bytes {
    let header = proto::data_stream::Header {
        stream_id: stream_id.to_string(),
        topic: topic.to_string(),
        mime_type: "text/plain".to_string(),
        timestamp: 0,
        total_length: Some(total_length),
        content_header: Some(proto::data_stream::header::ContentHeader::TextHeader(
            proto::data_stream::TextHeader::default(),
        )),
        ..Default::default()
    };
    let packet = proto::DataPacket {
        participant_identity: identity.to_string(),
        value: Some(proto::data_packet::Value::StreamHeader(header)),
        ..Default::default()
    };
    Bytes::from(packet.encode_to_vec())
}

/// Builds an encoded chunk `DataPacket` for stream `stream_id`.
fn chunk_packet(stream_id: &str, identity: &str, chunk_index: u64, content: &[u8]) -> Bytes {
    let chunk = proto::data_stream::Chunk {
        stream_id: stream_id.to_string(),
        chunk_index,
        content: content.to_vec(),
        ..Default::default()
    };
    let packet = proto::DataPacket {
        participant_identity: identity.to_string(),
        value: Some(proto::data_packet::Value::StreamChunk(chunk)),
        ..Default::default()
    };
    Bytes::from(packet.encode_to_vec())
}

/// Builds an encoded trailer `DataPacket` for stream `stream_id`.
fn trailer_packet(stream_id: &str, identity: &str, attributes: &[(&str, &str)]) -> Bytes {
    let trailer = proto::data_stream::Trailer {
        stream_id: stream_id.to_string(),
        attributes: attributes.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
        ..Default::default()
    };
    let packet = proto::DataPacket {
        participant_identity: identity.to_string(),
        value: Some(proto::data_packet::Value::StreamTrailer(trailer)),
        ..Default::default()
    };
    Bytes::from(packet.encode_to_vec())
}

/// An opened text reader and the identity of its sender.
type OpenedText = (Arc<TextStreamReader>, String);

/// Captures the first opened text reader.
struct TextCapture(Mutex<Option<oneshot::Sender<OpenedText>>>);

impl IncomingDataStreamManagerDelegate for TextCapture {
    fn on_byte_stream_opened(&self, _reader: Arc<ByteStreamReader>, _identity: String) {}

    fn on_text_stream_opened(&self, reader: Arc<TextStreamReader>, identity: String) {
        if let Some(tx) = self.0.lock().unwrap().take() {
            let _ = tx.send((reader, identity));
        }
    }

    fn on_stream_closed(&self, _stream_id: String, _identity: String) {}
}

#[test]
fn incoming_inline_text_stream_roundtrips() {
    crate::runtime::runtime().block_on(async {
        let (tx, rx) = oneshot::channel();
        let delegate = Arc::new(TextCapture(Mutex::new(Some(tx))));
        let manager = IncomingDataStreamManager::new(delegate, None);

        manager.handle_packet_received(
            inline_text_packet("s1", "alice", "my-topic", "hello world"),
            EncryptionType::None,
        );

        let (reader, identity) = rx.await.expect("a stream should open");
        assert_eq!(identity, "alice");
        assert_eq!(reader.info().topic, "my-topic");
        assert_eq!(reader.read_all().await.unwrap(), "hello world");
    });
}

#[test]
fn incoming_inline_stream_shorter_than_declared_errors_reader() {
    crate::runtime::runtime().block_on(async {
        let (tx, rx) = oneshot::channel();
        let delegate = Arc::new(TextCapture(Mutex::new(Some(tx))));
        let manager = IncomingDataStreamManager::new(delegate, None);

        // The header declares five bytes but carries three.
        let packet = inline_text_packet("s1", "alice", "my-topic", "hey");
        let mut packet = proto::DataPacket::decode(packet.as_ref()).unwrap();
        let Some(proto::data_packet::Value::StreamHeader(header)) = packet.value.as_mut() else {
            panic!("expected a stream header packet");
        };
        header.total_length = Some(5);
        manager.handle_packet_received(packet.encode_to_vec().into(), EncryptionType::None);

        let (reader, _) = rx.await.expect("a stream should open");
        assert!(matches!(reader.read_all().await, Err(DataStreamError::Incomplete)));
    });
}

#[test]
fn incoming_chunk_with_mismatched_encryption_errors_reader() {
    crate::runtime::runtime().block_on(async {
        let (tx, rx) = oneshot::channel();
        let delegate = Arc::new(TextCapture(Mutex::new(Some(tx))));
        let manager = IncomingDataStreamManager::new(delegate, None);

        // The stream is announced unencrypted, but a chunk arrives claiming GCM encryption.
        manager.handle_packet_received(
            multipacket_text_header_packet("s1", "alice", "my-topic", 5),
            EncryptionType::None,
        );
        let (reader, _) = rx.await.expect("a stream should open");
        manager
            .handle_packet_received(chunk_packet("s1", "alice", 0, b"hello"), EncryptionType::Gcm);

        let result = reader.read_all().await;
        assert!(matches!(
            result,
            Err(DataStreamError::EncryptionTypeMismatch {
                expected: EncryptionType::None,
                received: EncryptionType::Gcm,
            })
        ));
    });
}

#[test]
fn incoming_trailer_with_mismatched_encryption_errors_reader() {
    crate::runtime::runtime().block_on(async {
        let (tx, rx) = oneshot::channel();
        let delegate = Arc::new(TextCapture(Mutex::new(Some(tx))));
        let manager = IncomingDataStreamManager::new(delegate, None);

        // The stream is announced under GCM, but its trailer arrives in plaintext.
        manager.handle_packet_received(
            multipacket_text_header_packet("s1", "alice", "my-topic", 5),
            EncryptionType::Gcm,
        );
        let (reader, _) = rx.await.expect("a stream should open");
        manager
            .handle_packet_received(chunk_packet("s1", "alice", 0, b"hello"), EncryptionType::Gcm);
        manager.handle_packet_received(trailer_packet("s1", "alice", &[]), EncryptionType::None);

        let result = reader.read_all().await;
        assert!(matches!(
            result,
            Err(DataStreamError::EncryptionTypeMismatch {
                expected: EncryptionType::Gcm,
                received: EncryptionType::None,
            })
        ));
    });
}

#[test]
fn incoming_open_stream_count_tracks_headers_and_aborts() {
    crate::runtime::runtime().block_on(async {
        let (tx, _rx) = oneshot::channel();
        let delegate = Arc::new(TextCapture(Mutex::new(Some(tx))));
        let manager = IncomingDataStreamManager::new(delegate, None);
        assert_eq!(manager.open_stream_count().await, 0);

        // The count query is processed in order with the packets enqueued before it, so this
        // waits for the (orphaned) header to register without racing the run loop — exactly what
        // exercising the abort paths requires.
        manager.handle_packet_received(
            multipacket_text_header_packet("s1", "alice", "my-topic", 5),
            EncryptionType::None,
        );
        assert_eq!(manager.open_stream_count().await, 1);

        manager.abort_all_streams();
        assert_eq!(manager.open_stream_count().await, 0);
    });
}

#[test]
fn incoming_reader_info_shows_trailer_attributes() {
    crate::runtime::runtime().block_on(async {
        let (tx, rx) = oneshot::channel();
        let delegate = Arc::new(TextCapture(Mutex::new(Some(tx))));
        let manager = IncomingDataStreamManager::new(delegate, None);

        manager.handle_packet_received(
            multipacket_text_header_packet("s1", "alice", "my-topic", 5),
            EncryptionType::None,
        );
        let (reader, _) = rx.await.expect("a stream should open");
        manager
            .handle_packet_received(chunk_packet("s1", "alice", 0, b"hello"), EncryptionType::None);
        manager.handle_packet_received(
            trailer_packet("s1", "alice", &[("summary", "done")]),
            EncryptionType::None,
        );

        assert_eq!(reader.read_all().await.unwrap(), "hello");
        assert_eq!(reader.info().attributes.get("summary").map(String::as_str), Some("done"));
    });
}

#[test]
fn incoming_reader_reports_progress() {
    crate::runtime::runtime().block_on(async {
        let (tx, rx) = oneshot::channel();
        let delegate = Arc::new(TextCapture(Mutex::new(Some(tx))));
        let manager = IncomingDataStreamManager::new(delegate, None);

        manager.handle_packet_received(
            multipacket_text_header_packet("s1", "alice", "my-topic", 10),
            EncryptionType::None,
        );
        let (reader, _) = rx.await.expect("a stream should open");
        let progress =
            |bytes_processed| Some(StreamProgress { bytes_processed, bytes_total: Some(10) });
        assert_eq!(reader.next_progress().await, progress(0));

        // Progress counts bytes as they arrive, before the reader reads them.
        manager
            .handle_packet_received(chunk_packet("s1", "alice", 0, b"hello"), EncryptionType::None);
        assert_eq!(reader.next_progress().await, progress(5));
        manager
            .handle_packet_received(chunk_packet("s1", "alice", 1, b"world"), EncryptionType::None);
        assert_eq!(reader.next_progress().await, progress(10));
        manager.handle_packet_received(trailer_packet("s1", "alice", &[]), EncryptionType::None);
        assert_eq!(reader.next_progress().await, None);

        assert_eq!(reader.read_all().await.unwrap(), "helloworld");
    });
}

/// How long a test waits for something that must happen.
const TIMEOUT: Duration = Duration::from_secs(5);

/// Topic of the inline marker streams that [`count_closes`] sends.
const MARKER_TOPIC: &str = "marker";

/// Polls `future` once and returns the result of that poll.
async fn poll_once<F: Future>(mut future: Pin<&mut F>) -> Poll<F::Output> {
    std::future::poll_fn(|cx| Poll::Ready(future.as_mut().poll(cx))).await
}

/// Rewrites the header in an encoded header `DataPacket`.
fn edit_header(packet: Bytes, edit: impl FnOnce(&mut proto::data_stream::Header)) -> Bytes {
    let mut packet = proto::DataPacket::decode(packet.as_ref()).unwrap();
    let Some(proto::data_packet::Value::StreamHeader(header)) = packet.value.as_mut() else {
        panic!("expected a stream header packet");
    };
    edit(header);
    Bytes::from(packet.encode_to_vec())
}

/// Builds an encoded multi-packet byte stream header `DataPacket` for stream `stream_id`.
fn multipacket_byte_header_packet(stream_id: &str, identity: &str, total_length: u64) -> Bytes {
    let header = multipacket_text_header_packet(stream_id, identity, "my-topic", total_length);
    edit_header(header, |header| {
        header.mime_type = "application/octet-stream".to_string();
        header.content_header = Some(proto::data_stream::header::ContentHeader::ByteHeader(
            proto::data_stream::ByteHeader { name: "file".to_string() },
        ));
    })
}

/// Forwards every opened reader, except marker streams, and the id of every closed stream.
struct StreamEvents {
    text: mpsc::UnboundedSender<Arc<TextStreamReader>>,
    bytes: mpsc::UnboundedSender<Arc<ByteStreamReader>>,
    closed: mpsc::UnboundedSender<String>,
}

impl IncomingDataStreamManagerDelegate for StreamEvents {
    fn on_byte_stream_opened(&self, reader: Arc<ByteStreamReader>, _identity: String) {
        let _ = self.bytes.send(reader);
    }

    fn on_text_stream_opened(&self, reader: Arc<TextStreamReader>, _identity: String) {
        if reader.info().topic != MARKER_TOPIC {
            let _ = self.text.send(reader);
        }
    }

    fn on_stream_closed(&self, stream_id: String, _identity: String) {
        let _ = self.closed.send(stream_id);
    }
}

/// The receiving ends of [`StreamEvents`].
struct Events {
    text: mpsc::UnboundedReceiver<Arc<TextStreamReader>>,
    bytes: mpsc::UnboundedReceiver<Arc<ByteStreamReader>>,
    closed: mpsc::UnboundedReceiver<String>,
}

/// Creates a manager whose delegate forwards its events to the returned [`Events`].
fn manager_with_events() -> (Arc<IncomingDataStreamManager>, Events) {
    let (text_tx, text) = mpsc::unbounded_channel();
    let (bytes_tx, bytes) = mpsc::unbounded_channel();
    let (closed_tx, closed) = mpsc::unbounded_channel();
    let delegate = Arc::new(StreamEvents { text: text_tx, bytes: bytes_tx, closed: closed_tx });
    (IncomingDataStreamManager::new(delegate, None), Events { text, bytes, closed })
}

/// Counts the close events for `stream_id` that `manager` emitted so far.
///
/// An inline marker stream closes at once, and close events reach the delegate in order, so every
/// close event emitted before the marker arrives before the marker's own.
async fn count_closes(
    manager: &IncomingDataStreamManager,
    closed: &mut mpsc::UnboundedReceiver<String>,
    stream_id: &str,
) -> usize {
    let marker = inline_text_packet("marker", "alice", MARKER_TOPIC, "x");
    manager.handle_packet_received(marker, EncryptionType::None);
    let mut count = 0;
    loop {
        let id = tokio::time::timeout(TIMEOUT, closed.recv()).await.unwrap().unwrap();
        match id.as_str() {
            "marker" => return count,
            id if id == stream_id => count += 1,
            _ => {}
        }
    }
}

/// Opens a text stream that declares 10 bytes and delivers its first 5, and returns its reader.
async fn open_text_stream(
    manager: &IncomingDataStreamManager,
    events: &mut Events,
    stream_id: &str,
) -> Arc<TextStreamReader> {
    let header = multipacket_text_header_packet(stream_id, "alice", "my-topic", 10);
    manager.handle_packet_received(header, EncryptionType::None);
    let chunk = chunk_packet(stream_id, "alice", 0, b"hello");
    manager.handle_packet_received(chunk, EncryptionType::None);
    let reader = events.text.recv().await.expect("the stream should open");
    assert_eq!(reader.info().id, stream_id);
    reader
}

#[test]
fn incoming_reader_close_ends_its_stream_without_more_packets() {
    crate::runtime::runtime().block_on(async {
        let (manager, mut events) = manager_with_events();
        let reader = open_text_stream(&manager, &mut events, "s1").await;
        let other = open_text_stream(&manager, &mut events, "s2").await;

        // A read that waits for the rest of the stream ends when the reader is closed.
        let mut read = std::pin::pin!(reader.read_all());
        assert!(poll_once(read.as_mut()).await.is_pending());
        reader.close();
        let result = tokio::time::timeout(TIMEOUT, read).await.expect("the read should end");
        assert!(matches!(result, Err(DataStreamError::AlreadyClosed)));
        assert!(matches!(reader.next().await, Err(DataStreamError::AlreadyClosed)));
        assert_eq!(reader.next_progress().await, None);

        // The sender sends nothing more, and the manager still drops the stream and reports it
        // closed exactly once.
        assert_eq!(manager.open_stream_count().await, 1);
        assert_eq!(count_closes(&manager, &mut events.closed, "s1").await, 1);

        // The other stream on the same manager is not affected.
        manager
            .handle_packet_received(chunk_packet("s2", "alice", 1, b"world"), EncryptionType::None);
        manager.handle_packet_received(trailer_packet("s2", "alice", &[]), EncryptionType::None);
        assert_eq!(other.read_all().await.unwrap(), "helloworld");
        assert_eq!(count_closes(&manager, &mut events.closed, "s2").await, 1);
    });
}

#[test]
fn incoming_byte_reader_close_ends_its_stream() {
    crate::runtime::runtime().block_on(async {
        let (manager, mut events) = manager_with_events();
        manager.handle_packet_received(
            multipacket_byte_header_packet("b1", "alice", 10),
            EncryptionType::None,
        );
        manager
            .handle_packet_received(chunk_packet("b1", "alice", 0, b"hello"), EncryptionType::None);
        let reader = events.bytes.recv().await.expect("the stream should open");

        let mut read = std::pin::pin!(reader.read_all());
        assert!(poll_once(read.as_mut()).await.is_pending());
        reader.close();
        let result = tokio::time::timeout(TIMEOUT, read).await.expect("the read should end");
        assert!(matches!(result, Err(DataStreamError::AlreadyClosed)));
        assert!(matches!(reader.next().await, Err(DataStreamError::AlreadyClosed)));

        assert_eq!(manager.open_stream_count().await, 0);
        assert_eq!(count_closes(&manager, &mut events.closed, "b1").await, 1);
    });
}

#[test]
fn incoming_reader_closed_twice_reports_one_close() {
    crate::runtime::runtime().block_on(async {
        let (manager, mut events) = manager_with_events();
        let reader = open_text_stream(&manager, &mut events, "s1").await;

        reader.close();
        reader.close();
        assert!(matches!(reader.next().await, Err(DataStreamError::AlreadyClosed)));
        drop(reader);

        assert_eq!(manager.open_stream_count().await, 0);
        assert_eq!(count_closes(&manager, &mut events.closed, "s1").await, 1);
    });
}

#[test]
fn incoming_reader_close_racing_the_end_reports_one_close() {
    crate::runtime::runtime().block_on(async {
        let (manager, mut events) = manager_with_events();
        // `close()` and `handle_packet_received` enqueue on the same queue at once, so each order
        // below reaches the manager as written.
        for (id, order) in [
            ("r0", ["close", "chunk", "trailer"]),
            ("r1", ["chunk", "close", "trailer"]),
            ("r2", ["chunk", "trailer", "close"]),
        ] {
            let reader = open_text_stream(&manager, &mut events, id).await;
            for step in order {
                match step {
                    "close" => reader.close(),
                    "chunk" => manager.handle_packet_received(
                        chunk_packet(id, "alice", 1, b"world"),
                        EncryptionType::None,
                    ),
                    _ => manager.handle_packet_received(
                        trailer_packet(id, "alice", &[]),
                        EncryptionType::None,
                    ),
                }
            }
            assert_eq!(count_closes(&manager, &mut events.closed, id).await, 1, "{order:?}");
        }
        assert_eq!(manager.open_stream_count().await, 0);
    });
}

#[test]
fn incoming_reader_close_spares_a_later_stream_with_the_same_id() {
    crate::runtime::runtime().block_on(async {
        let (manager, mut events) = manager_with_events();
        let first = open_text_stream(&manager, &mut events, "s1").await;
        manager
            .handle_packet_received(chunk_packet("s1", "alice", 1, b"world"), EncryptionType::None);
        manager.handle_packet_received(trailer_packet("s1", "alice", &[]), EncryptionType::None);
        assert_eq!(first.read_all().await.unwrap(), "helloworld");
        assert_eq!(count_closes(&manager, &mut events.closed, "s1").await, 1);

        // A new stream reuses the id, and then the first reader is closed late.
        let second = open_text_stream(&manager, &mut events, "s1").await;
        first.close();
        assert_eq!(manager.open_stream_count().await, 1);
        assert_eq!(count_closes(&manager, &mut events.closed, "s1").await, 0);

        manager
            .handle_packet_received(chunk_packet("s1", "alice", 1, b"world"), EncryptionType::None);
        manager.handle_packet_received(trailer_packet("s1", "alice", &[]), EncryptionType::None);
        assert_eq!(second.read_all().await.unwrap(), "helloworld");
        assert_eq!(count_closes(&manager, &mut events.closed, "s1").await, 1);
    });
}

/// Captures the first stream-closed notification.
struct ClosedCapture(Mutex<Option<oneshot::Sender<(String, String)>>>);

impl IncomingDataStreamManagerDelegate for ClosedCapture {
    fn on_byte_stream_opened(&self, _reader: Arc<ByteStreamReader>, _identity: String) {}

    fn on_text_stream_opened(&self, _reader: Arc<TextStreamReader>, _identity: String) {}

    fn on_stream_closed(&self, stream_id: String, identity: String) {
        if let Some(tx) = self.0.lock().unwrap().take() {
            let _ = tx.send((stream_id, identity));
        }
    }
}

#[test]
fn incoming_trailer_fires_stream_closed() {
    crate::runtime::runtime().block_on(async {
        let (tx, rx) = oneshot::channel();
        let delegate = Arc::new(ClosedCapture(Mutex::new(Some(tx))));
        let manager = IncomingDataStreamManager::new(delegate, None);

        manager.handle_packet_received(
            multipacket_text_header_packet("s1", "alice", "my-topic", 5),
            EncryptionType::None,
        );
        manager
            .handle_packet_received(chunk_packet("s1", "alice", 0, b"hello"), EncryptionType::None);
        manager.handle_packet_received(trailer_packet("s1", "alice", &[]), EncryptionType::None);

        let (stream_id, identity) = rx.await.expect("the stream should close");
        assert_eq!(stream_id, "s1");
        assert_eq!(identity, "alice");
    });
}

#[test]
fn incoming_inline_stream_fires_stream_closed() {
    // Inline single-packet streams never receive a trailer, so the closed signal must still fire
    // once their payload is delivered.
    crate::runtime::runtime().block_on(async {
        let (tx, rx) = oneshot::channel();
        let delegate = Arc::new(ClosedCapture(Mutex::new(Some(tx))));
        let manager = IncomingDataStreamManager::new(delegate, None);

        manager.handle_packet_received(
            inline_text_packet("s1", "alice", "my-topic", "hello world"),
            EncryptionType::None,
        );

        let (stream_id, identity) = rx.await.expect("the stream should close");
        assert_eq!(stream_id, "s1");
        assert_eq!(identity, "alice");
    });
}

#[test]
fn incoming_abort_fires_stream_closed() {
    crate::runtime::runtime().block_on(async {
        let (tx, rx) = oneshot::channel();
        let delegate = Arc::new(ClosedCapture(Mutex::new(Some(tx))));
        let manager = IncomingDataStreamManager::new(delegate, None);

        // Announce a multi-packet stream, then abort before its trailer ever arrives.
        manager.handle_packet_received(
            multipacket_text_header_packet("s1", "alice", "my-topic", 5),
            EncryptionType::None,
        );
        manager.abort_all_streams();

        let (stream_id, identity) = rx.await.expect("the stream should close");
        assert_eq!(stream_id, "s1");
        assert_eq!(identity, "alice");
    });
}

/// Collects every outbound packet the manager emits.
struct PacketCapture(Mutex<Vec<Bytes>>);

#[async_trait::async_trait]
impl OutgoingDataStreamManagerDelegate for PacketCapture {
    async fn on_packets_available(&self, packets: Vec<Bytes>) -> Result<(), PacketDeliveryError> {
        self.0.lock().unwrap().extend(packets);
        Ok(())
    }
}

/// A room where every recipient is v2 and advertises deflate-raw compression.
struct AllV2Registry;

impl RemoteParticipantRegistryDelegate for AllV2Registry {
    fn remote_client_protocol(&self, _identity: String) -> i32 {
        livekit_common::CLIENT_PROTOCOL_DATA_STREAM_V2
    }

    fn remote_capabilities(&self, _identity: String) -> Vec<ClientCapability> {
        vec![ClientCapability::CompressionDeflateRaw]
    }

    fn remote_identities(&self) -> Vec<String> {
        vec!["bob".to_string()]
    }
}

#[test]
fn outgoing_all_v2_text_inlines_compressed() {
    crate::runtime::runtime().block_on(async {
        let delegate = Arc::new(PacketCapture(Mutex::new(Vec::new())));
        let manager = OutgoingDataStreamManager::new(delegate.clone(), Arc::new(AllV2Registry));

        let options = StreamTextOptions {
            topic: "chat".to_string(),
            destination_identities: vec!["bob".to_string()],
            ..Default::default()
        };
        let info = manager
            .send_text("hello hello compressible world".to_string(), options)
            .await
            .expect("send_text should succeed");
        assert_eq!(info.topic, "chat");

        // send_text awaits the transport responder, which the forward task fulfills only after
        // invoking the delegate — so the packet is already captured here.
        let packets = delegate.0.lock().unwrap();
        assert_eq!(packets.len(), 1, "expected a single inline header packet");

        let decoded = proto::DataPacket::decode(packets[0].as_ref()).unwrap();
        let Some(proto::data_packet::Value::StreamHeader(header)) = decoded.value else {
            panic!("expected a stream header packet");
        };
        assert_eq!(header.compression(), proto::data_stream::CompressionType::DeflateRaw);
        let inline = header.inline_content.expect("inline content should be present");
        assert_ne!(inline.as_slice(), b"hello hello compressible world", "should be compressed");
    });
}

/// Decodes the trailer packet that ends `packets`.
fn last_trailer(packets: &[Bytes]) -> proto::data_stream::Trailer {
    let last = packets.last().expect("a trailer should have been sent");
    let Some(proto::data_packet::Value::StreamTrailer(trailer)) =
        proto::DataPacket::decode(last.as_ref()).unwrap().value
    else {
        panic!("expected a stream trailer packet");
    };
    trailer
}

#[test]
fn outgoing_writers_send_trailer_attributes() {
    crate::runtime::runtime().block_on(async {
        let delegate = Arc::new(PacketCapture(Mutex::new(Vec::new())));
        let manager = OutgoingDataStreamManager::new(delegate.clone(), Arc::new(AllV2Registry));
        let attributes: std::collections::HashMap<String, String> =
            [("summary".to_string(), "done".to_string())].into();

        let options = StreamTextOptions { topic: "chat".to_string(), ..Default::default() };
        let writer = manager.stream_text(options).await.unwrap();
        writer.write("hello".to_string()).await.unwrap();
        writer.close_with_options(None, attributes.clone()).await.unwrap();
        let trailer = last_trailer(&delegate.0.lock().unwrap());
        assert_eq!(trailer.attributes, attributes);
        assert_eq!(trailer.reason, "");

        let options = StreamByteOptions { topic: "file".to_string(), ..Default::default() };
        let writer = manager.stream_bytes(options).await.unwrap();
        writer.close_with_options(Some("cancelled".to_string()), attributes.clone()).await.unwrap();
        let trailer = last_trailer(&delegate.0.lock().unwrap());
        assert_eq!(trailer.attributes, attributes);
        assert_eq!(trailer.reason, "cancelled");
    });
}

/// Sends `text` on a text stream that declares `total_length`, and returns the stream's reader on
/// the receiving side once every packet was delivered.
async fn stream_text_with_total_length(text: &str, total_length: u64) -> Arc<TextStreamReader> {
    let delegate = Arc::new(PacketCapture(Mutex::new(Vec::new())));
    let outgoing = OutgoingDataStreamManager::new(delegate.clone(), Arc::new(AllV2Registry));
    let options = StreamTextOptions {
        topic: "chat".to_string(),
        total_length: Some(total_length),
        ..Default::default()
    };
    let writer = outgoing.stream_text(options).await.unwrap();
    assert_eq!(writer.info().total_length, Some(total_length));
    writer.write(text.to_string()).await.unwrap();
    writer.close().await.unwrap();

    let (tx, rx) = oneshot::channel();
    let incoming =
        IncomingDataStreamManager::new(Arc::new(TextCapture(Mutex::new(Some(tx)))), None);
    for packet in delegate.0.lock().unwrap().drain(..) {
        incoming.handle_packet_received(packet, EncryptionType::None);
    }
    rx.await.expect("a stream should open").0
}

#[test]
fn outgoing_text_stream_declares_total_length() {
    crate::runtime::runtime().block_on(async {
        let reader = stream_text_with_total_length("hello", 5).await;
        assert_eq!(reader.info().total_length, Some(5));
        assert_eq!(reader.read_all().await.unwrap(), "hello");

        // The receiver holds the stream to its declared length.
        let reader = stream_text_with_total_length("hi", 5).await;
        assert!(matches!(reader.read_all().await, Err(DataStreamError::Incomplete)));
    });
}

/// A transport delegate that accepts a fixed number of calls, then fails every subsequent one.
struct FailingTransport(std::sync::atomic::AtomicUsize);

impl FailingTransport {
    fn failing_after(successful_calls: usize) -> Self {
        Self(std::sync::atomic::AtomicUsize::new(successful_calls))
    }
}

#[async_trait::async_trait]
impl OutgoingDataStreamManagerDelegate for FailingTransport {
    async fn on_packets_available(&self, _packets: Vec<Bytes>) -> Result<(), PacketDeliveryError> {
        let remaining = &self.0;
        if remaining
            .fetch_update(
                std::sync::atomic::Ordering::SeqCst,
                std::sync::atomic::Ordering::SeqCst,
                |n| n.checked_sub(1),
            )
            .is_ok()
        {
            Ok(())
        } else {
            Err(PacketDeliveryError::Failed { reason: "transport is down".to_string() })
        }
    }
}

/// Collects the batch boundaries of each delegate invocation.
struct BatchCapture(Mutex<Vec<usize>>);

#[async_trait::async_trait]
impl OutgoingDataStreamManagerDelegate for BatchCapture {
    async fn on_packets_available(&self, packets: Vec<Bytes>) -> Result<(), PacketDeliveryError> {
        self.0.lock().unwrap().push(packets.len());
        Ok(())
    }
}

#[test]
fn outgoing_send_failure_propagates() {
    crate::runtime::runtime().block_on(async {
        let delegate = Arc::new(FailingTransport::failing_after(0));
        let manager = OutgoingDataStreamManager::new(delegate, Arc::new(AllV2Registry));

        let options = StreamTextOptions { topic: "chat".to_string(), ..Default::default() };
        let result = manager.send_text("hello".to_string(), options).await;
        assert!(matches!(result, Err(DataStreamError::SendFailed)));
    });
}

#[test]
fn outgoing_write_failure_errors_and_closes_writer() {
    crate::runtime::runtime().block_on(async {
        // Allow the header through, then fail: the failure lands on the write.
        let delegate = Arc::new(FailingTransport::failing_after(1));
        let manager = OutgoingDataStreamManager::new(delegate, Arc::new(AllV2Registry));

        let options = StreamTextOptions { topic: "chat".to_string(), ..Default::default() };
        let writer = manager.stream_text(options).await.expect("opening the stream should work");
        assert!(writer.is_open().await);

        let result = writer.write("hello".to_string()).await;
        assert!(matches!(result, Err(DataStreamError::SendFailed)));
        assert!(!writer.is_open().await, "a failed send should close the stream");
    });
}

#[test]
fn outgoing_one_shot_send_is_a_single_delegate_call() {
    crate::runtime::runtime().block_on(async {
        let delegate = Arc::new(BatchCapture(Mutex::new(Vec::new())));
        let manager = OutgoingDataStreamManager::new(delegate.clone(), Arc::new(PreV2Registry));

        // 40 KB to a pre-v2 recipient: legacy framing, header + 3 chunks + trailer — the whole
        // stream must arrive as ONE delegate call, not one call per packet.
        let options = StreamTextOptions {
            topic: "chat".to_string(),
            destination_identities: vec!["bob".to_string()],
            ..Default::default()
        };
        manager.send_text("A".repeat(40_000), options).await.expect("send_text should succeed");
        assert_eq!(*delegate.0.lock().unwrap(), vec![5]);
    });
}

/// Records every progress report.
struct ProgressCapture(Mutex<Vec<StreamProgress>>);

impl ProgressCapture {
    fn take(&self) -> Vec<StreamProgress> {
        std::mem::take(&mut self.0.lock().unwrap())
    }
}

impl StreamProgressDelegate for ProgressCapture {
    fn on_progress(&self, progress: StreamProgress) {
        self.0.lock().unwrap().push(progress);
    }
}

#[test]
fn outgoing_sends_report_progress() {
    crate::runtime::runtime().block_on(async {
        let delegate = Arc::new(PacketCapture(Mutex::new(Vec::new())));
        let manager = OutgoingDataStreamManager::new(delegate, Arc::new(PreV2Registry));
        let progress = Arc::new(ProgressCapture(Mutex::new(Vec::new())));
        let complete = |bytes| StreamProgress { bytes_processed: bytes, bytes_total: Some(bytes) };

        // One-shot sends hand the whole stream to the transport at once: one report, on completion.
        let options = StreamTextOptions { topic: "chat".to_string(), ..Default::default() };
        manager
            .send_text_with_progress("hello".to_string(), options, progress.clone())
            .await
            .unwrap();
        assert_eq!(progress.take(), vec![complete(5)]);

        let options = StreamByteOptions { topic: "blob".to_string(), ..Default::default() };
        let data = Bytes::from_static(b"abc");
        manager.send_bytes_with_progress(data, options, progress.clone()).await.unwrap();
        assert_eq!(progress.take(), vec![complete(3)]);

        // A file reports after each chunk: 15 000 + 5 000 bytes, uncompressed for a pre-v2 room.
        let path = std::env::temp_dir().join(format!("lk-ffi-progress-{}.bin", std::process::id()));
        std::fs::write(&path, vec![7u8; 20_000]).unwrap();
        let options = StreamByteOptions { topic: "file".to_string(), ..Default::default() };
        let result = manager
            .send_file_with_progress(path.to_string_lossy().into_owned(), options, progress.clone())
            .await;
        let _ = std::fs::remove_file(&path);
        result.unwrap();
        let reported: Vec<_> = progress.take().iter().map(|p| p.bytes_processed).collect();
        assert_eq!(reported, vec![15_000, 20_000]);
    });
}

/// A room where every recipient predates v2.
struct PreV2Registry;

impl RemoteParticipantRegistryDelegate for PreV2Registry {
    fn remote_client_protocol(&self, _identity: String) -> i32 {
        livekit_common::CLIENT_PROTOCOL_DEFAULT
    }

    fn remote_capabilities(&self, _identity: String) -> Vec<ClientCapability> {
        vec![]
    }

    fn remote_identities(&self) -> Vec<String> {
        vec!["bob".to_string()]
    }
}

/// Drives both managers through the pull adapters in [`super::polled`] — the path thread-affine
/// bindings take — and checks a payload survives the round trip.
async fn polled_roundtrip(
    registry: Arc<dyn RemoteParticipantRegistryDelegate>,
    text: &str,
) -> (usize, String) {
    let outgoing = super::polled::polled_outgoing_data_stream_manager(registry);
    let incoming = super::polled::polled_incoming_data_stream_manager(None);

    let options = StreamTextOptions {
        topic: "chat".to_string(),
        destination_identities: vec!["bob".to_string()],
        ..Default::default()
    };
    outgoing.manager.send_text(text.to_string(), options).await.expect("send_text should succeed");

    // send_text only resolves once every packet has been queued, so draining terminates rather
    // than blocking.
    let mut packet_count = 0;
    while let Ok(Some(packets)) =
        tokio::time::timeout(std::time::Duration::from_millis(200), outgoing.packets.next_packets())
            .await
    {
        for packet in packets {
            packet_count += 1;
            incoming.manager.handle_packet_received(packet, EncryptionType::None);
        }
    }

    let opened = incoming.streams.next_opened_stream().await.expect("a stream should open");
    let reader = opened.text_reader.expect("expected a text stream");
    (packet_count, reader.read_all().await.unwrap())
}

#[test]
fn polled_inlines_for_v2_recipients() {
    crate::runtime::runtime().block_on(async {
        let (packets, text) =
            polled_roundtrip(Arc::new(AllV2Registry), "hello hello compressible world").await;
        assert_eq!(packets, 1, "a v2 recipient should get a single inline packet");
        assert_eq!(text, "hello hello compressible world");
    });
}

#[test]
fn polled_falls_back_to_legacy_framing_for_pre_v2_recipients() {
    crate::runtime::runtime().block_on(async {
        let (packets, text) = polled_roundtrip(Arc::new(PreV2Registry), "hello world").await;
        assert_eq!(packets, 3, "a pre-v2 recipient should get header + chunk + trailer");
        assert_eq!(text, "hello world");
    });
}
