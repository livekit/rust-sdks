use cmake::Config;

fn main() {
    let dst = Config::new("soxr")
        .define("BUILD_TESTS", "OFF")
        .define("WITH_OPENMP", "OFF")
        .define("WITH_LSR_BINDINGS", "OFF")
        .define("BUILD_SHARED_LIBS", "OFF")
        .define("WITH_VR32", "OFF")
        .define("CMAKE_POSITION_INDEPENDENT_CODE", "ON")
        .build();

    let lib_dir = dst.join("lib");

    println!("cargo:rustc-link-search=native={}", lib_dir.display());
    println!("cargo:rustc-link-lib=static=soxr");
}
