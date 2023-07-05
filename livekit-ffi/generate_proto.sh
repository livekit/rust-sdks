#!/bin/bash

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
    $PROTOCOL/audio_frame.proto 