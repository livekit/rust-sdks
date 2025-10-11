use libloading::{Library, Symbol};
use std::{path::Path, process::Command};

const FFI_LIB_PATH: &str = env!("FFI_LIB_PATH"); // Set by build.rs

const EXPECTED_SYMBOLS: &[&str] = &[
    "livekit_ffi_initialize",
    "livekit_ffi_request",
    "livekit_ffi_drop_handle",
    "livekit_ffi_dispose",
];

#[test]
fn lib_load_test() {
    println!("Library path: {}", FFI_LIB_PATH);
    build_lib_if_required();
    unsafe {
        let lib = Library::new(FFI_LIB_PATH).expect("Unable to load library");
        for symbol in EXPECTED_SYMBOLS {
            let _loaded_symbol: Symbol<unsafe extern "C" fn() -> u32> =
                lib.get(symbol.as_bytes()).expect(&format!("Missing symbol: {}", symbol));
        }
    }
}

fn build_lib_if_required() {
    let path = Path::new(FFI_LIB_PATH);
    if !path.try_exists().unwrap() {
        println!("Library not found, building…");
        let status = Command::new("cargo")
            .args(&["build", "--lib"])
            .status()
            .expect("Failed to run cargo build for test");

        if !status.success() || !path.try_exists().unwrap() {
            panic!("Failed to build lib to run test");
        }
    }
}
