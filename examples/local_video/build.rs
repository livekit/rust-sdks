use std::path::PathBuf;

fn main() {
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();

    // Only compile the Argus shim on aarch64 Linux (Jetson).
    if target_os != "linux" || target_arch != "aarch64" {
        return;
    }

    let argus_include = PathBuf::from("/usr/src/jetson_multimedia_api/argus/include");
    let mmapi_include = PathBuf::from("/usr/src/jetson_multimedia_api/include");

    if !argus_include.exists() {
        println!(
            "cargo:warning=Argus headers not found at {}; skipping lk_argus build",
            argus_include.display()
        );
        return;
    }

    println!("cargo:rerun-if-changed=src/lk_argus.cpp");

    cc::Build::new()
        .cpp(true)
        .file("src/lk_argus.cpp")
        .include(&argus_include)
        .include(&mmapi_include)
        .flag("-std=c++14")
        .flag("-Wno-deprecated-declarations")
        .compile("lk_argus");

    // Link Argus client library (talks to nvargus-daemon) and NvBufSurface
    println!("cargo:rustc-link-lib=dylib=nvargus_socketclient");
    println!("cargo:rustc-link-lib=dylib=nvbufsurface");

    // Tegra library path
    let tegra_lib_dir = PathBuf::from("/usr/lib/aarch64-linux-gnu/tegra");
    if tegra_lib_dir.exists() {
        println!("cargo:rustc-link-search=native={}", tegra_lib_dir.display());
    }

    // Standard aarch64 library path
    println!("cargo:rustc-link-search=native=/usr/lib/aarch64-linux-gnu");
}
