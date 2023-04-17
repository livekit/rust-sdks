#!/bin/bash -eu

if [ ! -e "$(pwd)/depot_tools" ]
then
  git clone --depth 1 https://chromium.googlesource.com/chromium/tools/depot_tools.git
fi

export PATH="$(pwd)/depot_tools:$PATH"
export OUTPUT_DIR="$(pwd)/src/out"
export ARTIFACTS_DIR="$(pwd)/linux"

if [ ! -e "$(pwd)/src" ]
then
  gclient sync
fi

cd src
git apply "$COMMAND_DIR/patches/add_license_dav1d.patch" -v
git apply "$COMMAND_DIR/patches/ssl_verify_callback_with_native_handle.patch" -v
git apply "$COMMAND_DIR/patches/fix_mocks.patch" -v
cd ..

mkdir -p "$ARTIFACTS_DIR/lib"

for target_cpu in "x64"
do
  mkdir -p "$ARTIFACTS_DIR/lib/${target_cpu}"
  for is_debug in "true" "false"
  do
    args="is_debug=${is_debug} \
      target_os=\"linux\" \
      target_cpu=\"${target_cpu}\" \
      rtc_enable_protobuf=false \
      treat_warnings_as_errors=false \
      use_custom_libcxx=false \
      rtc_include_tests=false \
      rtc_build_tools=false \
      rtc_build_examples=false \
      rtc_libvpx_build_vp9=true \
      is_component_build=false \
      enable_stripping=true \
      use_goma=false \
      rtc_use_h264=false \
      symbol_level=0 \
      enable_iterator_debugging=false \
      use_rtti=true \
      rtc_use_x11=false"

    if [ $is_debug = "true" ]; then
      args="${args} is_asan=true is_lsan=true";
    fi

    # generate ninja files
    gn gen "$OUTPUT_DIR" --root="src" --args="${args}"

    # build static library
    ninja -C "$OUTPUT_DIR" webrtc

    filename="libwebrtc.a"
    if [ $is_debug = "true" ]; then
      filename="libwebrtcd.a"
    fi

    # cppy static library
    cp "$OUTPUT_DIR/obj/libwebrtc.a" "$ARTIFACTS_DIR/lib/${target_cpu}/${filename}"
  done
done

python3 "./src/tools_webrtc/libs/generate_licenses.py" \
  --target :webrtc "$OUTPUT_DIR" "$OUTPUT_DIR"

cd src
find . -name "*.h" -print | cpio -pd "$ARTIFACTS_DIR/include"

cp "$OUTPUT_DIR/LICENSE.md" "$ARTIFACTS_DIR"
