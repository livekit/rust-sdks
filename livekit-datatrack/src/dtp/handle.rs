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

use std::fmt::Display;
use thiserror::Error;

/// Value identifying which data track a packet belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Handle(u16);

#[derive(Debug, Error)]
pub enum HandleError {
    #[error("{0:#X} is reserved")]
    Reserved(u16),

    #[error("value too large to be a valid track handle")]
    TooLarge,
}

impl TryFrom<u16> for Handle {
    type Error = HandleError;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        if value == 0 {
            Err(HandleError::Reserved(value))?
        }
        Ok(Self(value))
    }
}

impl TryFrom<u32> for Handle {
    type Error = HandleError;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        let value: u16 = value.try_into().map_err(|_| HandleError::TooLarge)?;
        value.try_into()
    }
}

impl From<Handle> for u16 {
    fn from(handle: Handle) -> Self {
        handle.0
    }
}

impl From<Handle> for u32 {
    fn from(handle: Handle) -> Self {
        handle.0 as u32
    }
}

impl Display for Handle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{0:#X}", self.0)
    }
}

/// Utility for allocating unique track handles to use for publishing tracks.
#[derive(Debug, Default)]
pub struct HandleAllocator {
    /// Next handle value.
    value: u16,
}

impl HandleAllocator {
    /// Returns a unique track handle for the next publication, if one can be obtained.
    pub fn get(&mut self) -> Option<Handle> {
        self.value = self.value.checked_add(1)?;
        Handle(self.value).into()
    }
}

#[cfg(test)]
impl fake::Dummy<fake::Faker> for Handle {
    fn dummy_with_rng<R: rand::Rng + ?Sized>(_: &fake::Faker, rng: &mut R) -> Self {
        Self::try_from(rng.random_range(1..u16::MAX)).unwrap()
    }
}