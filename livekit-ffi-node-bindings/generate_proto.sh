#!/bin/bash

# SPDX-FileCopyrightText: 2024 LiveKit, Inc.
#
# SPDX-License-Identifier: Apache-2.0

# This script requires protobuf-compiler and https://www.npmjs.com/package/@bufbuild/protoc-gen-es
# `brew install protobuf-c && npm install -g @bufbuild/protoc-gen-es@2.2.0`

FFI_PROTOCOL=../livekit-ffi/protocol
FFI_OUT_NODE=./src/proto

# ffi
PATH=$PATH:$(pwd)/node_modules/.bin \
  protoc \
    -I=$FFI_PROTOCOL \
    --es_out $FFI_OUT_NODE \
    --es_opt target=ts \
    --es_opt import_extension=.js \
    $FFI_PROTOCOL/audio_frame.proto \
    $FFI_PROTOCOL/ffi.proto \
    $FFI_PROTOCOL/handle.proto \
    $FFI_PROTOCOL/participant.proto \
    $FFI_PROTOCOL/room.proto \
    $FFI_PROTOCOL/track.proto \
    $FFI_PROTOCOL/track_publication.proto \
    $FFI_PROTOCOL/video_frame.proto \
    $FFI_PROTOCOL/e2ee.proto \
    $FFI_PROTOCOL/stats.proto \
    $FFI_PROTOCOL/rpc.proto \
    $FFI_PROTOCOL/track_publication.proto \
    $FFI_PROTOCOL/data_stream.proto
