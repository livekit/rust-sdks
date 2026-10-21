#!/bin/bash
# Copyright 2025 LiveKit, Inc.
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

# dependencies: cargo install protoc-gen-prost@0.5.0 protoc-gen-prost-serde@0.4.0


PROTOCOL=${PROTOCOL:-protocol/protobufs}
OUT_RUST=src

protoc \
    -I=$PROTOCOL \
    --prost_out=$OUT_RUST \
    --prost_opt=compile_well_known_types \
    --prost_opt=extern_path=.google.protobuf=::pbjson_types \
    --prost_opt=flat_output_dir \
    --prost-serde_out=$OUT_RUST \
    --prost-serde_opt=ignore_unknown_fields \
    --prost-serde_opt=flat_output_dir \
    $PROTOCOL/livekit_egress.proto \
    $PROTOCOL/livekit_rtc.proto \
    $PROTOCOL/livekit_room.proto \
    $PROTOCOL/livekit_webhook.proto \
    $PROTOCOL/livekit_sip.proto \
    $PROTOCOL/livekit_models.proto \
    $PROTOCOL/livekit_connector.proto \
    $PROTOCOL/livekit_connector_whatsapp.proto \
    $PROTOCOL/livekit_connector_twilio.proto \
    $PROTOCOL/livekit_analytics.proto \
    $PROTOCOL/livekit_agent.proto \
    $PROTOCOL/livekit_agentdb.proto \
    $PROTOCOL/agent/livekit_agent_dev.proto \
    $PROTOCOL/agent/livekit_agent_session.proto \
    $PROTOCOL/agent/livekit_agent_text.proto \
    $PROTOCOL/agent/livekit_agent_thread.proto \
    $PROTOCOL/agent/livekit_agent_inference.proto \
    $PROTOCOL/cloud_replay.proto \
    $PROTOCOL/livekit_agent_dispatch.proto \
    $PROTOCOL/livekit_agent_simulation.proto \
    $PROTOCOL/livekit_agent_worker.proto \
    $PROTOCOL/livekit_cloud_agent.proto \
    $PROTOCOL/livekit_ingress.proto \
    $PROTOCOL/livekit_internal.proto \
    $PROTOCOL/livekit_metrics.proto \
    $PROTOCOL/livekit_phone_number.proto \
    $PROTOCOL/livekit_token_source.proto \
    $PROTOCOL/logger/options.proto \
    $PROTOCOL/roomrpc/siprpc/roomrpc_sip_v1.proto
