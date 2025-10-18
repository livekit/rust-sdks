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

use cc;
use rayon::prelude::*;
use regex::Regex;
use std::borrow::Cow;
use std::path::Path;
use std::{env, path::PathBuf};
use std::{fs, io};

//const LIBYUV_REPO: &str = "https://chromium.googlesource.com/libyuv/libyuv";
//const LIBYUV_COMMIT: &str = "af6ac82";
const FNC_PREFIX: &str = "rs_";

/*fn run_git_cmd(current_dir: &PathBuf, args: &[&str]) -> ExitStatus {
    Command::new("git")
        .current_dir(current_dir)
        .args(args)
        .status()
        .unwrap()
}*/

fn rename_symbols(
    fnc_list: &[&str],
    include_files: &[fs::DirEntry],
    source_files: &[fs::DirEntry],
) {
    // Find all occurences of the function in every header and source files
    // and prefix it with FNC_PREFIX
    include_files.par_iter().chain(source_files).for_each(|file| {
        let mut content = fs::read_to_string(&file.path()).unwrap();
        for line in fnc_list {
            let fnc = line.trim();
            if fnc.is_empty() {
                continue;
            }

            // Split line using space as delimiter (If there is two words, the second word is the new name instead of using prefix)
            let split: Vec<&str> = fnc.split_whitespace().collect();
            let fnc = split[0];

            let new_name = if split.len() > 1 {
                split[1].to_owned()
            } else {
                format!("{}{}", FNC_PREFIX, fnc)
            };

            let re = Regex::new(&format!(r"\b{}\b", fnc)).unwrap();
            if let Cow::Owned(c) = re.replace_all(&content, &new_name) {
                content = c
            }
        }

        fs::write(&file.path(), content.to_string()).unwrap();
    });
}

fn copy_dir(source: impl AsRef<Path>, destination: impl AsRef<Path>) -> io::Result<()> {
    fs::create_dir_all(&destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            copy_dir(entry.path(), destination.as_ref().join(entry.file_name()))?;
        } else {
            fs::copy(entry.path(), destination.as_ref().join(entry.file_name()))?;
        }
    }
    Ok(())
}

fn clone_if_needed(_output_dir: &PathBuf, libyuv_dir: &PathBuf) -> bool {
    if libyuv_dir.exists() {
        return false; // Already cloned
    }

    /*let status = run_git_cmd(output_dir, &["clone", LIBYUV_REPO]);
    if !status.success() {
        fs::remove_dir_all(&libyuv_dir).unwrap();
        panic!("failed to clone libyuv, is git installed?");
    }

    let status = run_git_cmd(&libyuv_dir, &["checkout", LIBYUV_COMMIT]);
    if !status.success() {
        fs::remove_dir_all(&libyuv_dir).unwrap();
        panic!("failed to checkout to {}", LIBYUV_COMMIT);
    }*/

    if let Err(err) = copy_dir("libyuv", libyuv_dir) {
        fs::remove_dir_all(&libyuv_dir).unwrap();
        panic!("failed to copy libyuv: {:?}", err);
    }

    true
}

fn main() {
    let output_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let libyuv_dir = output_dir.join("libyuv");
    let include_dir = libyuv_dir.join("include");
    let source_dir = libyuv_dir.join("source");

    let cloned = clone_if_needed(&output_dir, &libyuv_dir);

    let include_files = fs::read_dir(include_dir.join("libyuv"))
        .unwrap()
        .map(Result::unwrap)
        .filter(|f| f.path().extension().unwrap() == "h")
        .collect::<Vec<_>>();

    let source_files = fs::read_dir(source_dir)
        .unwrap()
        .map(Result::unwrap)
        .filter(|f| f.path().extension().unwrap() == "cc")
        .collect::<Vec<_>>();

    let fnc_content = fs::read_to_string("yuv_functions.txt").unwrap();
    let fnc_list = fnc_content.lines().collect::<Vec<_>>();

    if cloned {
        // Rename symbols to avoid conflicts with other libraries
        // that have libyuv statically linked (e.g libwebrtc).
        rename_symbols(&fnc_list, &include_files, &source_files);
    }

    cc::Build::new()
        .warnings(false)
        .include(libyuv_dir.join("include"))
        .files(source_files.iter().map(|f| f.path()))
        .compile("yuv");

    let mut bindgen = bindgen::Builder::default()
        .header(include_dir.join("libyuv.h").to_string_lossy())
        .clang_arg(format!("-I{}", include_dir.to_str().unwrap()));

    for fnc in fnc_list {
        let new_name = format!("{}{}", FNC_PREFIX, fnc);
        bindgen = bindgen.allowlist_function(&new_name);
    }

    let output = bindgen.generate().unwrap();
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap()).join("yuv.rs");
    output.write_to_file(out_path).unwrap();

    println!("cargo:rerun-if-changed=yuv_functions.txt");
}
