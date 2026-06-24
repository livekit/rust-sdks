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

#[cfg(feature = "__lk-e2e-test")]
use {
    anyhow::{Ok, Result},
    common::test_rooms,
    livekit::data_track::{DataTrackSchemaEncoding, DataTrackSchemaId},
    test_case::test_case
};

mod common;

const MAX_SCHEMA_DEFINITION_SIZE: usize = 60_000;

#[cfg(feature = "__lk-e2e-test")]
#[test_case(DataTrackSchemaEncoding::JsonSchema ; "json_schema")]
#[test_case(DataTrackSchemaEncoding::Protobuf ; "protobuf")]
#[test_case(DataTrackSchemaEncoding::Custom("a".to_string()) ; "custom")]
#[test_log::test(tokio::test)]
async fn test_define_schema(encoding: DataTrackSchemaEncoding) -> Result<()> {
    let mut rooms = test_rooms(2).await?;
    let (pub_room, _) = rooms.pop().unwrap();
    let (sub_room, _) = rooms.pop().unwrap();
    let identity = pub_room.local_participant().identity();

    let id = DataTrackSchemaId::new("some_schema", encoding);
    let definition = "a".repeat(MAX_SCHEMA_DEFINITION_SIZE);

    pub_room.local_participant().define_schema(id.clone(), definition.clone()).await?;

    let retrieved = sub_room.local_participant().get_schema(id, identity).await?;
    assert_eq!(retrieved, definition);

    Ok(())
}

#[cfg(feature = "__lk-e2e-test")]
#[test_log::test(tokio::test)]
async fn test_define_schema_over_limit() -> Result<()> {
    let (room, _) = test_rooms(1).await?.pop().unwrap();

    let id = DataTrackSchemaId::new("some_schema", DataTrackSchemaEncoding::JsonSchema);
    let definition = "a".repeat(2 * MAX_SCHEMA_DEFINITION_SIZE); // Deliberately over size limit

    let result = room.local_participant().define_schema(id, definition).await;
    assert!(result.is_err());

    Ok(())
}

#[cfg(feature = "__lk-e2e-test")]
#[test_log::test(tokio::test)]
async fn test_define_schema_duplicate() -> Result<()> {
    let (room, _) = test_rooms(1).await?.pop().unwrap();

    let id = DataTrackSchemaId::new("some_schema", DataTrackSchemaEncoding::JsonSchema);
    let definition = "a".repeat(MAX_SCHEMA_DEFINITION_SIZE);

    room.local_participant().define_schema(id.clone(), definition.clone()).await?;

    // Define the same schema again
    let result = room.local_participant().define_schema(id, definition).await;
    assert!(result.is_err());

    Ok(())
}

#[cfg(feature = "__lk-e2e-test")]
#[test_log::test(tokio::test)]
async fn test_get_undefined_schema() -> Result<()> {
    let (room, _) = test_rooms(1).await?.pop().unwrap();
    let identity = room.local_participant().identity();

    let id =  DataTrackSchemaId::new("undefined", DataTrackSchemaEncoding::JsonSchema);
    let result = room.local_participant().get_schema(id, identity).await;
    assert!(result.is_err());

    Ok(())
}