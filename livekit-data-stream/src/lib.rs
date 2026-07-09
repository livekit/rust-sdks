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

#![doc = include_str!("../README.md")]

mod utils;
pub use utils::{SendError, StreamError, StreamResult};

mod types;
pub use types::{
    ByteHeader, Chunk, CompressionType, ContentHeader, Header, OperationType, Packet, StreamId,
    TextHeader, Trailer,
};

mod info;
pub use info::{ByteStreamInfo, TextStreamInfo};

mod utf8_chunk;

mod incoming;
pub use incoming::{
    AnyStreamReader, ByteStreamReader, IncomingEvent, IncomingOutput, IncomingStreamInput,
    IncomingStreamManager, StreamReader, TextStreamReader,
};

mod outgoing;
pub use outgoing::{
    ByteStreamWriter, OutgoingStreamManager, StreamByteOptions, StreamTextOptions, StreamWriter,
    TextStreamWriter,
};
