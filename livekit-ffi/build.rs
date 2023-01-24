use std::io::Result;

fn main() -> Result<()> {
    prost_build::compile_protos(&["protocol/ffi.proto"], &["protocol/"])?;
    Ok(())
}

