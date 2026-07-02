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

mod transport;
mod types;

#[cfg(feature = "__native")]
mod native;

pub use transport::{PlatformConnectResult, PlatformConnection, PlatformTransport};
pub use types::{Header, HttpResponse, TransportError};

use std::sync::{Arc, OnceLock};

static REGISTERED: OnceLock<Arc<dyn PlatformTransport>> = OnceLock::new();

/// Register the process-wide transport. Call once at startup, before the first
/// `connect`. A later call is ignored (first registration wins).
pub fn set_transport(t: Arc<dyn PlatformTransport>) {
    let _ = REGISTERED.set(t);
}

/// Resolve the process-wide transport.
///
/// Returns the explicitly registered transport if any; otherwise, on native
/// builds, the built-in [`NativeTransport`]; otherwise `None`.
pub fn transport() -> Option<Arc<dyn PlatformTransport>> {
    if let Some(t) = REGISTERED.get() {
        return Some(Arc::clone(t));
    }
    #[cfg(feature = "__native")]
    {
        Some(native::native_default())
    }
    #[cfg(not(feature = "__native"))]
    {
        None
    }
}

#[cfg(feature = "foreign")]
uniffi::setup_scaffolding!();

#[cfg(feature = "__native")]
pub mod testing {
    use crate::PlatformTransport;
    use std::sync::Arc;
    /// Construct a fresh NativeTransport for tests (bypasses the global registry).
    pub fn native_transport() -> Arc<dyn PlatformTransport> {
        Arc::new(crate::native::NativeTransport)
    }
}
