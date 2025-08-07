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
toolchain="sysroot"

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
    --toolchain)
      toolchain="$2"
      if [ "$toolchain" != "sysroot" ] && [ "$toolchain" != "host" ]; then
        echo "Error: Invalid value for --toolchain. Must be 'sysroot' or 'host'."
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
echo "Toolchain: $toolchain"

if [ ! -e "$(pwd)/depot_tools" ]
then
  git clone --depth 1 https://chromium.googlesource.com/chromium/tools/depot_tools.git
fi

old_path="$PATH"
export COMMAND_DIR=$(cd $(dirname $0); pwd)
export PATH="$PATH:$(pwd)/depot_tools"
export OUTPUT_DIR="$(pwd)/src/out-$arch-$profile"
export ARTIFACTS_DIR="$(pwd)/linux-$arch-$profile"

if [ "$toolchain" = "host" ]; then
  export DEPOT_TOOLS_UPDATE=0
  export VPYTHON_BYPASS='manually managed python not supported by chrome operations'
fi

if [ ! -e "$(pwd)/src" ]
then
  if [ "$toolchain" = "host" ]; then
    cd depot_tools
    # We don't need those deps to build natively
    git apply "$COMMAND_DIR/patches/gclient_ignore_platform_specific_deps.patch" -v --ignore-space-change --ignore-whitespace --whitespace=nowarn
    cd ..
    gclient sync -D --no-history --nohooks
    # manually run some hooks
    python3 src/build/landmines.py --landmine-scripts src/tools_webrtc/get_landmines.py --src-dir src
    python3 src/build/util/lastchange.py -o src/build/util/LASTCHANGE
  else
    gclient sync -D --no-history
  fi
fi

cd src
git apply "$COMMAND_DIR/patches/add_licenses.patch" -v --ignore-space-change --ignore-whitespace --whitespace=nowarn
git apply "$COMMAND_DIR/patches/ssl_verify_callback_with_native_handle.patch" -v --ignore-space-change --ignore-whitespace --whitespace=nowarn
git apply "$COMMAND_DIR/patches/add_deps.patch" -v --ignore-space-change --ignore-whitespace --whitespace=nowarn

if [ "$toolchain" = "host" ]; then
  git apply "$COMMAND_DIR/patches/gn_use_system_python3.patch" -v --ignore-space-change --ignore-whitespace --whitespace=nowarn
  git apply "$COMMAND_DIR/patches/generate_licenses_use_system_gn.patch" -v --ignore-space-change --ignore-whitespace --whitespace=nowarn
fi

cd third_party
git apply "$COMMAND_DIR/patches/abseil_use_optional.patch" -v --ignore-space-change --ignore-whitespace --whitespace=nowarn
cd ..

cd ..

mkdir -p "$ARTIFACTS_DIR/lib"

if [ "$toolchain" = "sysroot" ]; then
  python3 "./src/build/linux/sysroot_scripts/install-sysroot.py" --arch="$arch"
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
  use_custom_libcxx=false \
  rtc_include_tests=false \
  rtc_build_tools=false \
  rtc_build_examples=false \
  rtc_libvpx_build_vp9=true \
  enable_libaom=true \
  is_component_build=false \
  enable_stripping=true \
  use_goma=false \
  ffmpeg_branding=\"Chrome\" \
  rtc_use_h264=true \
  rtc_use_pipewire=false \
  symbol_level=0 \
  enable_iterator_debugging=false \
  use_rtti=true \
  rtc_use_x11=false"

if [ "$debug" = "true" ]; then
  args="${args} is_asan=true is_lsan=true";
fi

if [ "$toolchain" = "host" ]; then
  export PATH="$old_path"
  [ -n "$CC" ] || export CC=clang
  [ -n "$CXX" ] || export CXX=clang++
  [ -n "$AR" ] || export AR=ar
  [ -n "$NM" ] || export NM=nm
  args="${args} \
    custom_toolchain=\"//build/toolchain/linux/unbundle:default\" \
    host_toolchain=\"//build/toolchain/linux/unbundle:default\" \
    clang_use_chrome_plugins=false \
    use_sysroot=false";
fi

# generate ninja files
gn gen "$OUTPUT_DIR" --root="src" --args="${args}"

# build static library
ninja -C "$OUTPUT_DIR" :default

# make libwebrtc.a
# don't include nasm
ar -rc "$ARTIFACTS_DIR/lib/libwebrtc.a" `find "$OUTPUT_DIR/obj" -name '*.o' -not -path "*/third_party/nasm/*"`
objcopy --redefine-syms="$COMMAND_DIR/boringssl_prefix_symbols.txt" "$ARTIFACTS_DIR/lib/libwebrtc.a"

python3 "./src/tools_webrtc/libs/generate_licenses.py" \
  --target :default "$OUTPUT_DIR" "$OUTPUT_DIR"

cp "$OUTPUT_DIR/obj/webrtc.ninja" "$ARTIFACTS_DIR"
cp "$OUTPUT_DIR/args.gn" "$ARTIFACTS_DIR"
cp "$OUTPUT_DIR/LICENSE.md" "$ARTIFACTS_DIR"

cd src
find . -name "*.h" -print | cpio -pd "$ARTIFACTS_DIR/include"

