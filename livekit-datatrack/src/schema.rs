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

use livekit_protocol as proto;
use std::sync::Arc;

/// Identifier for a data track schema.
///
/// A schema ID is a compound identifier consisting of two components:
/// - Name (e.g. "joint_positions")
/// - Encoding
///
/// Two schema IDs with the same name but different encodings are not equivalent.
///
/// Clones of this type are cheap since the name component is reference counted.
///
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct DataTrackSchemaId {
    name: Arc<str>,
    encoding: DataTrackSchemaEncoding,
}

impl DataTrackSchemaId {
    /// Creates a new schema ID.
    pub fn new(name: impl Into<String>, encoding: DataTrackSchemaEncoding) -> Self {
        Self { name: Arc::<str>::from(name.into()), encoding }
    }

    /// Returns the name component of the ID.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the encoding component of the ID.
    pub fn encoding(&self) -> DataTrackSchemaEncoding {
        self.encoding
    }
}

/// Encoding used for a schema definition.
///
/// See also: [`DataTrackSchemaId`]
///
#[non_exhaustive]
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
#[cfg_attr(test, derive(fake::Dummy))]
pub enum DataTrackSchemaEncoding {
    /// Protocol Buffer IDL, describes [`Protobuf`] encoded frames.
    ///
    /// [`Protobuf`]: DataTrackFrameEncoding::Protobuf
    Protobuf,
    /// FlatBuffer IDL, describes [`Flatbuffer`] encoded frames.
    ///
    /// [`Flatbuffer`]: DataTrackFrameEncoding::Flatbuffer
    Flatbuffer,
    /// ROS 1 Message, describes [`Ros1`] encoded frames.
    ///
    /// [`Ros1`]: DataTrackFrameEncoding::Ros1
    Ros1Msg,
    /// ROS 2 Message, describes [`Cdr`] encoded frames.
    ///
    /// [`Cdr`]: DataTrackFrameEncoding::Cdr
    Ros2Msg,
    /// ROS 2 IDL, describes [`Cdr`] encoded frames.
    ///
    /// [`Cdr`]: DataTrackFrameEncoding::Cdr
    Ros2Idl,
    /// OMG IDL, describes [`Cdr`] encoded frames.
    ///
    /// [`Cdr`]: DataTrackFrameEncoding::Cdr
    OmgIdl,
    /// JSON Schema, describes [`Json`] encoded frames.
    ///
    /// [`Json`]: DataTrackFrameEncoding::Json
    JsonSchema,
    /// Another encoding not known to this client version.
    Other,
}

/// Encoding used for frames pushed on a data track.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
#[cfg_attr(test, derive(fake::Dummy))]
pub enum DataTrackFrameEncoding {
    /// ROS 1, must be described by a [`Ros1Msg`] schema.
    ///
    /// [`Ros1Msg`]: DataTrackSchemaEncoding::Ros1Msg
    Ros1,
    /// CDR, must be described by a [`Ros2Msg`], [`Ros2Idl`], or [`OmgIdl`] schema.
    ///
    /// [`Ros2Msg`]: DataTrackSchemaEncoding::Ros2Msg
    /// [`Ros2Idl`]: DataTrackSchemaEncoding::Ros2Idl
    /// [`OmgIdl`]: DataTrackSchemaEncoding::OmgIdl
    Cdr,
    /// Protocol Buffer, must be described by a [`Protobuf`] schema.
    ///
    /// [`Protobuf`]: DataTrackSchemaEncoding::Protobuf
    Protobuf,
    /// FlatBuffer, must be described by a [`Flatbuffer`] schema.
    ///
    /// [`Flatbuffer`]: DataTrackSchemaEncoding::Flatbuffer
    Flatbuffer,
    /// CBOR, self-describing.
    Cbor,
    /// MessagePack, self-describing.
    Msgpack,
    /// JSON, self-describing or described by a [`JsonSchema`] schema.
    ///
    /// [`JsonSchema`]: DataTrackSchemaEncoding::JsonSchema
    Json,
    /// Another encoding not known to this client version.
    Other,
}

impl From<proto::DataTrackSchemaId> for DataTrackSchemaId {
    fn from(msg: proto::DataTrackSchemaId) -> Self {
        let encoding = msg.encoding().into();
        DataTrackSchemaId::new(msg.name, encoding)
    }
}

impl From<DataTrackSchemaId> for proto::DataTrackSchemaId {
    fn from(value: DataTrackSchemaId) -> Self {
        Self {
            name: value.name.to_string(),
            encoding: proto::DataTrackSchemaEncoding::from(value.encoding) as i32,
        }
    }
}

impl From<proto::DataTrackSchemaEncoding> for DataTrackSchemaEncoding {
    fn from(msg: proto::DataTrackSchemaEncoding) -> Self {
        match msg {
            proto::DataTrackSchemaEncoding::Unspecified => Self::Other,
            proto::DataTrackSchemaEncoding::Protobuf => Self::Protobuf,
            proto::DataTrackSchemaEncoding::Flatbuffer => Self::Flatbuffer,
            proto::DataTrackSchemaEncoding::Ros1Msg => Self::Ros1Msg,
            proto::DataTrackSchemaEncoding::Ros2Msg => Self::Ros2Msg,
            proto::DataTrackSchemaEncoding::Ros2Idl => Self::Ros2Idl,
            proto::DataTrackSchemaEncoding::OmgIdl => Self::OmgIdl,
            proto::DataTrackSchemaEncoding::JsonSchema => Self::JsonSchema,
        }
    }
}

impl From<DataTrackSchemaEncoding> for proto::DataTrackSchemaEncoding {
    fn from(value: DataTrackSchemaEncoding) -> Self {
        match value {
            DataTrackSchemaEncoding::Other => Self::Unspecified,
            DataTrackSchemaEncoding::Protobuf => Self::Protobuf,
            DataTrackSchemaEncoding::Flatbuffer => Self::Flatbuffer,
            DataTrackSchemaEncoding::Ros1Msg => Self::Ros1Msg,
            DataTrackSchemaEncoding::Ros2Msg => Self::Ros2Msg,
            DataTrackSchemaEncoding::Ros2Idl => Self::Ros2Idl,
            DataTrackSchemaEncoding::OmgIdl => Self::OmgIdl,
            DataTrackSchemaEncoding::JsonSchema => Self::JsonSchema,
        }
    }
}

impl From<proto::DataTrackFrameEncoding> for DataTrackFrameEncoding {
    fn from(msg: proto::DataTrackFrameEncoding) -> Self {
        match msg {
            proto::DataTrackFrameEncoding::Unspecified => Self::Other,
            proto::DataTrackFrameEncoding::Ros1 => Self::Ros1,
            proto::DataTrackFrameEncoding::Cdr => Self::Cdr,
            proto::DataTrackFrameEncoding::Protobuf => Self::Protobuf,
            proto::DataTrackFrameEncoding::Flatbuffer => Self::Flatbuffer,
            proto::DataTrackFrameEncoding::Cbor => Self::Cbor,
            proto::DataTrackFrameEncoding::Msgpack => Self::Msgpack,
            proto::DataTrackFrameEncoding::Json => Self::Json,
        }
    }
}

impl From<DataTrackFrameEncoding> for proto::DataTrackFrameEncoding {
    fn from(value: DataTrackFrameEncoding) -> Self {
        match value {
            DataTrackFrameEncoding::Other => Self::Unspecified,
            DataTrackFrameEncoding::Ros1 => Self::Ros1,
            DataTrackFrameEncoding::Cdr => Self::Cdr,
            DataTrackFrameEncoding::Protobuf => Self::Protobuf,
            DataTrackFrameEncoding::Flatbuffer => Self::Flatbuffer,
            DataTrackFrameEncoding::Cbor => Self::Cbor,
            DataTrackFrameEncoding::Msgpack => Self::Msgpack,
            DataTrackFrameEncoding::Json => Self::Json,
        }
    }
}

#[cfg(test)]
impl fake::Dummy<fake::Faker> for DataTrackSchemaId {
    fn dummy_with_rng<R: rand::Rng + ?Sized>(_: &fake::Faker, rng: &mut R) -> Self {
        use fake::{Fake, Faker};
        let name: String = Faker.fake_with_rng(rng);
        let encoding: DataTrackSchemaEncoding = Faker.fake_with_rng(rng);
        Self::new(name, encoding)
    }
}
