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

//! The uniffi-bindgen-react-native CLI, shared across the workspace.
//!
//! Generates NAPI TypeScript bindings from any compiled uniffi cdylib, e.g.:
//! `cargo run -p bindgens --bin uniffi-bindgen-react-native -- generate napi bindings \
//!     --library --lib-colocated --crate livekit_uniffi \
//!     --ts-dir <dir> target/release/liblivekit_uniffi.dylib`
//!
//! Mirrors ubrn's own `crates/ubrn_cli/src/main.rs`. The crate is depended on with
//! `default-features = false`; see this crate's Cargo.toml for why that matters.
use clap::Parser;
use ubrn_cli::{cli, Result};

fn main() -> Result<()> {
    let args = cli::CliArgs::parse();
    args.run()
}
