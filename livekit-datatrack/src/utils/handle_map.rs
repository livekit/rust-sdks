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

use crate::{api::DataTrackSid, dtp::Handle};
use std::collections::HashMap;

/// Map between track handle and SID.
///
/// All operations are O(1).
///
#[derive(Debug, Default)]
pub struct HandleMap {
    sid_to_handle: HashMap<DataTrackSid, Handle>,
    handle_to_sid: HashMap<Handle, DataTrackSid>,
}

impl HandleMap {
    /// Insert the given mapping between track handle and SID.
    ///
    /// Returns a Boolean indicating whether the entry was inserted.
    /// Insertion will fail if the mapping already exists in either direction.
    ///
    pub fn insert(&mut self, handle: Handle, sid: DataTrackSid) -> bool {
        if self.sid_to_handle.contains_key(&sid) || self.handle_to_sid.contains_key(&handle) {
            return false;
        }
        self.sid_to_handle.insert(sid.clone(), handle);
        self.handle_to_sid.insert(handle, sid);
        return true;
    }

    /// Get the SID associated with the given handle.
    pub fn get_sid(&self, handle: Handle) -> Option<&DataTrackSid> {
        self.handle_to_sid.get(&handle)
    }

    /// Remove the mapping with the given SID.
    pub fn remove(&mut self, sid: &DataTrackSid) {
        let Some(handle) = self.sid_to_handle.remove(sid) else { return };
        self.handle_to_sid.remove(&handle);
    }
}
