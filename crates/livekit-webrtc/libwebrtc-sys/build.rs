use regex::Regex;
use std::env;
use std::fs;
use std::io::Write;
use std::path;
use std::process::Command;

const MAC_SDKS: &str =
    "/Applications/Xcode.app/Contents/Developer/Platforms/MacOSX.platform/Developer/SDKs";

fn get_mac_sysroot() -> String {
    let mut sdks: Vec<String> = vec![];
    let files = fs::read_dir(MAC_SDKS).unwrap();
    for entry in files {
        let entry = entry.unwrap();
        let filename = entry.file_name().to_str().unwrap().to_owned();
        sdks.push(filename);
    }

    sdks = sdks
        .iter()
        .filter(|value| value.contains("MacOSX1"))
        .map(|original| original.to_owned())
        .collect();

    let last = sdks.last().unwrap();

    format!("{}/{}", MAC_SDKS, &last)
}

fn macos_link_search_path() -> Option<String> {
    let output = Command::new("clang")
        .arg("--print-search-dirs")
        .output()
        .ok()?;
    if !output.status.success() {
        // Failed to run 'clang --print-search-dirs'.
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if line.contains("libraries: =") {
            let path = line.split('=').nth(1)?;
            return Some(format!("{}/lib/darwin", path));
        }
    }

    // Failed to determine link search path.
    None
}

fn main() {
    // TODO Download precompiled binaries of WebRTC for the target_os
    let target_os = "macos";
    //let target_arch = "arm64";

    let libwebrtc_dir = path::PathBuf::from("libwebrtc");

    // Just required for the bridge build to succeed.
    let includes = &[
        path::PathBuf::from("./include"),
        libwebrtc_dir.join("include/"),
        libwebrtc_dir.join("include/third_party/abseil-cpp/"),
        libwebrtc_dir.join("include/third_party/libc++/"),
        // For mac & ios
        libwebrtc_dir.join("include/sdk/objc"),
        libwebrtc_dir.join("include/sdk/objc/base"),
    ];

    let mut builder = cxx_build::bridges(&[
        "src/peer_connection.rs",
        "src/peer_connection_factory.rs",
        "src/media_stream.rs",
        "src/data_channel.rs",
        "src/jsep.rs",
        "src/candidate.rs",
        "src/rtp_receiver.rs",
        "src/rtp_transceiver.rs",
        "src/rtc_error.rs",
        "src/webrtc.rs",
    ]);

    builder.file("src/peer_connection.cpp");
    builder.file("src/peer_connection_factory.cpp");
    builder.file("src/media_stream.cpp");
    builder.file("src/data_channel.cpp");
    builder.file("src/jsep.cpp");
    builder.file("src/candidate.cpp");
    builder.file("src/rtp_receiver.cpp");
    builder.file("src/rtp_transceiver.cpp");
    builder.file("src/rtc_error.cpp");
    builder.file("src/webrtc.cpp");

    for include in includes {
        builder.include(include);
    }

    println!(
        "cargo:rustc-link-search=native={}",
        libwebrtc_dir.canonicalize().unwrap().to_str().unwrap()
    );

    match target_os {
        "windows" => {
            println!("cargo:rustc-link-lib=dylib=msdmo");
            println!("cargo:rustc-link-lib=dylib=wmcodecdspuuid");
            println!("cargo:rustc-link-lib=dylib=dmoguids");
            println!("cargo:rustc-link-lib=dylib=crypt32");
            println!("cargo:rustc-link-lib=dylib=iphlpapi");
            println!("cargo:rustc-link-lib=dylib=ole32");
            println!("cargo:rustc-link-lib=dylib=secur32");
            println!("cargo:rustc-link-lib=dylib=winmm");
            println!("cargo:rustc-link-lib=dylib=ws2_32");
            println!("cargo:rustc-link-lib=dylib=strmiids");
            println!("cargo:rustc-link-lib=dylib=d3d11");
            println!("cargo:rustc-link-lib=dylib=dxgi");
            println!("cargo:rustc-link-lib=dylib=dwmapi");
            println!("cargo:rustc-link-lib=dylib=webrtc");

            builder
                .flag("/std:c++17")
                .flag("/EHsc")
                .define("WEBRTC_WIN", None)
                .define("NOMINMAX", None);
        }
        "macos" => {
            println!("cargo:rustc-link-lib=dylib=c++");
            println!("cargo:rustc-link-lib=framework=Foundation");
            println!("cargo:rustc-link-lib=framework=AVFoundation");
            println!("cargo:rustc-link-lib=framework=CoreAudio");
            println!("cargo:rustc-link-lib=framework=AudioToolbox");
            println!("cargo:rustc-link-lib=framework=Appkit");
            println!("cargo:rustc-link-lib=framework=CoreMedia");
            println!("cargo:rustc-link-lib=framework=CoreGraphics");
            println!("cargo:rustc-link-lib=framework=VideoToolbox");
            println!("cargo:rustc-link-lib=framework=CoreVideo");
            println!("cargo:rustc-link-lib=framework=OpenGL");
            println!("cargo:rustc-link-lib=framework=Metal");
            println!("cargo:rustc-link-lib=framework=QuartzCore");
            println!("cargo:rustc-link-lib=framework=IOKit");
            println!("cargo:rustc-link-lib=framework=IOSurface");
            println!("cargo:rustc-link-lib=static=webrtc");

            builder
                .flag("-stdlib=libc++")
                .flag("-std=c++17")
                .flag(format!("-isysroot{}", get_mac_sysroot()).as_str())
                .define("WEBRTC_ENABLE_OBJC_SYMBOL_EXPORT", None)
                .define("WEBRTC_POSIX", None)
                .define("WEBRTC_MAC", None);

            if let Some(path) = macos_link_search_path() {
                println!("cargo:rustc-link-lib=clang_rt.osx");
                println!("cargo:rustc-link-search={}", path);
            }
        }
        "ios" => {
            builder
                .flag("-std=c++17")
                .file("src/objc_test.mm")
                .define("WEBRTC_ENABLE_OBJC_SYMBOL_EXPORT", None)
                .define("WEBRTC_MAC", None)
                .define("WEBRTC_POSIX", None)
                .define("WEBRTC_IOS", None);
        }
        "android" => {
            let ndk_env = env::var("ANDROID_NDK_HOME").expect(
                "ANDROID_NDK_HOME is not set, please set it to the path of your Android NDK",
            );
            let android_ndk = path::PathBuf::from(ndk_env);

            let host_os = if cfg!(target_os = "linux") {
                "linux-x86_64"
            } else if cfg!(target_os = "macos") {
                "darwin-x86_64"
            } else if cfg!(target_os = "windows") {
                "windows-x86_64"
            } else {
                panic!("Unsupported host OS");
            };

            let toolchain = android_ndk.join(std::format!("toolchains/llvm/prebuilt/{}", host_os));

            // libgcc ( redirects to libunwind )
            println!(
                "cargo:rustc-link-search={}",
                toolchain.join("lib").to_str().unwrap()
            );

            // Needed when loading the library in the JVM ( System.loadLibrary )
            println!("cargo:rustc-link-lib=egl");
            println!("cargo:rustc-link-lib=OpenSLES");

            // Find JNI symbols
            let readelf_output = std::process::Command::new(toolchain.join("bin/llvm-readelf"))
                .arg("-Ws")
                .arg(libwebrtc_dir.join("libwebrtc.a"))
                .output()
                .expect("failed to run llvm-readelf");

            // Get all JNI symbols
            let jni_regex = Regex::new(r"(Java_org_webrtc.*)").unwrap();
            let content = &String::from_utf8_lossy(&readelf_output.stdout);
            let mut jni_symbols = Vec::new();
            jni_regex.captures_iter(content).for_each(|cap| {
                jni_symbols.push(cap.get(1).unwrap().as_str());
            });

            // JNI Version Script & Keep JNI symbols
            let vs_path = path::PathBuf::from(env::var("OUT_DIR").unwrap()).join("webrtc_jni.map");
            let mut vs_file = fs::File::create(&vs_path).unwrap();
            builder.file("src/jni_onload.cc");
            println!("cargo:rustc-link-arg=-Wl,--undefined=JNI_OnLoad");

            write!(vs_file, "JNI_WEBRTC {{\n\tglobal: ").unwrap();
            write!(vs_file, "JNI_OnLoad; ").unwrap();
            for x in jni_symbols {
                println!("cargo:rustc-link-arg=-Wl,--undefined={}", x);
                write!(vs_file, "{}; ", x).unwrap();
            }
            write!(vs_file, "\n}};").unwrap();

            println!(
                "cargo:rustc-link-arg=-Wl,--version-script={}",
                vs_path.to_str().unwrap()
            );

            builder
                .flag("-std=c++17")
                .define("WEBRTC_LINUX", None)
                .define("WEBRTC_POSIX", None)
                .define("WEBRTC_ANDROID", None);
        }
        _ => {
            panic!("Unsupported target, {}", target_os);
        }
    }

    // TODO(theomonnom) Only add this define when building tests
    builder.define("LIVEKIT_TEST", None);

    builder.warnings(false).compile("lkwebrtc");

    for entry in glob::glob("./src/**/*.cpp").unwrap() {
        println!("cargo:rerun-if-changed={}", entry.unwrap().display());
    }

    for entry in glob::glob("./include/**/*.h").unwrap() {
        println!("cargo:rerun-if-changed={}", entry.unwrap().display());
    }
}
