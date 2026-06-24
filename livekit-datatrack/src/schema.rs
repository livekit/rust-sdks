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
/// different encoding refers to a distinct schema. Cloning this type is cheap.
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
#[derive(Clone, Hash, PartialEq, Eq)]
pub struct DataTrackSchemaId(Arc<DataTrackSchemaIdInner>);

#[derive(Hash, PartialEq, Eq)]
struct DataTrackSchemaIdInner {
    name: String,
    encoding: DataTrackSchemaEncoding,
}

impl std::fmt::Debug for DataTrackSchemaId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DataTrackSchemaId")
            .field("name", &self.0.name)
            .field("encoding", &self.0.encoding)
            .finish()
    }
}

impl DataTrackSchemaId {
    /// Creates a new schema ID.
    pub fn new(name: impl Into<String>, encoding: DataTrackSchemaEncoding) -> Self {
        let inner = DataTrackSchemaIdInner { name: name.into(), encoding };
        Self(inner.into())
    }

    /// Returns the name component of the ID.
    pub fn name(&self) -> &str {
        &self.0.name
    }

    /// Returns the encoding component of the ID.
    pub fn encoding(&self) -> &DataTrackSchemaEncoding {
        &self.0.encoding
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

    /// Another well-known encoding not known to this client version.
    Other,
    /// An application-specific encoding identified by the contained string.
    ///
    /// Prefer using one of the well-known encodings unless the format is not enumerated.
    /// The identifier must be non-empty and no longer than 25 characters.
    ///
    Custom(String),
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

    /// Another well-known encoding not known to this client version.
    Other,
    /// An application-specific encoding identified by the contained string.
    ///
    /// Prefer using one of the well-known encodings unless the format is not enumerated.
    /// The identifier must be non-empty and no longer than 25 characters.
    ///
    Custom(String),
}

impl From<proto::DataTrackSchemaId> for DataTrackSchemaId {
    fn from(msg: proto::DataTrackSchemaId) -> Self {
        let encoding = msg.encoding.map(Into::into).unwrap_or(DataTrackSchemaEncoding::Other);
        DataTrackSchemaId::new(msg.name, encoding)
    }
}

impl From<DataTrackSchemaId> for proto::DataTrackSchemaId {
    fn from(value: DataTrackSchemaId) -> Self {
        Self { name: value.name().to_string(), encoding: Some(value.encoding().clone().into()) }
    }
}

impl From<proto::DataTrackSchemaEncoding> for DataTrackSchemaEncoding {
    fn from(msg: proto::DataTrackSchemaEncoding) -> Self {
        use proto::data_track_schema_encoding::{Value, WellKnownSchemaEncoding as WellKnown};
        match msg.value {
            Some(Value::WellKnown(value)) => match WellKnown::try_from(value) {
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
            Some(Value::Custom(name)) => Self::Custom(name),
            None => Self::Other,
        }
    }
}

impl From<DataTrackSchemaEncoding> for proto::DataTrackSchemaEncoding {
    fn from(value: DataTrackSchemaEncoding) -> Self {
        use proto::data_track_schema_encoding::{Value, WellKnownSchemaEncoding as WellKnown};
        let value = match value {
            DataTrackSchemaEncoding::Protobuf => Value::WellKnown(WellKnown::Protobuf as i32),
            DataTrackSchemaEncoding::Flatbuffer => Value::WellKnown(WellKnown::Flatbuffer as i32),
            DataTrackSchemaEncoding::Ros1Msg => Value::WellKnown(WellKnown::Ros1Msg as i32),
            DataTrackSchemaEncoding::Ros2Msg => Value::WellKnown(WellKnown::Ros2Msg as i32),
            DataTrackSchemaEncoding::Ros2Idl => Value::WellKnown(WellKnown::Ros2Idl as i32),
            DataTrackSchemaEncoding::OmgIdl => Value::WellKnown(WellKnown::OmgIdl as i32),
            DataTrackSchemaEncoding::JsonSchema => Value::WellKnown(WellKnown::JsonSchema as i32),
            DataTrackSchemaEncoding::Custom(name) => Value::Custom(name),
            DataTrackSchemaEncoding::Other => Value::WellKnown(WellKnown::Unspecified as i32),
        }
        .into();
        Self { value }
    }
}

impl From<proto::DataTrackFrameEncoding> for DataTrackFrameEncoding {
    fn from(msg: proto::DataTrackFrameEncoding) -> Self {
        use proto::data_track_frame_encoding::{Value, WellKnownFrameEncoding as WellKnown};
        match msg.value {
            Some(Value::WellKnown(value)) => match WellKnown::try_from(value) {
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
            Some(Value::Custom(name)) => Self::Custom(name),
            None => Self::Other,
        }
    }
}

impl From<DataTrackFrameEncoding> for proto::DataTrackFrameEncoding {
    fn from(value: DataTrackFrameEncoding) -> Self {
        use proto::data_track_frame_encoding::{Value, WellKnownFrameEncoding as WellKnown};
        let value = match value {
            DataTrackFrameEncoding::Ros1 => Value::WellKnown(WellKnown::Ros1 as i32),
            DataTrackFrameEncoding::Cdr => Value::WellKnown(WellKnown::Cdr as i32),
            DataTrackFrameEncoding::Protobuf => Value::WellKnown(WellKnown::Protobuf as i32),
            DataTrackFrameEncoding::Flatbuffer => Value::WellKnown(WellKnown::Flatbuffer as i32),
            DataTrackFrameEncoding::Cbor => Value::WellKnown(WellKnown::Cbor as i32),
            DataTrackFrameEncoding::Msgpack => Value::WellKnown(WellKnown::Msgpack as i32),
            DataTrackFrameEncoding::Json => Value::WellKnown(WellKnown::Json as i32),
            DataTrackFrameEncoding::Custom(name) => Value::Custom(name),
            DataTrackFrameEncoding::Other => Value::WellKnown(WellKnown::Unspecified as i32),
        }
        .into();
        Self { value }
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
