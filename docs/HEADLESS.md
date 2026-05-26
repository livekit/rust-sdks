# Headless Linux Build Support

This document details how to compile and run the LiveKit Rust SDK on headless Linux environments (such as server deployments, Docker containers, or CI environments) where desktop-related libraries (e.g., X11, GLib, GIO, DRM, and GBM) are not installed.

---

## Overview

By default, the SDK compiles with native desktop capture support (`glib-main-loop`). On Linux, this requires GLib event loops and X11/Wayland libraries. For server-side or containerized usage, these desktop dependencies are unnecessary and complicate builds.

We provide a **unified hybrid configuration** offering two parallel mechanisms to compile without these dependencies:

1. **Option A: Cargo Features** (Declarative configuration within your Cargo dependency tree)
2. **Option B: Environment Variables** (Direct shell or CI/CD environment overrides)

Regardless of the option chosen, **the public Rust API surface remains identical**. `DesktopCapturer::new()` compiles successfully but returns `None` at runtime, ensuring your downstream code doesn't need to be gated by custom `#[cfg]` attributes.

---

## Comparison: Option A vs. Option B

| Dimension | Option A: Cargo Features | Option B: Environment Variable |
| :--- | :--- | :--- |
| **Configuration Style** | Declarative in `Cargo.toml`. | Shell environment (`export LK_HEADLESS=1`). |
| **Scope** | Targets a specific crate or workspace package. | Global override across the entire build invocation. |
| **Workspace Ergonomics** | Fragile if some crates transitively unify default features. | Highly robust for workspace builds and containerized builds. |
| **Caching Integration** | Automatic compile cache invalidation by Cargo. | Enabled via `cargo:rerun-if-env-changed` in `build.rs`. |
| **Best Used For** | Purely headless applications declaring their own dependencies. | CI/CD pipelines, Dockerfiles, and heterogeneous workspaces. |

---

## Option A: Cargo Features (`headless`)

To configure a headless build declaratively, disable the default features of the `livekit` crate and opt in to `headless` and `tokio`:

```toml
[dependencies]
livekit = { version = "0.7", default-features = false, features = ["tokio", "headless"] }
```

### Option A: FAQ

#### Q1: Why do I need `default-features = false`?
By default, the `livekit` crate includes the `glib-main-loop` feature to provide desktop capturing integration out of the box. You must disable default features to prevent this feature from pulling in GLib.

#### Q2: What happens if a third-party dependency pulls in default features of `livekit`?
Due to Cargo's feature unification, if any dependency in your tree enables default features for `livekit`, the `glib-main-loop` feature will be unified and enabled globally. In this case, Option A will fail, and you must use **Option B** to force a headless compile.

#### Q3: Do I need `resolver = "2"` in my workspace?
Yes. Cargo's resolver version 2 is required to prevent build-dependencies (like `build.rs` dependencies) from unifying their features with target dependencies. The root `Cargo.toml` in the SDK is already configured with `resolver = "2"`.

---

## Option B: Environment Variable (`LK_HEADLESS=1`)

To force a headless build globally without editing `Cargo.toml` files, set the environment variable:

```bash
LK_HEADLESS=1 cargo build --release
```

Or inject it into a Dockerfile:

```dockerfile
ENV LK_HEADLESS=1
RUN cargo build --release
```

### Option B: FAQ

#### Q1: How does `LK_HEADLESS=1` bypass Cargo feature unification?
Even if the `glib-main-loop` Cargo feature is transitively enabled, the `build.rs` script of `webrtc-sys` checks the `LK_HEADLESS` environment variable. If set to `1` or `true`, the build script completely bypasses probing for GLib/GIO/X11/DRM/GBM packages.

#### Q2: Can this cause runtime linker errors?
No. Because `build.rs` skips registering these library linkages, the binary will not link against X11, GLib, or DRM libraries.

#### Q3: Does this affect hardware accelerated video codecs (CUDA / VAAPI)?
No. Hardware acceleration (such as NVidia CUDA/NVCodec or Intel VAAPI) is preserved on headless builds since they do not require desktop environments. They continue to load their respective drivers dynamically via dlopen at runtime.

#### Q4: How is compile caching affected when changing `LK_HEADLESS`?
The build script prints `cargo:rerun-if-env-changed=LK_HEADLESS`. Cargo automatically invalidates the compile cache and rebuilds the native bindings if you switch the environment variable between builds.

---

## What Changes in the API?

The Rust API surface is kept completely uniform. You do **not** need to wrap your screensharing logic in `#[cfg]` attributes:

```rust
use livekit::webrtc::desktop_capturer::{DesktopCapturer, DesktopCapturerOptions, DesktopCaptureSourceType};

// This compiles on both desktop and headless targets
let options = DesktopCapturerOptions {
    source_type: DesktopCaptureSourceType::Screen,
    include_cursor: true,
    allow_sck_system_picker: false,
};

// Returns Some(capturer) on desktop; returns None on headless Linux/servers
if let Some(capturer) = DesktopCapturer::new(options) {
    println!("Desktop capturer started successfully!");
} else {
    println!("Running in a headless/server environment (desktop capturer disabled).");
}
```

---

## How It Works Under the Hood

1. **Feature Wiring**: The `headless` Cargo feature is forwarded from `livekit` -> `libwebrtc` -> `webrtc-sys`.
2. **Build Script Bypassing**: If `LK_HEADLESS=1` or `CARGO_FEATURE_HEADLESS` is detected, `webrtc-sys/build.rs` skips GLib/GIO/X11 pkg-config lookups and defines the preprocessor macro `-DLK_HEADLESS=1` for the C++ compiler.
3. **C++ Stubbing**: Inside `desktop_capturer.cpp`, all methods accessing WebRTC's desktop capture systems are enclosed in `#ifndef LK_HEADLESS` guards. In headless mode, the factory returns `nullptr`, and all operations become safe no-ops.
