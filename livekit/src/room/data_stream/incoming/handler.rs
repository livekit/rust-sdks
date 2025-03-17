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
    reader::{ByteStreamReader, TextStreamReader},
    AnyStreamReader, StreamReader,
};
use crate::{
    data_stream::{info::StreamInfo, StreamError, StreamResult},
    id::ParticipantIdentity,
};
use std::{collections::HashMap, error::Error, future::Future, pin::Pin, sync::Arc};

type StreamHandlerResult = Result<(), Box<dyn Error + Send + Sync>>;
pub type StreamHandlerFuture = Pin<Box<dyn Future<Output = StreamHandlerResult> + Send>>;

type StreamHandler<R> = Arc<dyn Fn(R, ParticipantIdentity) -> StreamHandlerFuture + Send + Sync>;
type TextStreamHandler = StreamHandler<TextStreamReader>;
type ByteStreamHandler = StreamHandler<ByteStreamReader>;

/// Registry for incoming data stream handlers.
#[derive(Default)]
pub struct HandlerRegistry {
    byte_handlers: HashMap<String, ByteStreamHandler>,
    text_handlers: HashMap<String, TextStreamHandler>,
}

impl HandlerRegistry {
    pub fn register_byte_stream_handler(
        &mut self,
        topic: String,
        handler: ByteStreamHandler,
    ) -> StreamResult<()> {
        if self.byte_handlers.contains_key(&topic) {
            Err(StreamError::HandlerAlreadyRegistered)?
        }
        self.byte_handlers.insert(topic, handler);
        Ok(())
    }

    pub fn register_text_stream_handler(
        &mut self,
        topic: String,
        handler: TextStreamHandler,
    ) -> StreamResult<()> {
        if self.text_handlers.contains_key(&topic) {
            Err(StreamError::HandlerAlreadyRegistered)?
        }
        self.text_handlers.insert(topic, handler);
        Ok(())
    }

    /// Dispatch the given stream reader to a registered handler (if one is registered).
    pub(super) fn dispatch(&self, reader: AnyStreamReader, identity: ParticipantIdentity) -> bool {
        match reader {
            AnyStreamReader::Byte(reader) => {
                let topic = reader.info().topic();
                let Some(handler) = self.byte_handlers.get(topic) else {
                    return false;
                };
                tokio::spawn(handler(reader, identity));
                true
            }
            AnyStreamReader::Text(reader) => {
                let topic = reader.info().topic();
                let Some(handler) = self.text_handlers.get(topic) else {
                    return false;
                };
                tokio::spawn(handler(reader, identity));
                true
            }
        }
    }
}
