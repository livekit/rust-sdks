#!/bin/bash

COMMAND_DIR="$(cd $(dirname $0); pwd)"
SOURCE_DIR="$COMMAND_DIR/src"
ARCHIVE_PATH="$COMMAND_DIR/libwebrtc-source.tar.xz"

set -x

# for debugging this script
reset() {
    rm -rf "$SOURCE_DIR" "$COMMAND_DIR/depot_tools" "$ARCHIVE_PATH"
}

download() {
    cd "$COMMAND_DIR"
    git clone --depth 1 https://chromium.googlesource.com/chromium/tools/depot_tools.git
    # use --nohooks to avoid the download_from_google_storage hook that takes > 6 minutes
    # then manually run the other hooks
    export PATH="$COMMAND_DIR/depot_tools:$PATH"
    gclient sync -D --no-history --nohooks
    python3 "$SOURCE_DIR/tools/rust/update_rust.py"
    python3 "$SOURCE_DIR/tools/clang/scripts/update.py"
    mv depot_tools src
}

patch() {
    cd $SOURCE_DIR
    git apply "$COMMAND_DIR/patches/add_licenses.patch" -v --ignore-space-change --ignore-whitespace --whitespace=nowarn
    git apply "$COMMAND_DIR/patches/ssl_verify_callback_with_native_handle.patch" -v --ignore-space-change --ignore-whitespace --whitespace=nowarn
    git apply "$COMMAND_DIR/patches/add_deps.patch" -v --ignore-space-change --ignore-whitespace --whitespace=nowarn
    cd third_party
    git apply "$COMMAND_DIR/patches/david_disable_gun_source_macro.patch" -v --ignore-space-change --ignore-whitespace --whitespace=nowarn
}

clean() {
    required_files_and_dirs=(
        abseil-cpp
        boringssl
        catapult
        closure_compiler
        dav1d
        ffmpeg
        freetype
        googletest
        harfbuzz-ng
        icu
        jsoncpp
        libaom
        libjpeg_turbo
        libjpeg.gni
        BUILD.gn
        libpng
        libsrtp
        libvpx
        libyuv
        nasm
        openh264
        opus
        perfetto
        protobuf
        protobuf-javascript
        rust
        rust-toolchain
        zlib
        fuzztest
        crc32c
        pffft
        re2
        google_benchmark
        rnnoise
    )
    for filename in $(ls -1 "$SOURCE_DIR/third_party"); do
        if ! [[ ${required_files_and_dirs[@]} =~ $filename ]]; then
            rm -rf "$SOURCE_DIR/third_party/$filename"
        fi
    done

    third_party_dirs=(
        rust-toolchain/bin
        rust-toolchain/lib
        catapult/tracing/test_data
        catapult/third_party/vinn/third_party/v8
        catapult/third_party/gsutil
        rust/chromium_crates_io
        boringssl/src/third_party/wycheproof_testvectors
        openh264/src/res
        ffmpeg/tests
        harfbuzz-ng/src/test
        harfbuzz-ng/src/perf
        perfetto/src/traced/probes/ftrace/test
        perfetto/src/trace_processor
        perfetto/docs
        perfetto/test
        perfetto/ui
        icu/source/data
        icu/source/test
        icu/source
        boringssl/src/crypto/cipher/test
        boringssl/src/fuzz
        opus/src/dnn/torch/osce/resources
        nasm/travis
        nasm/test
        closure_compiler/compiler
    )
    for dir in "${third_party_dirs[@]}"; do
        rm -rf "$SOURCE_DIR/third_party/$dir"
    done

    src_dirs=(
        depot_tools/.cipd_bin
        depot_tools/.cipd_client
        buildtools/reclient
        tools/perf/testdata
        tools/luci-go
        tools/metrics
        tools/resultdb
        tools/perf/page_sets
        tools/perf/core/shard_maps/timing_data
        tools/disable_tests/tests
        data
        base/test
    )
    for dir in "${src_dirs[@]}"; do
        rm -rf "$SOURCE_DIR/$dir"
    done

    dir_names=(testdata test_data doc docs \.git)
        for dir_name in "${dir_names[@]}"; do
            for dir in $(find "$SOURCE_DIR" -name $dir_name); do
                rm -rf "$dir"
            done
    done
}

package() {
    tar -Jcf "$ARCHIVE_PATH" -C "$COMMAND_DIR" src
}

reset
set -e
download
patch
clean
package
