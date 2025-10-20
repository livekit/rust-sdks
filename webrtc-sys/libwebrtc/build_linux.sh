#!/bin/bash
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
toolchain="gnu"

while [ "$#" -gt 0 ]; do
  case "$1" in
    --sources)
      sources="$(realpath $2)"
      shift 2
      ;;
    --arch)
      arch="$2"
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
    --toolchain)
      toolchain="$2"
      if [ "$toolchain" != "gnu" ] && [ "$toolchain" != "llvm" ] && [ "$toolchain" != "chromium-llvm" ]; then
        echo "Error: Invalid value for --toolchain. Must be 'gnu', 'llvm', or 'chromium-llvm' (Chromium's bundled Clang with Debian sysroot)"
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

if [ -z "$sources" ]; then
  echo "Error: --sources must be set."
  exit 1
fi

echo "Building LiveKit WebRTC - Linux"
echo "Arch: $arch"
echo "Profile: $profile"
echo "Toolchain: $toolchain"

COMMAND_DIR=$(cd $(dirname $0); pwd)
OUTPUT_DIR="$(dirname "$sources")"
BUILD_DIR="$OUTPUT_DIR/build-$arch-$profile"
ARTIFACTS_DIR="$OUTPUT_DIR/linux-$arch-$profile"

if [ "$toolchain" == "gnu" ]; then
  [ -n "$CC" ] || export CC="$(which gcc)"
  [ -n "$CXX" ] || export CXX="$(which g++)"
  [ -n "$AR" ] || export AR="$(which ar)"
  [ -n "$NM" ] || export NM="$(which nm)"
  export CXXFLAGS="${CXXFLAGS} -Wno-changes-meaning -Wno-unknown-pragmas -D_DEFAULT_SOURCE"
  OBJCOPY="$(which objcopy)"
  chromium_libcxx=false
  toolchain_gn_args="is_clang=false \
  custom_toolchain=\"//build/toolchain/linux/unbundle:default\" \
  host_toolchain=\"//build/toolchain/linux/unbundle:default\""
elif [ "$toolchain" == "llvm" ]; then
  [ -n "$CC" ] || export CC="$(which clang)"
  [ -n "$CXX" ] || export CXX="$(which clang++)"
  [ -n "$AR" ] || export AR="$(which llvm-ar)"
  [ -n "$NM" ] || export NM="$(which llvm-nm)"
  OBJCOPY="$(which llvm-objcopy)"
  # Using system libc++ stumbles over
  # https://github.com/llvm/llvm-project/issues/50248
  # so use Chromium's libc++
  chromium_libcxx=true
  toolchain_gn_args="is_clang=true \
  clang_use_chrome_plugins=false \
  custom_toolchain=\"//build/toolchain/linux/unbundle:default\" \
  host_toolchain=\"//build/toolchain/linux/unbundle:default\""
elif [ "$toolchain" == "chromium-llvm" ]; then
  AR="$sources/third_party/llvm-build/Release+Asserts/bin/llvm-ar"
  OBJCOPY="$sources/third_party/llvm-build/Release+Asserts/bin/llvm-objcopy"
  chromium_libcxx=true
  toolchain_gn_args="is_clang=true use_custom_libcxx=true"
fi

debug="false"
if [ "$profile" = "debug" ]; then
  debug="true"
fi

args="is_debug=$debug  \
  target_os=\"linux\" \
  target_cpu=\"$arch\" \
  rtc_enable_protobuf=false \
  treat_warnings_as_errors=false \
  use_custom_libcxx=${chromium_libcxx}
  use_llvm_libatomic=false \
  use_libcxx_modules=false \
  use_custom_libcxx_for_host=false \
  use_sysroot=false \
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
  rtc_use_pipewire=false \
  symbol_level=0 \
  enable_iterator_debugging=false \
  use_rtti=true \
  rtc_use_x11=false \
  $toolchain_gn_args"

set -xe

# generate ninja files
export PATH="$sources/depot_tools:$PATH"
gn gen "$BUILD_DIR" --root="$sources" --args="${args}"

# build static library
ninja -C "$BUILD_DIR" :default

mkdir -p "$ARTIFACTS_DIR/lib"

# make libwebrtc.a
# don't include nasm
"$AR" -rc "$ARTIFACTS_DIR/lib/libwebrtc.a" `find "$BUILD_DIR/obj" -name '*.o' -not -path "*/third_party/nasm/*"`
"$OBJCOPY" --redefine-syms="$COMMAND_DIR/boringssl_prefix_symbols.txt" "$ARTIFACTS_DIR/lib/libwebrtc.a"

python3 "$sources/tools_webrtc/libs/generate_licenses.py" \
  --target :default "$BUILD_DIR" "$BUILD_DIR"

cp "$BUILD_DIR/obj/webrtc.ninja" "$ARTIFACTS_DIR"
cp "$BUILD_DIR/args.gn" "$ARTIFACTS_DIR"
cp "$BUILD_DIR/LICENSE.md" "$ARTIFACTS_DIR"

cd $sources
if [ $chromium_libcxx == "true" ]; then
  mkdir -p "$ARTIFACTS_DIR/include/buildtools/third_party"
  cp -R buildtools/third_party/libc++ "$ARTIFACTS_DIR/include/buildtools/third_party"
  mkdir -p "$ARTIFACTS_DIR/include/third_party/libc++/src"
  cp -R third_party/libc++/src/include "$ARTIFACTS_DIR/include/third_party/libc++/src"
fi
find . -name "*.h" -print | cpio -pd "$ARTIFACTS_DIR/include"
find . -name "*.inc" -print | cpio -pd "$ARTIFACTS_DIR/include"
