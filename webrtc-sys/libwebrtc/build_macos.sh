#!/bin/bash -eu

if [ ! -e "$(pwd)/depot_tools" ]
then
  git clone --depth 1 https://chromium.googlesource.com/chromium/tools/depot_tools.git
fi

export COMMAND_DIR=$(cd $(dirname $0); pwd)
export PATH="$(pwd)/depot_tools:$PATH"
export OUTPUT_DIR="$(pwd)/src/out"
export ARTIFACTS_DIR="$(pwd)/macos"

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

for is_debug in "true" "false"
do
  for target_cpu in "x64" "arm64"
  do

    # generate ninja files
    gn gen "$OUTPUT_DIR" --root="src" \
      --args="is_debug=${is_debug} \
      enable_dsyms=${is_debug} \
      target_os=\"mac\"  \
      target_cpu=\"${target_cpu}\" \
      mac_deployment_target=\"10.11\" \
      treat_warnings_as_errors=false \
      rtc_enable_protobuf=false \
      rtc_include_tests=false \
      rtc_build_examples=false \
      rtc_build_tools=false \
      rtc_libvpx_build_vp9=true \
      is_component_build=false \
      enable_stripping=true \
      rtc_enable_symbol_export=true \
      rtc_enable_objc_symbol_export=false \
      rtc_use_h264=false \
      use_custom_libcxx=false \
      clang_use_chrome_plugins=false \
      use_rtti=true \
      use_lld=false"

    # build static library
    ninja -C "$OUTPUT_DIR" api/audio_codecs:builtin_audio_decoder_factory \
      api/task_queue:default_task_queue_factory \
      sdk:native_api \
      sdk:default_codec_factory_objc \
      pc:peerconnection \
      sdk:videocapture_objc \
      sdk:mac_framework_objc

    filename="libwebrtc.a"
    if [ $is_debug = "true" ]; then
      filename="libwebrtcd.a"
    fi

    # make libwebrtc.a
    mkdir -p "$ARTIFACTS_DIR/lib/${target_cpu}"

    # don't include nasm (on macos x86_64, there is two main fnc)
    ar -rc "$ARTIFACTS_DIR/lib/${target_cpu}/${filename}" `find "$OUTPUT_DIR/obj" -name '*.o' -not -path "*/third_party/nasm/*"`
  done
done

python3 "./src/tools_webrtc/libs/generate_licenses.py" \
  --target :webrtc "$OUTPUT_DIR" "$OUTPUT_DIR"

cd src
find . -name "*.h" -print | cpio -pd "$ARTIFACTS_DIR/include"

cp "$OUTPUT_DIR/LICENSE.md" "$ARTIFACTS_DIR"
