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


PROTOCOL=protocol
OUT_RUST=src

protoc \
    -I=$PROTOCOL \
    --prost_out=$OUT_RUST \
    $PROTOCOL/ffi.proto \
    $PROTOCOL/handle.proto \
    $PROTOCOL/room.proto \
    $PROTOCOL/track.proto \
    $PROTOCOL/participant.proto \
    $PROTOCOL/video_frame.proto \
    $PROTOCOL/audio_frame.proto \
    $PROTOCOL/e2ee.proto \
    $PROTOCOL/stats.proto
