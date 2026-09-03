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

/// Returns the version specified in the crate's Cargo.toml.
///
/// This is the first function exposed over UniFFI from `livekit-ffi`. It carries
/// no server state and touches no handle store, so downstream SDKs can call it to
/// prove the UniFFI bindgen toolchain is wired up end-to-end.
#[uniffi::export]
pub fn build_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[cfg(test)]
mod tests {
    use super::build_version;

    #[test]
    fn build_version_matches_cargo_pkg_version() {
        assert_eq!(build_version(), env!("CARGO_PKG_VERSION"));
        assert!(!build_version().is_empty());
    }
}
