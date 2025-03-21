use std::env;

fn main() {
    let mut build = cc::Build::new();

    build.include("src");
    build.define("SOXR_LIB", "0");

    build
        .flag_if_supported("-std=gnu89")
        .flag_if_supported("-Wnested-externs")
        .flag_if_supported("-Wmissing-prototypes")
        .flag_if_supported("-Wstrict-prototypes")
        .flag_if_supported("-Wconversion")
        .flag_if_supported("-Wall")
        .flag_if_supported("-Wextra")
        .flag_if_supported("-pedantic")
        .flag_if_supported("-Wundef")
        .flag_if_supported("-Wpointer-arith");
    //.flag_if_supported("-Wno-long-long");

    // TODO(theomonnom): Add SIMD support
    let sources = [
        "src/soxr.c",
        "src/data-io.c",
        "src/dbesi0.c",
        "src/filter.c",
        "src/cr.c",
        "src/cr32.c",
        "src/fft4g32.c",
        "src/fft4g.c",
        "src/fft4g64.c",
        "src/vr32.c",
    ];

    for source in &sources {
        build.file(source);
    }

    build.compile("libsoxr.a");

    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap();
    if target_os.as_str() != "windows" {
        println!("cargo:rustc-link-lib=m");
    }
}
