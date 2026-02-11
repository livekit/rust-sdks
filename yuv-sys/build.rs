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
use std::path::{Path, PathBuf};
use std::{env, fs, io};

const FNC_PREFIX: &str = "rs_";

// Architecture-specific source files compiled as separate units with special
// compiler flags. Matches the compilation units in libyuv's CMakeLists.txt.

const NEON_FILES: &[&str] = &["compare_neon.cc", "rotate_neon.cc", "row_neon.cc", "scale_neon.cc"];

const NEON64_FILES: &[&str] =
    &["compare_neon64.cc", "rotate_neon64.cc", "row_neon64.cc", "scale_neon64.cc"];

const SVE_FILES: &[&str] = &["row_sve.cc"];

const SME_FILES: &[&str] = &["rotate_sme.cc", "row_sme.cc", "scale_sme.cc"];

/// Prefix public API symbols with FNC_PREFIX to avoid conflicts with other
/// statically linked libyuv instances (e.g. libwebrtc).
fn rename_symbols(
    fnc_list: &[&str],
    include_files: &[fs::DirEntry],
    source_files: &[fs::DirEntry],
) {
    include_files.par_iter().chain(source_files).for_each(|file| {
        let mut content = fs::read_to_string(&file.path()).unwrap();
        for line in fnc_list {
            let fnc = line.trim();
            if fnc.is_empty() {
                continue;
            }

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
        return false;
    }

    if let Err(err) = copy_dir("libyuv", libyuv_dir) {
        fs::remove_dir_all(&libyuv_dir).unwrap();
        panic!("failed to copy libyuv: {:?}", err);
    }

    true
}

fn can_compile_sme(out_dir: &Path) -> bool {
    let test_file = out_dir.join("sme_check.c");
    fs::write(&test_file, "__arm_locally_streaming void func(void) { }\n").unwrap();

    cc::Build::new()
        .warnings(false)
        .flag("-march=armv9-a+i8mm+sme")
        .file(&test_file)
        .try_compile("sme_check")
        .is_ok()
}

fn can_compile_sve(out_dir: &Path) -> bool {
    let test_file = out_dir.join("sve_check.c");
    fs::write(&test_file, "void func(void) { asm volatile(\"cnth x0\"); }\n").unwrap();

    cc::Build::new()
        .warnings(false)
        .flag("-march=armv8.5-a+i8mm+sve2")
        .file(&test_file)
        .try_compile("sve_check")
        .is_ok()
}

fn new_build(libyuv_dir: &Path) -> cc::Build {
    let mut build = cc::Build::new();
    build.warnings(false).include(libyuv_dir.join("include"));
    build
}

fn main() {
    let output_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let libyuv_dir = output_dir.join("libyuv");
    let include_dir = libyuv_dir.join("include");
    let source_dir = libyuv_dir.join("source");

    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    let is_msvc = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default() == "msvc";
    let is_aarch64 = target_arch == "aarch64";
    let is_arm32 = target_arch == "arm";

    let cloned = clone_if_needed(&output_dir, &libyuv_dir);

    let include_files = fs::read_dir(include_dir.join("libyuv"))
        .unwrap()
        .map(Result::unwrap)
        .filter(|f| f.path().extension().unwrap() == "h")
        .collect::<Vec<_>>();

    let all_source_files = fs::read_dir(&source_dir)
        .unwrap()
        .map(Result::unwrap)
        .filter(|f| f.path().extension().unwrap() == "cc")
        .collect::<Vec<_>>();

    let fnc_content = fs::read_to_string("yuv_functions.txt").unwrap();
    let fnc_list = fnc_content.lines().collect::<Vec<_>>();

    if cloned {
        rename_symbols(&fnc_list, &include_files, &all_source_files);
    }

    // Check compiler support before compiling, since we need to define
    // LIBYUV_DISABLE_SVE/SME globally if unsupported.
    let sve_supported = is_aarch64 && !is_msvc && can_compile_sve(&output_dir);
    let sme_supported = is_aarch64 && !is_msvc && can_compile_sme(&output_dir);

    let all_arch_files: Vec<&str> = [NEON_FILES, NEON64_FILES, SVE_FILES, SME_FILES].concat();

    let common_files: Vec<PathBuf> = all_source_files
        .iter()
        .filter(|f| {
            let name = f.file_name().to_string_lossy().to_string();
            !all_arch_files.contains(&name.as_str())
        })
        .map(|f| f.path())
        .collect();

    let mut common_build = new_build(&libyuv_dir);
    common_build.files(&common_files);

    if is_arm32 && !is_msvc {
        common_build.define("LIBYUV_NEON", "1");
    }

    if is_aarch64 && !is_msvc {
        if !sme_supported {
            common_build.define("LIBYUV_DISABLE_SME", None);
        }
        if !sve_supported {
            common_build.define("LIBYUV_DISABLE_SVE", None);
        }
    }

    #[cfg(feature = "jpeg")]
    {
        let jpeg_pkg = pkg_config::Config::new()
            .probe("libjpeg")
            .or_else(|_| pkg_config::Config::new().probe("libjpeg-turbo"))
            .or_else(|_| pkg_config::Config::new().probe("jpeg"))
            .ok();

        if let Some(pkg) = &jpeg_pkg {
            common_build.define("HAVE_JPEG", None);
            for p in &pkg.include_paths {
                common_build.include(p);
            }
        }
    }

    common_build.compile("yuv");

    if !is_msvc {
        if is_arm32 {
            new_build(&libyuv_dir)
                .define("LIBYUV_NEON", "1")
                .flag("-mfpu=neon")
                .files(NEON_FILES.iter().map(|f| source_dir.join(f)))
                .compile("yuv_neon");
        } else if is_aarch64 {
            new_build(&libyuv_dir)
                .flag("-march=armv8.2-a+dotprod+i8mm")
                .files(NEON64_FILES.iter().map(|f| source_dir.join(f)))
                .compile("yuv_neon64");

            if sve_supported {
                let sve_files: Vec<PathBuf> = SVE_FILES
                    .iter()
                    .map(|f| source_dir.join(f))
                    .filter(|p| p.exists())
                    .collect();
                if !sve_files.is_empty() {
                    new_build(&libyuv_dir)
                        .flag("-march=armv8.5-a+i8mm+sve2")
                        .files(&sve_files)
                        .compile("yuv_sve");
                }
            }

            if sme_supported {
                let sme_files: Vec<PathBuf> = SME_FILES
                    .iter()
                    .map(|f| source_dir.join(f))
                    .filter(|p| p.exists())
                    .collect();
                if !sme_files.is_empty() {
                    new_build(&libyuv_dir)
                        .flag("-march=armv9-a+i8mm+sme")
                        .files(&sme_files)
                        .compile("yuv_sme");
                }
            }
        }
    }

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
