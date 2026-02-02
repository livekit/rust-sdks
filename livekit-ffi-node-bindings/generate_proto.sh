#!/bin/bash
# Copyright 2026 LiveKit, Inc.
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
