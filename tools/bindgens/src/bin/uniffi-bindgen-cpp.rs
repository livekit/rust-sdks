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

//! The pinned UniFFI C++ bindgen CLI used by LiveKit SDK consumers.
//!
//! The C++ generator is a binary-only crate, so this workspace command installs
//! the pinned revision into the workspace target directory and forwards all
//! arguments to it. Generate C++ bindings from `livekit-ffi`, for example:
//! `cargo run -p bindgens --bin uniffi-bindgen-cpp -- \
//!     --library target/debug/liblivekit_ffi.dylib --out-dir <dir>`

use std::{
    env,
    path::{Path, PathBuf},
    process::{Command, ExitCode},
};

const REPOSITORY: &str = "https://github.com/alan-george-lk/uniffi-bindgen-cpp.git";
const REVISION: &str = "7d54c1c56e0e4c46772ef4c2b5be3cb26396cbdb";

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("uniffi-bindgen-cpp: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<ExitCode, String> {
    let install_root = install_root();
    let executable =
        install_root.join("bin").join(format!("uniffi-bindgen-cpp{}", env::consts::EXE_SUFFIX));

    if !executable.is_file() {
        install(&install_root)?;
    }

    let status = Command::new(&executable)
        .args(env::args_os().skip(1))
        .status()
        .map_err(|error| format!("failed to run {}: {error}", executable.display()))?;

    match status.code() {
        Some(code) if (0..=u8::MAX as i32).contains(&code) => Ok(ExitCode::from(code as u8)),
        Some(code) => Err(format!("generator exited with unsupported status {code}")),
        None => Err("generator terminated without an exit status".to_owned()),
    }
}

fn install_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("target")
        .join("uniffi-bindgen-cpp")
        .join(REVISION)
}

fn install(install_root: &Path) -> Result<(), String> {
    let cargo = env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let status = Command::new(cargo)
        .args(["install", "--locked", "--git", REPOSITORY, "--rev", REVISION, "--root"])
        .arg(install_root)
        .arg("uniffi-bindgen-cpp")
        .status()
        .map_err(|error| format!("failed to install pinned C++ generator: {error}"))?;

    if status.success() {
        Ok(())
    } else {
        Err(format!("installing pinned C++ generator failed with {status}"))
    }
}
