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

//! Dart bindings generator for the UniFFI interface.
//!
//! [_uniffi-dart_](https://github.com/Uniffi-Dart/uniffi-dart) ships no working
//! CLI of its own, so this thin binary drives its library API in
//! [library mode][lm] — reading UniFFI metadata directly from a compiled
//! `cdylib`, exactly as the Kotlin/Swift/Node tasks do via the stock
//! `uniffi-bindgen`.
//!
//! Usage: `uniffi-bindgen-dart --library <cdylib> --out-dir <dir> [--config <uniffi.toml>]`
//!
//! [lm]: https://mozilla.github.io/uniffi-rs/latest/bindings.html

use camino::Utf8PathBuf;

fn main() {
    let mut library: Option<Utf8PathBuf> = None;
    let mut out_dir: Option<Utf8PathBuf> = None;
    let mut config: Option<Utf8PathBuf> = None;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut take = |name: &str| -> Utf8PathBuf {
            args.next().unwrap_or_else(|| panic!("{name} requires a value")).into()
        };
        match arg.as_str() {
            "--library" => library = Some(take("--library")),
            "--out-dir" => out_dir = Some(take("--out-dir")),
            "--config" => config = Some(take("--config")),
            // Tolerate a leading `generate` subcommand for symmetry with `uniffi-bindgen`.
            "generate" => {}
            other => panic!("unrecognized argument: {other}"),
        }
    }

    let library = library.expect("--library <cdylib> is required");
    let out_dir = out_dir.expect("--out-dir <dir> is required");
    // The crate's `uniffi.toml` carries any `[bindings.dart]` config; default to
    // it so a bare invocation from the crate root picks it up.
    let config = config.unwrap_or_else(|| Utf8PathBuf::from("uniffi.toml"));

    uniffi_dart::gen::generate_dart_bindings(
        &config,        // udl_file: only consulted to locate config in library mode
        None,           // config_file_override
        Some(&out_dir), // out_dir_override
        &library,       // compiled cdylib to read metadata from
        true,           // library_mode
    )
    .expect("failed to generate Dart bindings");

    println!("Generated Dart bindings to {out_dir}");
}
