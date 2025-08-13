# webrtc-sys

This crate provides wrapper over the WebRTC API for use from Rust.
We use the crate [cxx.rs](https://cxx.rs/) to simplify our bindings.

## Platform Support

### Windows MSVC

On Windows with the MSVC toolchain, this crate handles the following issues:

- **Exception Handling**: Ensures cxx bridge compilation has exceptions enabled (`/EHsc`)
- **C++20 Standard**: Uses `/std:c++20` and `/Zc:__cplusplus` for proper C++20 support
- **Windows Libraries**: Automatically links required Windows libraries (ws2_32, secur32, bcrypt, etc.)
- **Abseil Compatibility**: Resolves include order issues with bundled vs system Abseil

### Abseil Configuration

This crate supports multiple Abseil configurations:

#### Using System Abseil

To use a system-installed Abseil instead of the bundled version:

**Option 1: Environment Variables**
```bash
export USE_SYSTEM_ABSEIL=1
export ABSEIL_ROOT=/path/to/abseil-cpp
cargo build
```

**Option 2: Cargo Features**
```toml
[dependencies]
webrtc-sys = { version = "0.3", features = ["system-abseil"] }
```

#### Environment Variables

- `USE_SYSTEM_ABSEIL=1`: Enable system Abseil usage
- `ABSEIL_ROOT`: Path to Abseil include directory (preferred)
- `ABSEIL_DIR`: Alternative to ABSEIL_ROOT (backwards compatibility)
- `ABSEIL_LIB_DIR`: Path to Abseil libraries for linking
- `USE_CUSTOM_ABSEIL=1`: Download and use a specific Abseil version

#### Cargo Features

- `system-abseil`: Prefer system-installed Abseil over bundled version
- `bundled-abseil`: Use WebRTC's bundled Abseil (default)

#### Known-Good Abseil Versions

- **20240722.0**: Latest tested version for custom downloads
- **20210324.0**: Common system version (Ubuntu 22.04)

## Wrappers

Most of our wrappers use the cxx.rs types compatible with Rust.
As most of our wrappers are stateless, we allow multiple instances of a specific wrapper to point to the same underlying webrtc pointer. (e.g: multiple livekit::MediaStreamTrack pointing to the same webrtc::MediaStreamTrackInterface).

Threadsafe methods use the const keyword so we can easily call them from the Rust side without worrying about the mutability of the object. (This is similar on how Cell/UnsafeCell works but implemented on the C++ side: interior mutability).

## Code

We also use this C++ code to provide other needed utilities/features on the Rust side (e.g: tiny bindings to libyuv).

