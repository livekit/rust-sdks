#!/bin/bash
# Copyright 2025 LiveKit, Inc.
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

# Replaces the libstdc++ 10 that Chromium's Debian Bullseye sysroot ships with the
# host's libstdc++, so that libwebrtc.a and webrtc-sys end up on one C++ ABI.
#
# Why this is needed
# ------------------
# libstdc++ reordered the members of std::span in GCC 15: it used to be
# {extent, pointer} and is now {pointer, extent}. The mangled name did not change, so
# a libwebrtc built against libstdc++ 10 headers links cleanly against a webrtc-sys
# built against libstdc++ 15 headers, and then disagrees at runtime about which
# register holds the pointer and which holds the length. Sending on a data channel
# calls CopyOnWriteBuffer::Set(std::span<const uint8_t>), which reads the data pointer
# as an allocation size and aborts in operator new with std::bad_alloc.
#
# Who reads the sysroot's C++ headers
# -----------------------------------
# Only clang. It resolves libstdc++ from the newest GCC installation it finds *inside*
# --sysroot, i.e. <sysroot>/usr/lib/gcc/<triple>/10, and takes the headers next to it.
# The host GCC keeps using its own /usr/include/c++/<version> even under --sysroot, so
# an is_clang=false build does not need this script at all.
#
# Run this after install-sysroot.py and before gn gen, when building with
# is_clang=true and use_sysroot=true.

set -euo pipefail

arch="x64"
gcc_version=""

while [ "$#" -gt 0 ]; do
  case "$1" in
    --arch)
      arch="$2"
      if [ "$arch" != "x64" ] && [ "$arch" != "arm64" ]; then
        echo "Error: Invalid value for --arch. Must be 'x64' or 'arm64'."
        exit 1
      fi
      shift 2
      ;;
    --gcc-version)
      gcc_version="$2"
      shift 2
      ;;
    *)
      echo "Error: Unknown argument '$1'"
      echo "Usage: $0 [--arch x64|arm64] [--gcc-version N]"
      exit 1
      ;;
  esac
done

COMMAND_DIR=$(cd "$(dirname "$0")"; pwd)

case "$arch" in
  x64)
    triple="x86_64-linux-gnu"
    debarch="amd64"
    ;;
  arm64)
    triple="aarch64-linux-gnu"
    debarch="arm64"
    ;;
esac

sysroot="$COMMAND_DIR/src/build/linux/debian_bullseye_${debarch}-sysroot"

if [ ! -d "$sysroot" ]; then
  echo "Error: sysroot not found at $sysroot"
  echo "Run this first: python3 ./src/build/linux/sysroot_scripts/install-sysroot.py --arch=$arch"
  exit 1
fi

# The host toolchain is the ABI we are matching, so default to whatever it is rather
# than hardcoding a version that will go stale.
if [ -z "$gcc_version" ]; then
  gcc_version=$(gcc -dumpversion | cut -d. -f1)
fi

host_cxx_include="/usr/include/c++/$gcc_version"
host_cxx_include_arch="/usr/include/$triple/c++/$gcc_version"
host_gcc_lib="/usr/lib/gcc/$triple/$gcc_version"

for dir in "$host_cxx_include" "$host_cxx_include_arch" "$host_gcc_lib"; do
  if [ ! -d "$dir" ]; then
    echo "Error: $dir is missing."
    if [ "$arch" = "arm64" ] && [ "$(uname -m)" != "aarch64" ]; then
      echo "Cross-compiling to arm64 needs the cross toolchain: apt install g++-$gcc_version-aarch64-linux-gnu"
    else
      echo "Install the matching toolchain: apt install g++-$gcc_version"
    fi
    exit 1
  fi
done

echo "Upgrading sysroot libstdc++ -> $gcc_version"
echo "  sysroot: $sysroot"
echo "  from:    $host_cxx_include"

# Headers. -L dereferences symlinks, because a symlink pointing outside the sysroot
# would resolve against the build machine's / once the compiler is given --sysroot.
rm -rf "$sysroot/usr/include/c++/$gcc_version"
mkdir -p "$sysroot/usr/include/c++"
cp -rL "$host_cxx_include" "$sysroot/usr/include/c++/$gcc_version"

rm -rf "$sysroot/usr/include/$triple/c++/$gcc_version"
mkdir -p "$sysroot/usr/include/$triple/c++"
cp -rL "$host_cxx_include_arch" "$sysroot/usr/include/$triple/c++/$gcc_version"

# clang only treats <sysroot>/usr/lib/gcc/<triple>/<version> as a GCC installation
# when the startup files are there, and it is that discovery which picks the header
# version. Without this the headers above are simply never found.
rm -rf "$sysroot/usr/lib/gcc/$triple/$gcc_version"
mkdir -p "$sysroot/usr/lib/gcc/$triple"
cp -rL "$host_gcc_lib" "$sysroot/usr/lib/gcc/$triple/$gcc_version"

# Runtime library, for anything that links libstdc++ out of the sysroot.
host_libstdcxx=$(ls -1 "/usr/lib/$triple"/libstdc++.so.6.0.* 2>/dev/null | sort -V | tail -1)
if [ -n "$host_libstdcxx" ]; then
  cp "$host_libstdcxx" "$sysroot/usr/lib/$triple/"
  ln -sf "$(basename "$host_libstdcxx")" "$sysroot/usr/lib/$triple/libstdc++.so.6"
  echo "  runtime: $(basename "$host_libstdcxx")"
fi

# Leave the old tree in place. clang picks the highest version it finds, and other
# parts of the sysroot still reference gcc 10 paths.

# Verify that clang actually resolves the new version, since getting this wrong fails
# silently: the build succeeds and only misbehaves at runtime.
clang="$COMMAND_DIR/src/third_party/llvm-build/Release+Asserts/bin/clang++"
if [ -x "$clang" ]; then
  probe=$(mktemp --suffix=.cpp)
  echo 'int main(){}' > "$probe"
  search=$("$clang" --sysroot="$sysroot" --target="$triple" -stdlib=libstdc++ \
    -E -v -x c++ "$probe" -o /dev/null 2>&1 |
    sed -n '/#include <...> search starts here/,/End of search list/p')
  rm -f "$probe"

  if ! echo "$search" | grep -q "include/c++/$gcc_version"; then
    echo "Error: clang still does not resolve libstdc++ $gcc_version from the sysroot:"
    echo "$search"
    exit 1
  fi
  if echo "$search" | grep -o "include/c++/[0-9]*" | grep -qv "include/c++/$gcc_version"; then
    echo "Warning: another libstdc++ version is also on the include path:"
    echo "$search" | grep -o "include/c++/[0-9]*" | sort -u
  fi
  echo "  verified: clang resolves libstdc++ $gcc_version"
else
  echo "  skipped verification: $clang not built yet"
fi

echo "Done."
