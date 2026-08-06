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

use super::common::{ClientCapability, StreamTextOptions};
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

/// Captures the first opened text reader.
struct TextCapture(Mutex<Option<oneshot::Sender<(Arc<TextStreamReader>, String)>>>);

impl IncomingDataStreamManagerDelegate for TextCapture {
    fn on_byte_stream_opened(&self, _reader: Arc<ByteStreamReader>, _identity: String) {}

    fn on_text_stream_opened(&self, reader: Arc<TextStreamReader>, identity: String) {
        if let Some(tx) = self.0.lock().unwrap().take() {
            let _ = tx.send((reader, identity));
        }
    }
}

#[test]
fn incoming_inline_text_stream_roundtrips() {
    crate::runtime::runtime().block_on(async {
        let (tx, rx) = oneshot::channel();
        let delegate = Arc::new(TextCapture(Mutex::new(Some(tx))));
        let manager = IncomingDataStreamManager::new(delegate, None);

        manager.handle_packet_received(inline_text_packet("alice", "my-topic", "hello world"));

        let (reader, identity) = rx.await.expect("a stream should open");
        assert_eq!(identity, "alice");
        assert_eq!(reader.info().topic, "my-topic");
        assert_eq!(reader.read_all().await.unwrap(), "hello world");
    });
}

/// Collects every outbound packet the manager emits.
struct PacketCapture(Mutex<Vec<Bytes>>);

impl OutgoingDataStreamManagerDelegate for PacketCapture {
    fn on_packets_available(&self, packets: Vec<Bytes>) {
        self.0.lock().unwrap().extend(packets);
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
