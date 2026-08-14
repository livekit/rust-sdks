use std::path::{Path, PathBuf};

fn main() {
    println!("cargo:rustc-check-cfg=cfg(lk_argus)");
    println!("cargo:rustc-check-cfg=cfg(lk_mpp)");
    println!("cargo:rerun-if-env-changed=LK_MPP_INCLUDE_DIR");
    println!("cargo:rerun-if-env-changed=PKG_CONFIG_SYSROOT_DIR");

    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();

    // Rockchip MPP support is compiled by webrtc-sys. Mirror its header
    // detection so the publisher only references the internal decoder bridge
    // when that bridge is available.
    if target_os == "linux" && target_arch == "aarch64" && find_mpp_include_dir().is_some() {
        println!("cargo:rustc-cfg=lk_mpp");
    }

    // Only compile the Argus shim on aarch64 Linux (Jetson).
    if target_os != "linux" || target_arch != "aarch64" {
        return;
    }

    let argus_include = PathBuf::from("/usr/src/jetson_multimedia_api/argus/include");
    let mmapi_include = PathBuf::from("/usr/src/jetson_multimedia_api/include");

    println!("cargo:rerun-if-changed={}", argus_include.display());
    println!("cargo:rerun-if-changed={}", mmapi_include.display());

    if !argus_include.exists() {
        println!(
            "cargo:warning=Argus headers not found at {}; skipping lk_argus build",
            argus_include.display()
        );
        return;
    }

    println!("cargo:rustc-cfg=lk_argus");
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

fn find_mpp_include_dir() -> Option<PathBuf> {
    let header = Path::new("rockchip/rk_mpi.h");

    if let Ok(path) = std::env::var("LK_MPP_INCLUDE_DIR") {
        let include_dir = PathBuf::from(path);
        if include_dir.join(header).is_file() {
            return Some(include_dir);
        }
        println!(
            "cargo:warning=LK_MPP_INCLUDE_DIR does not contain {}; ignoring it",
            header.display()
        );
    }

    if let Ok(path) = std::env::var("PKG_CONFIG_SYSROOT_DIR") {
        let include_dir = PathBuf::from(path).join("usr/include");
        if include_dir.join(header).is_file() {
            return Some(include_dir);
        }
    }

    if std::env::var("HOST").ok() == std::env::var("TARGET").ok() {
        let include_dir = PathBuf::from("/usr/include");
        if include_dir.join(header).is_file() {
            return Some(include_dir);
        }
    }

    None
}
