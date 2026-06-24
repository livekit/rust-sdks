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
/// A compound identifier with two components: name and encoding.
///
/// Two IDs are equal only if both components match; the same name with a
/// different encoding refers to a distinct schema. Cloning is cheap, as the name
/// component is reference counted.
///
/// # Examples
///
/// ```
/// # use livekit_datatrack::api::{DataTrackSchemaId, DataTrackSchemaEncoding};
/// let schema = DataTrackSchemaId::new("my_schema", DataTrackSchemaEncoding::Protobuf);
///
/// assert_eq!(schema.name(), "my_schema");
/// assert_eq!(schema.encoding(), &DataTrackSchemaEncoding::Protobuf);
/// ```
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
    pub fn encoding(&self) -> &DataTrackSchemaEncoding {
        &self.encoding
    }
}

/// Encoding used for a schema definition.
///
/// Identifies the interface definition language the schema is written in (e.g. a
/// `.proto` file for [`Protobuf`]). This in turn dictates the wire format of the
/// frames the schema describes, captured by [`DataTrackFrameEncoding`].
///
/// [`Protobuf`]: DataTrackSchemaEncoding::Protobuf
///
#[non_exhaustive]
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
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
    /// A custom encoding identified by the contained string.
    ///
    /// Escape hatch for encodings outside the well-known set. The identifier
    /// must be non-empty and no longer than 25 characters.
    ///
    Custom(String),
    /// Another encoding not known to this client version.
    Other,
}

/// Encoding used for frames pushed on a data track.
///
/// The serialization format of the frame bytes (e.g. [`Protobuf`]); the structure
/// of those bytes is described by a schema, see [`DataTrackSchemaEncoding`].
///
/// [`Protobuf`]: DataTrackFrameEncoding::Protobuf
///
#[non_exhaustive]
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
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
    /// A custom encoding identified by the contained string.
    ///
    /// Escape hatch for encodings outside the well-known set. The identifier
    /// must be non-empty and no longer than 25 characters.
    ///
    Custom(String),
    /// Another encoding not known to this client version.
    Other,
}

impl From<proto::DataTrackSchemaId> for DataTrackSchemaId {
    fn from(msg: proto::DataTrackSchemaId) -> Self {
        let encoding = msg.encoding.map(Into::into).unwrap_or(DataTrackSchemaEncoding::Other);
        DataTrackSchemaId::new(msg.name, encoding)
    }
}

impl From<DataTrackSchemaId> for proto::DataTrackSchemaId {
    fn from(value: DataTrackSchemaId) -> Self {
        Self { name: value.name.to_string(), encoding: Some(value.encoding.into()) }
    }
}

impl From<proto::DataTrackSchemaEncoding> for DataTrackSchemaEncoding {
    fn from(msg: proto::DataTrackSchemaEncoding) -> Self {
        use proto::data_track_schema_encoding::{Encoding, WellKnownSchemaEncoding as WellKnown};
        match msg.encoding {
            Some(Encoding::WellKnown(value)) => match WellKnown::try_from(value) {
                Ok(WellKnown::Protobuf) => Self::Protobuf,
                Ok(WellKnown::Flatbuffer) => Self::Flatbuffer,
                Ok(WellKnown::Ros1Msg) => Self::Ros1Msg,
                Ok(WellKnown::Ros2Msg) => Self::Ros2Msg,
                Ok(WellKnown::Ros2Idl) => Self::Ros2Idl,
                Ok(WellKnown::OmgIdl) => Self::OmgIdl,
                Ok(WellKnown::JsonSchema) => Self::JsonSchema,
                // Unspecified or a value introduced after this client version.
                Ok(WellKnown::Unspecified) | Err(_) => Self::Other,
            },
            Some(Encoding::Custom(name)) => Self::Custom(name),
            None => Self::Other,
        }
    }
}

impl From<DataTrackSchemaEncoding> for proto::DataTrackSchemaEncoding {
    fn from(value: DataTrackSchemaEncoding) -> Self {
        use proto::data_track_schema_encoding::{Encoding, WellKnownSchemaEncoding as WellKnown};
        let encoding = match value {
            DataTrackSchemaEncoding::Protobuf => Encoding::WellKnown(WellKnown::Protobuf as i32),
            DataTrackSchemaEncoding::Flatbuffer => Encoding::WellKnown(WellKnown::Flatbuffer as i32),
            DataTrackSchemaEncoding::Ros1Msg => Encoding::WellKnown(WellKnown::Ros1Msg as i32),
            DataTrackSchemaEncoding::Ros2Msg => Encoding::WellKnown(WellKnown::Ros2Msg as i32),
            DataTrackSchemaEncoding::Ros2Idl => Encoding::WellKnown(WellKnown::Ros2Idl as i32),
            DataTrackSchemaEncoding::OmgIdl => Encoding::WellKnown(WellKnown::OmgIdl as i32),
            DataTrackSchemaEncoding::JsonSchema => Encoding::WellKnown(WellKnown::JsonSchema as i32),
            DataTrackSchemaEncoding::Custom(name) => Encoding::Custom(name),
            DataTrackSchemaEncoding::Other => Encoding::WellKnown(WellKnown::Unspecified as i32),
        };
        Self { encoding: Some(encoding) }
    }
}

impl From<proto::DataTrackFrameEncoding> for DataTrackFrameEncoding {
    fn from(msg: proto::DataTrackFrameEncoding) -> Self {
        use proto::data_track_frame_encoding::{Encoding, WellKnownFrameEncoding as WellKnown};
        match msg.encoding {
            Some(Encoding::WellKnown(value)) => match WellKnown::try_from(value) {
                Ok(WellKnown::Ros1) => Self::Ros1,
                Ok(WellKnown::Cdr) => Self::Cdr,
                Ok(WellKnown::Protobuf) => Self::Protobuf,
                Ok(WellKnown::Flatbuffer) => Self::Flatbuffer,
                Ok(WellKnown::Cbor) => Self::Cbor,
                Ok(WellKnown::Msgpack) => Self::Msgpack,
                Ok(WellKnown::Json) => Self::Json,
                // Unspecified or a value introduced after this client version.
                Ok(WellKnown::Unspecified) | Err(_) => Self::Other,
            },
            Some(Encoding::Custom(name)) => Self::Custom(name),
            None => Self::Other,
        }
    }
}

impl From<DataTrackFrameEncoding> for proto::DataTrackFrameEncoding {
    fn from(value: DataTrackFrameEncoding) -> Self {
        use proto::data_track_frame_encoding::{Encoding, WellKnownFrameEncoding as WellKnown};
        let encoding = match value {
            DataTrackFrameEncoding::Ros1 => Encoding::WellKnown(WellKnown::Ros1 as i32),
            DataTrackFrameEncoding::Cdr => Encoding::WellKnown(WellKnown::Cdr as i32),
            DataTrackFrameEncoding::Protobuf => Encoding::WellKnown(WellKnown::Protobuf as i32),
            DataTrackFrameEncoding::Flatbuffer => Encoding::WellKnown(WellKnown::Flatbuffer as i32),
            DataTrackFrameEncoding::Cbor => Encoding::WellKnown(WellKnown::Cbor as i32),
            DataTrackFrameEncoding::Msgpack => Encoding::WellKnown(WellKnown::Msgpack as i32),
            DataTrackFrameEncoding::Json => Encoding::WellKnown(WellKnown::Json as i32),
            DataTrackFrameEncoding::Custom(name) => Encoding::Custom(name),
            DataTrackFrameEncoding::Other => Encoding::WellKnown(WellKnown::Unspecified as i32),
        };
        Self { encoding: Some(encoding) }
    }
}

impl From<DataTrackSchemaId> for proto::DataBlobKey {
    fn from(id: DataTrackSchemaId) -> Self {
        Self { key: Some(proto::data_blob_key::Key::SchemaId(id.into())) }
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
