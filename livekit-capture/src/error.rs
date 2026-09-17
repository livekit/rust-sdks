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

//! The error type shared by capture sources.
//!
//! [`SourceError`] type-erases a source's own error type so that a pump can
//! report any source failure through
//! [`PumpError`](crate::pump::PumpError).

use std::error::Error;

type BoxError = Box<dyn Error + Send + Sync + 'static>;

#[cfg(feature = "source-pattern")]
pub use crate::renderer::RendererError;

/// Error returned by a capture source.
#[derive(Debug, thiserror::Error)]
#[error(transparent)]
pub struct SourceError(#[from] BoxError);

impl SourceError {
    pub fn new(error: impl Into<BoxError>) -> Self {
        Self(error.into())
    }
}
