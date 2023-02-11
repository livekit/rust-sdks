use curl::easy::Easy;
use flate2::read::GzDecoder;
use regex::Regex;
use std::env;
use std::fs;
use std::io::{self, Write};
use std::path;
use std::process::Command;
use tar::Archive;

const WEBRTC_TAG: &str = "m104.5112.08";

fn download_prebuilt(
    target_os: &str,
    target_arch: &str,
    out_path: path::PathBuf,
) -> Result<path::PathBuf, Box<dyn std::error::Error>> {
    let target_arch = match target_arch {
        "aarch64" => "arm64",
        _ => target_arch,
    };

    let file_ext = if target_os == "windows" {
        "zip"
    } else {
        "tar.gz"
    };

    let file_name = format!("webrtc.{}_{}.{}", target_os, target_arch, file_ext);
    let file_url = format!(
        "https://github.com/webrtc-sdk/webrtc-build/releases/download/{}/{}",
        WEBRTC_TAG, file_name
    );
    let file_path = out_path.join(&file_name);

    if !out_path.exists() {
        fs::create_dir(&out_path)?;
    }

    // Download the release archive
    if !file_path.exists() {
        let file = fs::File::create(&file_path)?;
        {
            let mut writer = io::BufWriter::new(file);
            let mut handle = Easy::new();
            handle.url(&file_url)?;
            handle.follow_location(true)?;
            handle.write_function(move |data| Ok(writer.write(data).unwrap()))?;
            handle.perform()?;

            let response_code = handle.response_code()?;
            if response_code != 200 {
                fs::remove_file(&file_path)?;
                Err(format!(
                    "Failed to download WebRTC-SDK (Status: {}) {}",
                    response_code, file_url
                ))?
            }
        }

        // Extract the archive
        let file = fs::File::open(&file_path)?;
        if file_ext == "zip" {
            let mut archive = zip::ZipArchive::new(file)?;
            for i in 0..archive.len() {
                let mut inner_file = archive.by_index(i)?;
                let relative_path = inner_file.mangled_name();

                if relative_path.to_string_lossy().is_empty() {
                    continue; // Ignore root
                }

                let extracted_file = out_path.join(relative_path);
                if inner_file.name().ends_with('/') {
                    // Directory
                    fs::create_dir_all(&extracted_file)?;
                } else {
                    // File
                    if let Some(p) = extracted_file.parent() {
                        if !p.exists() {
                            fs::create_dir_all(&p)?;
                        }
                    }
                    let mut outfile = fs::File::create(&extracted_file)?;
                    io::copy(&mut inner_file, &mut outfile)?;
                }
            }
        } else if file_ext == "tar.gz" {
            let unzipped = GzDecoder::new(file);
            let mut a = Archive::new(unzipped);
            a.unpack(&out_path)?;
        }
    }

    Ok(out_path.join("webrtc"))
}

fn main() {
    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap();
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap();

    let use_custom_webrtc = {
        let var = env::var("LK_CUSTOM_WEBRTC");
        var.is_ok() && var.unwrap() == "true"
    };

    let (webrtc_include, webrtc_lib) = if use_custom_webrtc {
        // Use a local WebRTC version (libwebrtc folder)
        let webrtc_dir = path::PathBuf::from("./libwebrtc");
        (webrtc_dir.join("src"), webrtc_dir.join("src/out/Dev/obj"))
    } else {
        // Download a prebuilt version of WebRTC
        let download_dir = env::var("OUT_DIR").unwrap() + "/webrtc-sdk";
        let webrtc_dir =
            download_prebuilt(&target_os, &target_arch, path::PathBuf::from(download_dir)).unwrap();

        (webrtc_dir.join("include"), webrtc_dir.join("lib"))
    };
    println!("cargo:rerun-if-env-changed=LK_CUSTOM_WEBRTC");

    // Just required for the bridge build to succeed.
    let includes = &[
        path::PathBuf::from("./include"),
        webrtc_include.clone(),
        webrtc_include.join("third_party/abseil-cpp/"),
        webrtc_include.join("third_party/libyuv/include/"),
        webrtc_include.join("third_party/libc++/"),
        // For mac & ios
        webrtc_include.join("sdk/objc"),
        webrtc_include.join("sdk/objc/base"),
    ];

    let mut builder = cxx_build::bridges(&[
        "src/peer_connection.rs",
        "src/peer_connection_factory.rs",
        "src/media_stream.rs",
        "src/data_channel.rs",
        "src/jsep.rs",
        "src/candidate.rs",
        "src/rtp_parameters.rs",
        "src/rtp_sender.rs",
        "src/rtp_receiver.rs",
        "src/rtp_transceiver.rs",
        "src/rtc_error.rs",
        "src/webrtc.rs",
        "src/video_frame.rs",
        "src/video_frame_buffer.rs",
        "src/yuv_helper.rs",
        "src/helper.rs",
    ]);

    builder.file("src/peer_connection.cpp");
    builder.file("src/peer_connection_factory.cpp");
    builder.file("src/media_stream.cpp");
    builder.file("src/data_channel.cpp");
    builder.file("src/jsep.cpp");
    builder.file("src/candidate.cpp");
    builder.file("src/rtp_receiver.cpp");
    builder.file("src/rtp_sender.cpp");
    builder.file("src/rtp_transceiver.cpp");
    builder.file("src/rtp_parameters.cpp");
    builder.file("src/rtc_error.cpp");
    builder.file("src/webrtc.cpp");
    builder.file("src/video_frame.cpp");
    builder.file("src/video_frame_buffer.cpp");
    builder.file("src/video_encoder_factory.cpp");
    builder.file("src/video_decoder_factory.cpp");

    for include in includes {
        builder.include(include);
    }

    println!(
        "cargo:rustc-link-search=native={}",
        webrtc_lib.canonicalize().unwrap().to_str().unwrap()
    );

    match &target_os as &str {
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
            println!("cargo:rustc-link-lib=dylib=gdi32");
            println!("cargo:rustc-link-lib=dylib=dxgi");
            println!("cargo:rustc-link-lib=dylib=dwmapi");
            println!("cargo:rustc-link-lib=static=webrtc");

            builder
                .flag("/std:c++17")
                .flag("/EHsc")
                .define("WEBRTC_WIN", None)
                //.define("WEBRTC_ENABLE_SYMBOL_EXPORT", None) Not necessary when using WebRTC as a static library
                .define("NOMINMAX", None);
        }
        "macos" => {
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
            println!("cargo:rustc-link-lib=clang_rt.osx");
            println!("cargo:rustc-link-arg=-ObjC");

            let sysroot = Command::new("xcrun")
                .args(&["--sdk", "macosx", "--show-sdk-path"])
                .output()
                .unwrap();

            let sysroot = String::from_utf8_lossy(&sysroot.stdout);
            let sysroot = sysroot.trim();

            let search_dirs = Command::new("clang")
                .arg("--print-search-dirs")
                .output()
                .unwrap();

            let search_dirs = String::from_utf8_lossy(&search_dirs.stdout);
            for line in search_dirs.lines() {
                if line.contains("libraries: =") {
                    let path = line.split('=').nth(1).unwrap();
                    let path = format!("{}/lib/darwin", path);
                    println!("cargo:rustc-link-search={}", path);
                }
            }

            builder.file("src/objc_video_factory.mm");

            builder
                .flag("-stdlib=libc++")
                .flag("-std=c++17")
                .flag(format!("-isysroot{}", sysroot).as_str())
                .define("WEBRTC_ENABLE_OBJC_SYMBOL_EXPORT", None)
                .define("WEBRTC_POSIX", None)
                .define("WEBRTC_MAC", None);
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
            let readelf_output = Command::new(toolchain.join("bin/llvm-readelf"))
                .arg("-Ws")
                .arg(webrtc_lib.join("/libwebrtc.a"))
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
