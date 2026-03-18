# Docker Build for LiveKit Rust SDKs

Builds the LiveKit Rust SDK and FFI library inside a [manylinux_2_28](https://github.com/pypa/manylinux) container, ensuring the resulting binaries are compatible with most Linux distributions (glibc 2.28+, i.e. RHEL 8, Ubuntu 20.04, Debian 11, and newer). The x86_64 build uses a CUDA-enabled base image for NVIDIA hardware-accelerated video codec support.

## How it works

The Dockerfile builds an **environment image** containing only system dependencies and the Rust toolchain — no source code is copied in. At build time, the host workspace is volume-mounted into the container (`-v $PWD:/workspace`). This matches the CI approach used in `ffi-builds.yml` and means:

- Git submodules (`yuv-sys/libyuv`, `livekit-protocol/protocol`) are available automatically
- The `target/` directory persists on the host, so incremental builds are fast
- No multi-gigabyte build context is sent to the Docker daemon

## Prerequisites

```bash
git submodule update --init
```

## Makefile targets

```bash
cd docker

# Build environment images (done automatically by sdk/ffi targets)
make env-x86_64
make env-aarch64

# Build the SDK (livekit crate)
make sdk-x86_64
make sdk-aarch64

# Build the FFI library (livekit-ffi crate)
make ffi-x86_64
make ffi-aarch64

# Remove environment images
make clean
```

Build artifacts are written to `target/<triple>/release/` in the host workspace.

## File ownership

Because the container runs as root, files created in `target/` will be root-owned. To reclaim ownership:

```bash
sudo chown -R $(id -u):$(id -g) target/
```
