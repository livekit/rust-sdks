use std::{
    env,
    error::Error,
    fs::{self, File},
    io::{self, BufRead, Write},
    path,
    process::Command,
};

use fs2::FileExt;
use regex::Regex;
use reqwest::StatusCode;

pub const SCRATH_PATH: &str = "livekit_webrtc";
pub const WEBRTC_TAG: &str = "webrtc-4a9b827";
pub const IGNORE_DEFINES: [&str; 2] = ["CR_CLANG_REVISION", "CR_XCODE_VERSION"];

pub fn target_os() -> String {
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap();
    let target = env::var("TARGET").unwrap();
    let is_simulator = target.ends_with("-sim");

    match target_os.as_str() {
        "windows" => "win",
        "macos" => "mac",
        "ios" => {
            if is_simulator {
                "ios-simulator"
            } else {
                "ios-device"
            }
        }
        _ => &target_os,
    }
    .to_string()
}

pub fn target_arch() -> String {
    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap();
    match target_arch.as_str() {
        "aarch64" => "arm64",
        "x86_64" => "x64",
        _ => &target_arch,
    }
    .to_owned()
}

/// The full name of the webrtc library
/// e.g. webrtc-mac-x64-release (Same name on GH releases)
pub fn webrtc_triple() -> String {
    let profile = if use_debug() { "debug" } else { "release" };
    format!("webrtc-{}-{}-{}", target_os(), target_arch(), profile)
}

/// Using debug builds of webrtc is still experimental for now
/// On Windows, Rust doesn't link against libcmtd on debug, which is an issue
/// Default to false (even on cargo debug)
pub fn use_debug() -> bool {
    let var = env::var("LK_DEBUG_WEBRTC");
    var.is_ok() && var.unwrap() == "true"
}

/// The location of the custom build is defined by the user
pub fn custom_dir() -> Option<path::PathBuf> {
    if let Some(path) = env::var("LK_CUSTOM_WEBRTC").ok() {
        return Some(path::PathBuf::from(path));
    }
    None
}

/// Location of the downloaded webrtc binaries
/// The reason why we don't use OUT_DIR is because we sometimes need to share the same binaries across multiple crates
/// without dependencies constraints
/// This also has the benefit of not re-downloading the binaries for each crate
pub fn prebuilt_dir() -> path::PathBuf {
    let target_dir = scratch::path(SCRATH_PATH);
    path::Path::new(&target_dir).join(format!(
        "livekit/{}-{}/{}",
        webrtc_triple(),
        WEBRTC_TAG,
        webrtc_triple()
    ))
}

pub fn download_url() -> String {
    format!(
        "https://github.com/livekit/client-sdk-rust/releases/download/{}/{}.zip",
        WEBRTC_TAG,
        webrtc_triple()
    )
}

/// Used location of libwebrtc depending on whether it's a custom build or not
pub fn webrtc_dir() -> path::PathBuf {
    if let Some(path) = custom_dir() {
        return path;
    }

    prebuilt_dir()
}

pub fn webrtc_defines() -> Vec<(String, Option<String>)> {
    // read preprocessor definitions from webrtc.ninja
    let defines_re = Regex::new(r"-D(\w+)(?:=([^\s]+))?").unwrap();
    let webrtc_gni = fs::File::open(webrtc_dir().join("webrtc.ninja")).unwrap();

    let mut defines_line = String::default();
    io::BufReader::new(webrtc_gni)
        .read_line(&mut defines_line)
        .unwrap();

    let mut vec = Vec::default();
    for cap in defines_re.captures_iter(&defines_line) {
        let define_name = &cap[1];
        let define_value = cap.get(2).map(|m| m.as_str());
        if IGNORE_DEFINES.contains(&define_name) {
            continue;
        }

        vec.push((define_name.to_owned(), define_value.map(str::to_string)));
    }

    vec
}

pub fn configure_jni_symbols() -> Result<(), Box<dyn Error>> {
    download_webrtc()?;

    let toolchain = android_ndk_toolchain()?;
    let toolchain_bin = toolchain.join("bin");

    let webrtc_dir = webrtc_dir();
    let webrtc_lib = webrtc_dir.join("lib");

    let out_dir = path::PathBuf::from(env::var("OUT_DIR").unwrap());

    // Find JNI symbols
    let readelf_output = Command::new(toolchain_bin.join("llvm-readelf"))
        .arg("-Ws")
        .arg(webrtc_lib.join("libwebrtc.a"))
        .output()
        .expect("failed to run llvm-readelf");

    let jni_regex = Regex::new(r"(Java_org_webrtc.*)").unwrap();
    let content = String::from_utf8_lossy(&readelf_output.stdout);
    let jni_symbols: Vec<&str> = jni_regex
        .captures_iter(&content)
        .into_iter()
        .map(|cap| cap.get(1).unwrap().as_str())
        .collect();

    if jni_symbols.is_empty() {
        return Err("No JNI symbols found".into()); // Shouldn't happen
    }

    // Keep JNI symbols
    for symbol in &jni_symbols {
        println!("cargo:rustc-link-arg=-Wl,--undefined={}", symbol);
    }

    // Version script
    let vs_path = out_dir.join("webrtc_jni.map");
    let mut vs_file = fs::File::create(&vs_path).unwrap();

    let jni_symbols = jni_symbols.join("; ");
    write!(vs_file, "JNI_WEBRTC {{\n\tglobal: {}; \n}};", jni_symbols).unwrap();

    println!(
        "cargo:rustc-link-arg=-Wl,--version-script={}",
        vs_path.display()
    );

    Ok(())
}

pub fn download_webrtc() -> Result<(), Box<dyn Error>> {
    let dir = scratch::path(SCRATH_PATH);
    let flock = File::create(dir.join(".lock"))?;
    flock.lock_exclusive()?;

    let webrtc_dir = webrtc_dir();
    if webrtc_dir.exists() {
        return Ok(());
    }

    let mut resp = reqwest::blocking::get(download_url())?;
    if resp.status() != StatusCode::OK {
        return Err(format!("failed to download webrtc: {}", resp.status()).into());
    }

    let tmp_dir = env::var("OUT_DIR").unwrap() + "/webrtc.zip";
    let tmp_dir = path::Path::new(&tmp_dir);

    {
        let file = fs::File::create(&tmp_dir)?;
        let mut writer = io::BufWriter::new(file);
        io::copy(&mut resp, &mut writer)?;
    }

    let file = fs::File::open(&tmp_dir)?;

    let mut archive = zip::ZipArchive::new(file)?;
    archive.extract(&webrtc_dir.parent().unwrap())?;

    Ok(())
}

pub fn android_ndk_toolchain() -> Result<path::PathBuf, &'static str> {
    let ndk_env = env::var("ANDROID_NDK_HOME");

    if ndk_env.is_err() {
        return Err("ANDROID_NDK_HOME is not set, please set it to the path of your Android NDK");
    }

    let android_ndk = path::PathBuf::from(ndk_env.unwrap());
    let host_os = if cfg!(linux) {
        "linux-x86_64"
    } else if cfg!(target_os = "macos") {
        "darwin-x86_64"
    } else if cfg!(windows) {
        "windows-x86_64"
    } else {
        return Err("Unsupported host OS");
    };

    Ok(android_ndk.join(format!("toolchains/llvm/prebuilt/{}", host_os)))
}
