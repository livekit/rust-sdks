#!/bin/bash

PROTOCOL=protocol
OUT_RUST=src

protoc \
    -I=$PROTOCOL \
    --prost_out=$OUT_RUST \
    --prost_opt=compile_well_known_types \
    --prost_opt=extern_path=.google.protobuf=::pbjson_types \
    --prost-serde_out=$OUT_RUST \
    $PROTOCOL/livekit_egress.proto \
    $PROTOCOL/livekit_rtc.proto \
    $PROTOCOL/livekit_room.proto \
    $PROTOCOL/livekit_webhook.proto \
    $PROTOCOL/livekit_models.proto 