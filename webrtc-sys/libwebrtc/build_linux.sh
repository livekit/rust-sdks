#!/bin/bash
# Exit immediately if any command fails. This ensures CI properly reports build
# failures instead of continuing to create empty/broken artifacts.
set -e

# Copyright 2023 LiveKit, Inc.
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


arch=""
profile="release"

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
    --profile)
      profile="$2"
      if [ "$profile" != "debug" ] && [ "$profile" != "release" ]; then
        echo "Error: Invalid value for --profile. Must be 'debug' or 'release'."
        exit 1
      fi
      shift 2
      ;;
    *)
      echo "Error: Unknown argument '$1'"
      exit 1
      ;;
  esac
done

if [ -z "$arch" ]; then
  echo "Error: --arch must be set."
  exit 1
fi

echo "Building LiveKit WebRTC - Linux"
echo "Arch: $arch"
echo "Profile: $profile"

if [ ! -e "$(pwd)/depot_tools" ]
then
  git clone --depth 1 https://chromium.googlesource.com/chromium/tools/depot_tools.git
fi

export COMMAND_DIR=$(cd $(dirname $0); pwd)
export PATH="$(pwd)/depot_tools:$PATH"
export OUTPUT_DIR="$(pwd)/src/out-$arch-$profile"
export ARTIFACTS_DIR="$(pwd)/linux-$arch-$profile"

if [ ! -e "$(pwd)/src" ]
then
  gclient sync -D --no-history
fi

cd src
git apply "$COMMAND_DIR/patches/add_licenses.patch" -v --ignore-space-change --ignore-whitespace --whitespace=nowarn
git apply "$COMMAND_DIR/patches/fix_license_json_parsing.patch" -v --ignore-space-change --ignore-whitespace --whitespace=nowarn
git apply "$COMMAND_DIR/patches/ssl_verify_callback_with_native_handle.patch" -v --ignore-space-change --ignore-whitespace --whitespace=nowarn
git apply "$COMMAND_DIR/patches/add_deps.patch" -v --ignore-space-change --ignore-whitespace --whitespace=nowarn
git apply "$COMMAND_DIR/patches/fix_desktop_capture_compile.patch" -v --ignore-space-change --ignore-whitespace --whitespace=nowarn
git apply "$COMMAND_DIR/patches/external_audio_source.patch" -v --ignore-space-change --ignore-whitespace --whitespace=nowarn
git apply "$COMMAND_DIR/patches/fix_pipewire_utils_compile.patch" -v --ignore-space-change --ignore-whitespace --whitespace=nowarn

# Disable CREL (compact relocations). Chromium's build enables experimental
# CREL via -Wa,--crel which causes segfaults on aarch64-linux (and is known
# broken on arm32 and s390x too).
# See: https://crbug.com/376278218
# See: https://github.com/zed-industries/zed/pull/51433#discussion_r2944567608
git -C build apply "$COMMAND_DIR/patches/disable_crel.patch" -v --ignore-space-change --ignore-whitespace --whitespace=nowarn

cd third_party

git apply "$COMMAND_DIR/patches/david_disable_gun_source_macro.patch" -v --ignore-space-change --ignore-whitespace --whitespace=nowarn

cd libyuv

git apply "$COMMAND_DIR/patches/disable_sme_for_libyuv.patch" -v --ignore-space-change --ignore-whitespace --whitespace=nowarn

cd ../../..

mkdir -p "$ARTIFACTS_DIR/lib"

python3 "./src/build/linux/sysroot_scripts/install-sysroot.py" --arch="$arch"

debug="false"
if [ "$profile" = "debug" ]; then
  debug="true"
fi

# Note: use_clang_modules=false is required to avoid C++ module compilation issues.
# Without this flag, the build may fail partway through, resulting in missing
# or incomplete artifacts.
#
# The C++ standard library choice is an ABI contract with webrtc-sys:
#
#   use_custom_libcxx=true  builds against Chromium's hermetic libc++, whose
#     headers we ship in the artifacts (see below) so webrtc-sys compiles against
#     byte-identical std types. The alternative -- letting each side use its own
#     host stdlib -- is unsound now that WebRTC puts std types like std::span in
#     public API signatures: libstdc++ reordered std::span's members after GCC 10,
#     so a span built by the host and read inside libwebrtc.a had its pointer and
#     size swapped, turning a 14-byte DataChannel::Send into new uint8_t[93TB].
args="is_debug=$debug  \
  target_os=\"linux\" \
  target_cpu=\"$arch\" \
  rtc_enable_protobuf=false \
  treat_warnings_as_errors=false \
  use_llvm_libatomic=false \
  use_custom_libcxx=true \
  use_custom_libcxx_for_host=true \
  use_clang_modules=false \
  rtc_include_tests=false \
  rtc_build_tools=false \
  rtc_build_examples=false \
  rtc_libvpx_build_vp9=true \
  enable_libaom=true \
  is_component_build=false \
  enable_stripping=true \
  ffmpeg_branding=\"Chrome\" \
  rtc_use_h264=true \
  rtc_use_h265=true \
  rtc_use_pipewire=true \
  symbol_level=0 \
  enable_iterator_debugging=false \
  use_rtti=true \
  rtc_use_x11=true"

# generate ninja files
gn gen "$OUTPUT_DIR" --root="src" --args="${args}"

# build static library
ninja -C "$OUTPUT_DIR" :default

# Build the hermetic libc++/libc++abi for the *target* explicitly.
#
# :default does not: nothing here links a target binary (no tests, tools or
# examples, and libwebrtc is a static library), so the libc++ headers get used
# but its own translation units are never compiled. On x64 that went unnoticed
# because use_custom_libcxx_for_host builds libc++ for the host toolchain, which
# in a native build shares $OUTPUT_DIR/obj with the target — so the host copy
# landed in the archive below by accident. Cross-compiling arm64 puts the host
# copy under $OUTPUT_DIR/clang_x64/obj instead, and the arm64 artifact shipped
# with every out-of-line std::__Cr symbol undefined (std::__Cr::locale,
# __libcpp_verbose_abort, __cxa_throw, ...). Nothing in this build notices; it
# only surfaces when a downstream crate links webrtc-sys.
#
# Ask ninja for the archive paths rather than hardcoding a GN label, so this
# fails loudly instead of silently no-opping if libc++ ever moves. Host copies
# live under a toolchain subdir, so anchoring at obj/ keeps them out.
libcxx_archives=$(ninja -C "$OUTPUT_DIR" -t targets all \
  | grep -oE '^obj/[^:]*/libc\+\+(abi)?\.a' | sort -u)
if [ -z "$libcxx_archives" ]; then
  echo "Error: no hermetic libc++ static library in the ninja graph for $OUTPUT_DIR." >&2
  echo "       use_custom_libcxx=true is an ABI contract with webrtc-sys; aborting." >&2
  exit 1
fi
echo "Building hermetic libc++: $libcxx_archives"
ninja -C "$OUTPUT_DIR" $libcxx_archives

# make libwebrtc.a
# don't include nasm
# Start from scratch: `ar -rc` only replaces members it is given, so members left
# over from a previous build with different args would survive into the archive.
rm -f "$ARTIFACTS_DIR/lib/libwebrtc.a"
ar -rc "$ARTIFACTS_DIR/lib/libwebrtc.a" `find "$OUTPUT_DIR/obj" -name '*.o' -not -path "*/third_party/nasm/*"`
src/third_party/llvm-build/Release+Asserts/bin/llvm-objcopy --redefine-syms="$COMMAND_DIR/boringssl_prefix_symbols.txt" "$ARTIFACTS_DIR/lib/libwebrtc.a"

# The archive is the whole standard library for webrtc-sys (build.rs passes
# cpp_link_stdlib(None), since mixing in the host libstdc++ would be a second,
# incompatible stdlib). Check the out-of-line half is really in there instead of
# publishing an artifact that only fails at the downstream link step. One nm pass
# over an 80MB archive is enough for both markers: libc++ and libc++abi land in
# obj/ together or not at all.
libcxx_markers=$(src/third_party/llvm-build/Release+Asserts/bin/llvm-nm --defined-only \
  "$ARTIFACTS_DIR/lib/libwebrtc.a" 2>/dev/null \
  | grep -oE '__libcpp_verbose_abort|__cxa_throw' | sort -u | tr '\n' ' ')
for sym in __libcpp_verbose_abort __cxa_throw; do
  case " $libcxx_markers " in
    *" $sym "*) ;;
    *)
      echo "Error: $ARTIFACTS_DIR/lib/libwebrtc.a defines no $sym." >&2
      echo "       The hermetic libc++/libc++abi objects are missing from the archive." >&2
      exit 1
      ;;
  esac
done

# License generation is optional - may fail with some Python versions
# Use vpython3 from depot_tools for consistent Python version
vpython3 "./src/tools_webrtc/libs/generate_licenses.py" \
  --target :default "$OUTPUT_DIR" "$OUTPUT_DIR" || echo "Warning: License generation failed (non-critical)"

cp "$OUTPUT_DIR/obj/webrtc.ninja" "$ARTIFACTS_DIR"
cp "$OUTPUT_DIR/obj/modules/desktop_capture/desktop_capture.ninja" "$ARTIFACTS_DIR"
cp "$OUTPUT_DIR/args.gn" "$ARTIFACTS_DIR"

cp "$OUTPUT_DIR/LICENSE.md" "$ARTIFACTS_DIR"

cd src
find . -name "*.h" -print | cpio -pd "$ARTIFACTS_DIR/include"
find . -name "*.inc" -print | cpio -pd "$ARTIFACTS_DIR/include"

# Ship Chromium's hermetic libc++ so webrtc-sys can compile against the exact
# same standard library that is baked into libwebrtc.a (see use_custom_libcxx
# above). The find calls cannot do this: libc++ headers have no extension.
# Paths mirror the -isystem/-I flags in build/config/c++/BUILD.gn, so build.rs
# can point at them the same way the WebRTC build does.
for inc in third_party/libc++/src/include third_party/libc++abi/src/include; do
  mkdir -p "$ARTIFACTS_DIR/include/$inc"
  cp -R "$inc/." "$ARTIFACTS_DIR/include/$inc/"
done
mkdir -p "$ARTIFACTS_DIR/include/buildtools/third_party/libc++"
cp buildtools/third_party/libc++/__config_site \
  buildtools/third_party/libc++/__assertion_handler \
  "$ARTIFACTS_DIR/include/buildtools/third_party/libc++/"
