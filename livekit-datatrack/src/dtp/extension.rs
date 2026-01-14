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

use core::fmt;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(test, derive(fake::Dummy))]
pub struct Extensions {
    pub user_timestamp: Option<UserTimestampExt>,
    pub e2ee: Option<E2eeExt>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(test, derive(fake::Dummy))]
pub struct UserTimestampExt(pub u64);

#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(test, derive(fake::Dummy))]
pub struct E2eeExt {
    pub key_index: u8,
    pub iv: [u8; 12],
}

impl fmt::Debug for E2eeExt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // For security, do not include fields in debug.
        f.debug_struct("E2ee").finish()
    }
}

pub(super) type ExtensionTag = u16;

impl UserTimestampExt {
    pub(super) const TAG: ExtensionTag = 2;
    pub(super) const LEN: usize = 8;
}

impl E2eeExt {
    pub(super) const TAG: ExtensionTag = 1;
    pub(super) const LEN: usize = 13;
}