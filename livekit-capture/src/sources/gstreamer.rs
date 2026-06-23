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

use std::error::Error;

use crate::encoded::{ingress::EncodedAccessUnitSource, OwnedEncodedAccessUnit};

/// Callback-backed encoded source for GStreamer appsink integrations.
#[derive(Debug)]
pub struct GStreamerAppSinkSource<F> {
    next_access_unit: F,
}

impl<F> GStreamerAppSinkSource<F> {
    /// Creates a source from a callback that pulls the next encoded appsink sample.
    pub fn new(next_access_unit: F) -> Self {
        Self { next_access_unit }
    }

    /// Returns the wrapped callback.
    pub fn callback(&self) -> &F {
        &self.next_access_unit
    }

    /// Returns the wrapped callback mutably.
    pub fn callback_mut(&mut self) -> &mut F {
        &mut self.next_access_unit
    }

    /// Consumes this source and returns the wrapped callback.
    pub fn into_callback(self) -> F {
        self.next_access_unit
    }
}

impl<F, E> EncodedAccessUnitSource for GStreamerAppSinkSource<F>
where
    F: FnMut() -> Result<Option<OwnedEncodedAccessUnit>, E>,
    E: Error + Send + Sync + 'static,
{
    type Error = E;

    fn next_access_unit(&mut self) -> Result<Option<OwnedEncodedAccessUnit>, Self::Error> {
        (self.next_access_unit)()
    }
}
