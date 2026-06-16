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
    bytes::Bytes,
    common::test_rooms,
    livekit_protocol as proto,
};

mod common;

const MAX_DATA_BLOB_SIZE: usize = 60_000;

#[cfg(feature = "__lk-e2e-test")]
#[test_log::test(tokio::test)]
async fn test_store_data_blob() -> Result<()> {
    let mut rooms = test_rooms(2).await?;
    let (pub_room, _) = rooms.pop().unwrap();
    let (sub_room, _) = rooms.pop().unwrap();
    let identity = pub_room.local_participant().identity();

    let key = data_blob_key("some_key");
    let contents = Bytes::from_static(&[0xFA; MAX_DATA_BLOB_SIZE]);

    pub_room.local_participant().store_data_blob(key.clone(), contents.clone()).await?;

    let definition = sub_room.local_participant().get_data_blob(key, identity).await?;
    assert_eq!(definition, contents);

    Ok(())
}

#[cfg(feature = "__lk-e2e-test")]
#[test_log::test(tokio::test)]
async fn test_store_data_blob_over_limit() -> Result<()> {
    let (room, _) = test_rooms(1).await?.pop().unwrap();

    let key = data_blob_key("some_key");
    let contents = Bytes::from_static(&[0xFA; 2 * MAX_DATA_BLOB_SIZE]); // Deliberately over size limit

    let result = room.local_participant().store_data_blob(key, contents).await;
    assert!(result.is_err());

    Ok(())
}

#[cfg(feature = "__lk-e2e-test")]
#[test_log::test(tokio::test)]
async fn test_store_data_blob_duplicate() -> Result<()> {
    let (room, _) = test_rooms(1).await?.pop().unwrap();

    let key = data_blob_key("some_key");
    let contents = Bytes::from_static(&[0xFA; MAX_DATA_BLOB_SIZE]);

    room.local_participant().store_data_blob(key.clone(), contents.clone()).await?;

    // Store under same key again
    let result = room.local_participant().store_data_blob(key, contents).await;
    assert!(result.is_err());

    Ok(())
}

#[cfg(feature = "__lk-e2e-test")]
#[test_log::test(tokio::test)]
async fn test_get_data_blob_unknown_key() -> Result<()> {
    let (room, _) = test_rooms(1).await?.pop().unwrap();
    let identity = room.local_participant().identity();

    let key = data_blob_key("unknown_key");
    let result = room.local_participant().get_data_blob(key, identity).await;

    assert!(result.is_err());

    Ok(())
}

#[cfg(feature = "__lk-e2e-test")]
fn data_blob_key(string: &str) -> proto::DataBlobKey {
    proto::DataBlobKey { key: Some(proto::data_blob_key::Key::Generic(string.to_string())) }
}
