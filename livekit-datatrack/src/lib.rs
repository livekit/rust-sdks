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

/// Public APIs re-exported by the LiveKit crate.
pub mod api {
    pub use crate::{error::*, frame::*, local::*, remote::*, track::*};
}

/// Internal APIs for use within the LiveKit crate.
pub mod internal {
    pub use crate::e2ee::*;
    pub mod local {
        pub use crate::local::{manager::*, proto::*};
    }
    pub mod remote {
        pub use crate::remote::{manager::*, proto::*};
    }
}
