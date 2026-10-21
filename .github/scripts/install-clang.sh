#!/usr/bin/env bash
#
# Install a clang new enough to build webrtc-sys on Linux.
#
# webrtc-sys compiles against the hermetic libc++ shipped inside the libwebrtc
# artifact (use_custom_libcxx=true). That libc++ tracks LLVM trunk and uses
# builtins that only exist in a recent clang, so the distro clang in our build
# images (AlmaLinux 8: clang 17, Ubuntu 24: clang 18) fails deep inside <limits>
# and <span> instead of saying the compiler is too old. webrtc-sys/build.rs
# checks the version up front and reports the real floor, which it reads from
# the artifact's own __configuration/compiler.h.
#
# The distro package managers have nothing recent enough, so pull the official
# LLVM release tarball instead. If a build image ever rejects it for glibc
# reasons, LLVM_VERSION is the knob to turn.
#
# Prints the bin directory on stdout; everything else goes to stderr. Also
# exports CC/CXX via $GITHUB_ENV when running as a workflow step.
#
# Usage:
#   runner: .github/scripts/install-clang.sh   # sets CC/CXX for later steps
#   docker: export LLVM_ROOT=/opt/llvm
#           .github/scripts/install-clang.sh
#           export CC=$LLVM_ROOT/bin/clang CXX=$LLVM_ROOT/bin/clang++

set -euo pipefail

LLVM_VERSION="${LLVM_VERSION:-21.1.8}"
LLVM_ROOT="${LLVM_ROOT:-/opt/llvm-$LLVM_VERSION}"

case "$(uname -m)" in
  x86_64)          llvm_arch=X64 ;;
  aarch64 | arm64) llvm_arch=ARM64 ;;
  *) echo "install-clang.sh: unsupported architecture $(uname -m)" >&2; exit 1 ;;
esac

if [ "$(id -u)" -eq 0 ]; then
  sudo=""
else
  sudo="sudo"
fi

if [ ! -x "$LLVM_ROOT/bin/clang++" ]; then
  url="https://github.com/llvm/llvm-project/releases/download/llvmorg-$LLVM_VERSION/LLVM-$LLVM_VERSION-Linux-$llvm_arch.tar.xz"
  echo "install-clang.sh: fetching $url" >&2
  $sudo mkdir -p "$LLVM_ROOT"
  # --strip-components=1 drops the LLVM-<version>-Linux-<arch>/ prefix.
  curl --fail --location --silent --show-error "$url" \
    | $sudo tar -xJ --strip-components=1 -C "$LLVM_ROOT"
fi

"$LLVM_ROOT/bin/clang++" --version >&2

if [ -n "${GITHUB_ENV:-}" ]; then
  {
    echo "CC=$LLVM_ROOT/bin/clang"
    echo "CXX=$LLVM_ROOT/bin/clang++"
  } >> "$GITHUB_ENV"
fi

echo "$LLVM_ROOT/bin"
