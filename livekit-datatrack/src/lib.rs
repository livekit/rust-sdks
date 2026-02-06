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

#![doc = include_str!("../README.md")]

/// Common types for local and remote tracks.
mod track;

/// Local track publication.
mod local;

/// Remote track subscription.
mod remote;

/// Application-level frame.
mod frame;

/// Provider for end-to-end encryption/decryption.
mod e2ee;

/// Data track packet (DTP) format.
mod packet;

/// Internal utilities.
mod utils;

/// Internal error.
mod error;

/// Public APIs re-exported by client SDKs.
pub mod api {
    pub use crate::{error::*, frame::*, local::*, remote::*, track::*};
}

/// Internal APIs used within client SDKs to power data tracks functionality.
pub mod backend {
    pub use crate::e2ee::*;

    /// Local track publication
    pub mod local {
        pub use crate::local::{events::*, manager::*, proto::*};
    }

    /// Remote track subscription
    pub mod remote {
        pub use crate::remote::{events::*, manager::*, proto::*};
    }
}
