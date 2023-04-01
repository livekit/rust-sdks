#!/bin/bash -eu

if [ ! -e "$(pwd)/depot_tools" ]
then
  git clone --depth 1 https://chromium.googlesource.com/chromium/tools/depot_tools.git
fi

export PATH="$(pwd)/depot_tools:$PATH"
export OUTPUT_DIR="$(pwd)/src/out"
export ARTIFACTS_DIR="$(pwd)/macos"

if [ ! -e "$(pwd)/src" ]
then
  gclient sync
fi

mkdir -p "$ARTIFACTS_DIR/lib"

for is_debug in "true" "false"
do
  for target_cpu in "x64" "arm64"
  do

    # generate ninja files
    gn gen "$OUTPUT_DIR" --root="src" \
      --args="is_debug=${is_debug} \
      target_os=\"mac\"  \
      target_cpu=\"${target_cpu}\" \
      use_custom_libcxx=false \
      rtc_include_tests=false \
      rtc_build_examples=false \
      rtc_use_h264=false \
      symbol_level=0 \
      enable_iterator_debugging=false \
      is_component_build=false \
      use_rtti=true"

    # build static library
    ninja -C "$OUTPUT_DIR" webrtc

    # copy static library
    mkdir -p "$ARTIFACTS_DIR/lib/${target_cpu}"
    cp "$OUTPUT_DIR/obj/libwebrtc.a" "$ARTIFACTS_DIR/lib/${target_cpu}/"
  done

  filename="libwebrtc.a"
  if [ $is_debug = "true" ]; then
    filename="libwebrtcd.a"
  fi

  # make universal binary
  lipo -create -output \
  "$ARTIFACTS_DIR/lib/${filename}" \
  "$ARTIFACTS_DIR/lib/arm64/libwebrtc.a" \
  "$ARTIFACTS_DIR/lib/x64/libwebrtc.a"

  rm -r "$ARTIFACTS_DIR/lib/arm64"
  rm -r "$ARTIFACTS_DIR/lib/x64"
done

python3 "./src/tools_webrtc/libs/generate_licenses.py" \
  --target :webrtc "$OUTPUT_DIR" "$OUTPUT_DIR"

cd src
find . -name "*.h" -print | cpio -pd "$ARTIFACTS_DIR/include"

cp "$OUTPUT_DIR/LICENSE.md" "$ARTIFACTS_DIR"
