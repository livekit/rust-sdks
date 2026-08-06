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

use bytes::Bytes;
use livekit_common as lk_common;
use livekit_data_stream::{api as ds_api, backend as ds};
use prost::Message as _;
use tokio_util::sync::{CancellationToken, DropGuard};

use super::common::{
    ByteStreamInfo, ClientCapability, DataStreamError, StreamByteOptions, StreamTextOptions,
    TextStreamInfo,
};
use ds_api::StreamWriter as _;

/// Sends data streams, choosing v2 single-packet/compression or legacy multi-packet framing based
/// on recipient capabilities. Outbound packets are handed to a foreign delegate for transport.
#[derive(uniffi::Object)]
pub struct OutgoingDataStreamManager {
    manager: ds::outgoing::Manager,
    registry: Arc<dyn lk_common::RemoteParticipantRegistry>,
    _guard: DropGuard,
}

/// Delegate for receiving outbound packets from [`OutgoingDataStreamManager`].
#[uniffi::export(with_foreign)]
pub trait OutgoingDataStreamManagerDelegate: Send + Sync {
    /// Encoded [`livekit_protocol::DataPacket`]s to be sent over the data channel transport.
    fn on_packets_available(&self, packets: Vec<Bytes>);
}

/// Read access to remote participants' advertised protocol and capabilities, implemented by the
/// foreign side. Mirrors [`lk_common::RemoteParticipantRegistry`]; used to decide inline/compression
/// eligibility per send.
#[uniffi::export(with_foreign)]
pub trait RemoteParticipantRegistryDelegate: Send + Sync {
    /// A remote participant's `client_protocol`, or `0` (`CLIENT_PROTOCOL_DEFAULT`) if unknown.
    fn remote_client_protocol(&self, identity: String) -> i32;

    /// A remote participant's advertised capabilities, or empty if unknown.
    fn remote_capabilities(&self, identity: String) -> Vec<ClientCapability>;

    /// The identities of every remote participant, used to resolve a broadcast send.
    fn remote_identities(&self) -> Vec<String>;
}

/// Adapts the foreign [`RemoteParticipantRegistryDelegate`] to the crate-internal
/// [`lk_common::RemoteParticipantRegistry`] the outgoing manager consumes.
struct ForeignRegistry(Arc<dyn RemoteParticipantRegistryDelegate>);

impl lk_common::RemoteParticipantRegistry for ForeignRegistry {
    fn remote_client_protocol(&self, identity: &lk_common::ParticipantIdentity) -> i32 {
        self.0.remote_client_protocol(identity.to_string())
    }

    fn remote_capabilities(
        &self,
        identity: &lk_common::ParticipantIdentity,
    ) -> Vec<lk_common::ClientCapability> {
        self.0.remote_capabilities(identity.to_string()).into_iter().map(Into::into).collect()
    }

    fn remote_identities(&self) -> Vec<lk_common::ParticipantIdentity> {
        self.0.remote_identities().into_iter().map(Into::into).collect()
    }
}

#[uniffi::export(async_runtime = "tokio")]
impl OutgoingDataStreamManager {
    #[uniffi::constructor]
    pub fn new(
        delegate: Arc<dyn OutgoingDataStreamManagerDelegate>,
        registry: Arc<dyn RemoteParticipantRegistryDelegate>,
    ) -> Arc<Self> {
        let token = CancellationToken::new();
        let (manager, mut packet_rx) = ds::outgoing::Manager::new();

        // Forward each outbound packet to the transport delegate and acknowledge the send. Wire
        // send-failures are not propagated back to the originating `send_*` call for now (matches
        // the data-track delegate); can be upgraded to a Result-returning delegate later.
        let forward_token = token.clone();
        crate::runtime::runtime().spawn(async move {
            loop {
                tokio::select! {
                    _ = forward_token.cancelled() => break,
                    recv = packet_rx.recv() => match recv {
                        Ok((packet, responder)) => {
                            delegate.on_packets_available(vec![Bytes::from(packet.encode_to_vec())]);
                            let _ = responder.respond(Ok(()));
                        }
                        Err(_) => break,
                    }
                }
            }
        });

        let registry: Arc<dyn lk_common::RemoteParticipantRegistry> =
            Arc::new(ForeignRegistry(registry));
        Self { manager, registry, _guard: token.drop_guard() }.into()
    }

    /// Sends a complete text payload, returning info about the created stream.
    pub async fn send_text(
        &self,
        text: String,
        options: StreamTextOptions,
    ) -> Result<TextStreamInfo, DataStreamError> {
        Ok(self.manager.send_text(&text, options.into(), &*self.registry).await?.into())
    }

    /// Sends a complete byte payload, returning info about the created stream.
    pub async fn send_bytes(
        &self,
        data: Bytes,
        options: StreamByteOptions,
    ) -> Result<ByteStreamInfo, DataStreamError> {
        Ok(self.manager.send_bytes(data, options.into(), &*self.registry).await?.into())
    }

    /// Streams a file from disk, returning info about the created stream.
    pub async fn send_file(
        &self,
        path: String,
        options: StreamByteOptions,
    ) -> Result<ByteStreamInfo, DataStreamError> {
        Ok(self.manager.send_file(path, options.into(), &*self.registry).await?.into())
    }

    /// Opens an incremental text stream writer (never compressed or inlined).
    pub async fn stream_text(
        &self,
        options: StreamTextOptions,
    ) -> Result<TextStreamWriter, DataStreamError> {
        Ok(TextStreamWriter(self.manager.stream_text(options.into()).await?))
    }

    /// Opens an incremental byte stream writer (never compressed or inlined).
    pub async fn stream_bytes(
        &self,
        options: StreamByteOptions,
    ) -> Result<ByteStreamWriter, DataStreamError> {
        Ok(ByteStreamWriter(self.manager.stream_bytes(options.into()).await?))
    }
}

/// Writer for an open text data stream.
#[derive(uniffi::Object)]
pub struct TextStreamWriter(ds_api::TextStreamWriter);

#[uniffi::export(async_runtime = "tokio")]
impl TextStreamWriter {
    /// Information about the underlying stream.
    pub fn info(&self) -> TextStreamInfo {
        self.0.info().clone().into()
    }

    /// Whether the stream is still open — false once it has been closed locally or a send has failed.
    pub async fn is_open(&self) -> bool {
        !self.0.is_closed().await
    }

    /// Appends text to the stream.
    pub async fn write(&self, text: String) -> Result<(), DataStreamError> {
        Ok(self.0.write(&text).await?)
    }

    /// Closes the stream normally.
    pub async fn close(&self) -> Result<(), DataStreamError> {
        Ok(self.0.clone().close().await?)
    }

    /// Closes the stream abnormally with a reason.
    pub async fn close_with_reason(&self, reason: String) -> Result<(), DataStreamError> {
        Ok(self.0.clone().close_with_reason(&reason).await?)
    }
}

/// Writer for an open byte data stream.
#[derive(uniffi::Object)]
pub struct ByteStreamWriter(ds_api::ByteStreamWriter);

#[uniffi::export(async_runtime = "tokio")]
impl ByteStreamWriter {
    /// Information about the underlying stream.
    pub fn info(&self) -> ByteStreamInfo {
        self.0.info().clone().into()
    }

    /// Whether the stream is still open — false once it has been closed locally or a send has failed.
    pub async fn is_open(&self) -> bool {
        !self.0.is_closed().await
    }

    /// Appends bytes to the stream.
    pub async fn write(&self, data: Bytes) -> Result<(), DataStreamError> {
        Ok(self.0.write(data.as_ref()).await?)
    }

    /// Closes the stream normally.
    pub async fn close(&self) -> Result<(), DataStreamError> {
        Ok(self.0.clone().close().await?)
    }

    /// Closes the stream abnormally with a reason.
    pub async fn close_with_reason(&self, reason: String) -> Result<(), DataStreamError> {
        Ok(self.0.clone().close_with_reason(&reason).await?)
    }
}
