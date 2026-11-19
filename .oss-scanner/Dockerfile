# Copyright 2026 LiveKit, Inc.
#
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
#     http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.

# Build image for Anthropic's OSS Scanner. Keep the Rust version in step with rust-toolchain.toml.
FROM rust:1.97.1-bookworm

ENV DEBIAN_FRONTEND=noninteractive

# The Linux packages from tests.yml, minus the NVIDIA/CUDA ones; webrtc-sys skips those encoders when cuda.h is absent.
RUN apt-get update && apt-get install -y --no-install-recommends \
      build-essential ca-certificates curl git pkg-config protobuf-compiler unzip xz-utils \
      libasound2-dev libssl-dev libx11-dev libgl1-mesa-dev libxext-dev libva-dev libdrm-dev \
      libgbm-dev libxfixes-dev libxdamage-dev libxrandr-dev libxcomposite-dev libglib2.0-dev \
    && rm -rf /var/lib/apt/lists/*

# webrtc-sys compiles against libwebrtc's hermetic libc++, which needs clang 21+; bookworm ships 14.
# The LLVM bin dir also provides ld.lld for the aarch64 `-fuse-ld=lld` in .cargo/config.toml.
COPY .github/scripts/install-clang.sh /tmp/install-clang.sh
# The LLVM release unpacks to ~11 GB. Keep the compiler, linker, binutils and libclang (for bindgen); drop the rest.
RUN LLVM_ROOT=/opt/llvm /tmp/install-clang.sh \
    && cd /opt/llvm/bin && find . -maxdepth 1 ! -name . \
         ! -name 'clang' ! -name 'clang++' ! -name 'clang-[0-9]*' ! -name 'clang-cpp' \
         ! -name 'lld' ! -name 'ld.lld' ! -name 'llvm-ar' ! -name 'llvm-ranlib' ! -name 'llvm-nm' \
         ! -name 'llvm-objcopy' ! -name 'llvm-objdump' ! -name 'llvm-strip' ! -name 'llvm-symbolizer' \
         ! -name 'llvm-config' -delete \
    && rm -f /opt/llvm/lib/*.a /opt/llvm/lib/liblldb* /opt/llvm/lib/libMLIR* /opt/llvm/lib/libflang* \
    && /opt/llvm/bin/clang++ --version && /opt/llvm/bin/ld.lld --version
ENV PATH=/opt/llvm/bin:$PATH \
    CC=/opt/llvm/bin/clang \
    CXX=/opt/llvm/bin/clang++

# Fetch the prebuilt libwebrtc that webrtc-sys-build would download, into a fixed path outside target/, so an
# offline `cargo clean` or `--release` build still finds it.
COPY webrtc-sys/build/src/lib.rs /tmp/webrtc-sys-build.rs
RUN set -e; \
    tag=$(sed -n 's/^pub const WEBRTC_TAG: &str = "\(.*\)";/\1/p' /tmp/webrtc-sys-build.rs); \
    case "$(dpkg --print-architecture)" in amd64) arch=x64 ;; arm64) arch=arm64 ;; *) exit 1 ;; esac; \
    curl -fsSL -o /tmp/webrtc.zip \
      "https://github.com/livekit/rust-sdks/releases/download/$tag/webrtc-linux-$arch-release.zip"; \
    mkdir -p /opt/libwebrtc; \
    unzip -q /tmp/webrtc.zip -d /opt/libwebrtc; \
    ln -s "/opt/libwebrtc/linux-$arch-release" /opt/libwebrtc/current; \
    rm /tmp/webrtc.zip
ENV LK_CUSTOM_WEBRTC=/opt/libwebrtc/current

WORKDIR /src
COPY . /src

# The scanner clones without submodules, and yuv-sys builds libyuv from one.
RUN git submodule update --init --recursive
RUN rustup toolchain install && rustc --version

RUN cargo fetch --locked

# Examples, tools/bindgens, the Node bindings, livekit-wakeword and livekit-capture are left out: they pull in GUI, ONNX
# and Node toolchains that add build time but no SDK attack surface.
RUN cargo test --locked --no-run \
      -p livekit -p livekit-api -p livekit-protocol -p livekit-token -p livekit-token-source \
      -p livekit-signaling -p livekit-net -p livekit-region -p livekit-common -p livekit-telemetry \
      -p livekit-data-stream -p livekit-datatrack -p livekit-rpc -p livekit-ffi \
      -p libwebrtc -p webrtc-sys -p webrtc-sys-build -p yuv-sys -p soxr-sys -p imgproc -p device-info \
    && cargo test --locked --no-run -p livekit-ffi --no-default-features --features core-modules

RUN timeout 10m sh -c '\
      cargo test --locked \
        -p livekit -p livekit-api -p livekit-protocol -p livekit-token -p livekit-token-source \
        -p livekit-signaling -p livekit-net -p livekit-region -p livekit-common -p livekit-telemetry \
        -p livekit-data-stream -p livekit-datatrack -p livekit-rpc -p livekit-ffi \
        -p libwebrtc -p webrtc-sys -p webrtc-sys-build -p yuv-sys -p soxr-sys -p imgproc -p device-info \
      && cargo test --locked -p livekit-ffi --no-default-features --features core-modules' \
    || echo 'WARNING: baseline tests failed'

ENV CARGO_NET_OFFLINE=true

CMD ["bash"]
