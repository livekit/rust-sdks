/// Links the foreground shim for macOS builds that enable Apple Vision.
fn main() {
    #[cfg(feature = "apple-vision")]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        println!("cargo:rerun-if-changed=src/layers/objects/vision.m");
        cc::Build::new()
            .file("src/layers/objects/vision.m")
            .flag("-fobjc-arc")
            .compile("vector_video_vision");
        for framework in ["Foundation", "CoreVideo", "Vision"] {
            println!("cargo:rustc-link-lib=framework={framework}");
        }
        println!("cargo:rustc-link-lib=objc");
    }
}
