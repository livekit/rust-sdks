use regex::Regex;
use std::env;
use std::fs;
use std::io::Write;
use std::path;

fn main() {
    let target_os = "android";
    //let target_arch = "arm64";

    let libwebrtc_dir = path::PathBuf::from("libwebrtc");

    // Just required for the bridge build to succeed.
    let includes = &[
        path::PathBuf::from("./include"),
        libwebrtc_dir.join("include/"),
        libwebrtc_dir.join("include/third_party/abseil-cpp/"),
        libwebrtc_dir.join("include/third_party/libc++/"),
    ];

    let mut builder = cxx_build::bridges(&["src/lib.rs"]);
    builder.flag("-std=c++17");
    builder.file("src/peer_connection_factory.cpp");
    builder.file("src/jni_onload.cc");

    for include in includes {
        builder.include(include);
    }

    println!(
        "cargo:rustc-link-search=native={}",
        libwebrtc_dir.canonicalize().unwrap().to_str().unwrap()
    );
    println!("cargo:rustc-link-lib=static=webrtc");

    match target_os {
        "macos" => {
            println!("cargo:rustc-link-lib=dylib=c++");
            println!("cargo:rustc-link-lib=framework=Foundation");
            println!("cargo:rustc-link-lib=framework=AVFoundation");
            println!("cargo:rustc-link-lib=framework=CoreAudio");
            println!("cargo:rustc-link-lib=framework=AudioToolbox");
            println!("cargo:rustc-link-lib=framework=Appkit");
            println!("cargo:rustc-link-lib=framework=CoreMedia");
            println!("cargo:rustc-link-lib=framework=CoreGraphics");

            builder
                .flag("-stdlib=libc++")
                .define("WEBRTC_ENABLE_OBJC_SYMBOL_EXPORT", None)
                .define("WEBRTC_POSIX", None)
                .define("WEBRTC_MAC", None);
        }
        "ios" => {
            // TODO(theomonnom)
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
                toolchain
                    .join("lib")
                    .to_str()
                    .unwrap()
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
            jni_regex.captures_iter(&content).for_each(|cap| {
                jni_symbols.push(cap.get(1).unwrap().as_str());
            });

            // JNI Version Script & Keep JNI symbols
            let vs_path = path::PathBuf::from(env::var("OUT_DIR").unwrap()).join("webrtc_jni.map");
            let mut vs_file = fs::File::create(&vs_path).unwrap();
            println!("cargo:rustc-link-arg=-Wl,--undefined=JNI_OnLoad");

            write!(vs_file, "JNI_WEBRTC {{\n\tglobal: ").unwrap();
            write!(vs_file, "JNI_OnLoad; ").unwrap();
            for x in &jni_symbols {
                println!("cargo:rustc-link-arg=-Wl,--undefined={}", x);
                write!(vs_file, "{}; ", x).unwrap();
            }
            write!(vs_file, "\n}};").unwrap();

            println!(
                "cargo:rustc-link-arg=-Wl,--version-script={}",
                vs_path.to_str().unwrap()
            );

            builder
                .define("WEBRTC_LINUX", None)
                .define("WEBRTC_POSIX", None)
                .define("WEBRTC_ANDROID", None);
        }
        _ => {
            panic!("Unsupported platform, {}", target_os);
        }
    }

    builder.warnings(false).compile("lkwebrtc");

    for entry in glob::glob("./src/**/*.cpp").unwrap() {
        println!(
            "cargo:rerun-if-changed={}",
            entry.unwrap().display().to_string()
        );
    }

    for entry in glob::glob("./include/**/*.h").unwrap() {
        println!(
            "cargo:rerun-if-changed={}",
            entry.unwrap().display().to_string()
        );
    }

    println!("cargo:rerun-if-changed=src/main.rs");
}
