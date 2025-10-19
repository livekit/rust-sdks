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

#[derive(Clone)]
pub struct Encryption<'a> {
    pub(crate) key_index: u8,
    pub(crate) iv: &'a Iv,
}

impl<'a> Encryption<'a> {
    pub fn key_index(&self) -> u8 {
        self.key_index
    }
    pub fn iv(&self) -> &Iv {
        self.iv
    }
}

/// AES initialization vector.
pub type Iv = [u8; consts::E2EE_EXT_IV_LEN];

pub(crate) mod consts {
    pub const SUPPORTED_VERSION: u8 = 0;
    pub const BASE_HEADER_LEN: usize = 8;

    // Bitfield shifts and masks for header flags
    pub const VERSION_SHIFT: u8 = 5;
    pub const VERSION_MASK: u8 = 0xE0;
    pub const FINAL_FLAG_SHIFT: u8 = 4;
    pub const FINAL_FLAG_MASK: u8 = 0x10;
    pub const E2EE_FLAG_SHIFT: u8 = 3;
    pub const E2EE_FLAG_MASK: u8 = 0x08;
    pub const TS_FLAG_SHIFT: u8 = 2;
    pub const TS_FLAG_MASK: u8 = 0x04;
    pub const UTS_FLAG_SHIFT: u8 = 1;
    pub const UTS_FLAG_MASK: u8 = 0x02;

    // Header field offsets and lengths
    pub const EXT_WORDS_OFFSET: usize = 3;
    pub const EXT_WORDS_LEN: usize = 1;
    pub const TRACK_HANDLE_OFFSET: usize = 4;
    pub const TRACK_HANDLE_LEN: usize = 2;
    pub const SEQUENCE_OFFSET: usize = 6;
    pub const SEQUENCE_LEN: usize = 2;

    // Start of extension fields
    pub const EXT_START_OFFSET: usize = BASE_HEADER_LEN;

    // Extension lengths
    pub const TIMESTAMP_EXT_LEN: usize = 4;
    pub const USER_TIMESTAMP_EXT_LEN: usize = 8;
    pub const E2EE_EXT_LEN: usize = E2EE_EXT_IV_LEN + 3 + E2EE_EXT_KEY_INDEX_LEN;

    // E2EE offsets and lengths
    pub const E2EE_EXT_IV_LEN: usize = 12;
    pub const E2EE_EXT_KEY_INDEX_LEN: usize = 1;
    pub const E2EE_EXT_KEY_INDEX_OFFSET: usize = 15;
}
