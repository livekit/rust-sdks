use std::env;
use std::fs::File;
use std::io::Write;
use std::path::Path;

fn main() {
    let target = env::var("TARGET").unwrap();
    let host = env::var("HOST").unwrap();

    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("triples.rs");
    let mut f = File::create(dest_path).unwrap();

    writeln!(f, "pub const HOST_TRIPLE: &str = \"{}\";", host).unwrap();
    writeln!(f, "pub const TARGET_TRIPLE: &str = \"{}\";", target).unwrap();
}
