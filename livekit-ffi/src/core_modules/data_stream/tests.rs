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

use std::sync::{Arc, Mutex};

use bytes::Bytes;
use livekit_protocol as proto;
use prost::Message as _;
use tokio::sync::oneshot;

use super::common::{
    ClientCapability, DataStreamError, EncryptionType, PacketDeliveryError, StreamTextOptions,
};
use super::incoming::{
    ByteStreamReader, IncomingDataStreamManager, IncomingDataStreamManagerDelegate,
    TextStreamReader,
};
use super::outgoing::{
    OutgoingDataStreamManager, OutgoingDataStreamManagerDelegate, RemoteParticipantRegistryDelegate,
};

/// Builds an encoded v2 inline (single-packet) text `DataPacket`.
fn inline_text_packet(identity: &str, topic: &str, text: &str) -> Bytes {
    let header = proto::data_stream::Header {
        stream_id: "s1".to_string(),
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

/// Builds an encoded v1 multi-packet text stream header `DataPacket` (no inline content).
fn multipacket_text_header_packet(identity: &str, topic: &str, total_length: u64) -> Bytes {
    let header = proto::data_stream::Header {
        stream_id: "s1".to_string(),
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

/// Builds an encoded chunk `DataPacket` for stream `s1`.
fn chunk_packet(identity: &str, chunk_index: u64, content: &[u8]) -> Bytes {
    let chunk = proto::data_stream::Chunk {
        stream_id: "s1".to_string(),
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

/// Builds an encoded trailer `DataPacket` for stream `s1`.
fn trailer_packet(identity: &str) -> Bytes {
    let trailer = proto::data_stream::Trailer { stream_id: "s1".to_string(), ..Default::default() };
    let packet = proto::DataPacket {
        participant_identity: identity.to_string(),
        value: Some(proto::data_packet::Value::StreamTrailer(trailer)),
        ..Default::default()
    };
    Bytes::from(packet.encode_to_vec())
}

/// Captures the first opened text reader.
struct TextCapture(Mutex<Option<oneshot::Sender<(Arc<TextStreamReader>, String)>>>);

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
            inline_text_packet("alice", "my-topic", "hello world"),
            EncryptionType::None,
        );

        let (reader, identity) = rx.await.expect("a stream should open");
        assert_eq!(identity, "alice");
        assert_eq!(reader.info().topic, "my-topic");
        assert_eq!(reader.read_all().await.unwrap(), "hello world");
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
            multipacket_text_header_packet("alice", "my-topic", 5),
            EncryptionType::None,
        );
        let (reader, _) = rx.await.expect("a stream should open");
        manager.handle_packet_received(chunk_packet("alice", 0, b"hello"), EncryptionType::Gcm);

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
            multipacket_text_header_packet("alice", "my-topic", 5),
            EncryptionType::Gcm,
        );
        let (reader, _) = rx.await.expect("a stream should open");
        manager.handle_packet_received(chunk_packet("alice", 0, b"hello"), EncryptionType::Gcm);
        manager.handle_packet_received(trailer_packet("alice"), EncryptionType::None);

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
            multipacket_text_header_packet("alice", "my-topic", 5),
            EncryptionType::None,
        );
        assert_eq!(manager.open_stream_count().await, 1);

        manager.abort_all_streams();
        assert_eq!(manager.open_stream_count().await, 0);
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
            multipacket_text_header_packet("alice", "my-topic", 5),
            EncryptionType::None,
        );
        manager.handle_packet_received(chunk_packet("alice", 0, b"hello"), EncryptionType::None);
        manager.handle_packet_received(trailer_packet("alice"), EncryptionType::None);

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
            inline_text_packet("alice", "my-topic", "hello world"),
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
            multipacket_text_header_packet("alice", "my-topic", 5),
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
