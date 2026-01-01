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

use crate::{
    data_track::internal::{DecryptionProvider, E2eeError, EncryptedPayload, EncryptionProvider},
    E2eeManager,
};

impl DecryptionProvider for E2eeManager {
    fn decrypt(&self, payload: EncryptedPayload) -> Result<bytes::Bytes, E2eeError> {
        todo!()
    }
}

impl EncryptionProvider for E2eeManager {
    fn encrypt(&self, payload: bytes::Bytes) -> Result<EncryptedPayload, E2eeError> {
        todo!()
    }
}
