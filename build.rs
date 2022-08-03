
fn main() {
    let target_os = "macos";
    //let target_arch = "arm64";

    let libwebrtc_dir = std::path::PathBuf::from("./libwebrtc");


    // Just required for the bridge build to succeed.
    let includes = &[
        std::path::PathBuf::from("./include"),
        libwebrtc_dir.join("include/"),
        libwebrtc_dir.join("include/third_party/abseil-cpp/"),
        libwebrtc_dir.join("include/third_party/libc++/"),
    ];

    let mut builder = cxx_build::bridges(&["src/main.rs"]);
    builder.flag("-std=c++17"); // Not sure about this .. Shouldn't this be c++14 ?
    builder.file("src/peer_connection_factory.cpp");

    for include in includes {
        builder.include(include);
    }

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

            builder.flag("-stdlib=libc++")
                .define("WEBRTC_ENABLE_OBJC_SYMBOL_EXPORT", None)
                .define("WEBRTC_POSIX", None)
                .define("WEBRTC_MAC", None);
        }
        _ => {
            eprintln!("Unsupported platform, {}", target_os);
            std::process::exit(1);
        }
    }

    builder.warnings(false).compile("lkwebrtc");

    println!("cargo:rustc-link-search=native={}", libwebrtc_dir.canonicalize().unwrap().to_str().unwrap());
    println!("cargo:rustc-link-lib=static=webrtc");

    for entry in glob::glob("./src/**/*.cpp").unwrap() {
        println!("cargo:rerun-if-changed={}", entry.unwrap().display().to_string());
    }

    for entry in glob::glob("./include/**/*.h").unwrap() {
        println!("cargo:rerun-if-changed={}", entry.unwrap().display().to_string());
    }

    println!("cargo:rerun-if-changed=src/main.rs");
}
