@echo off

rem Copyright 2023 LiveKit, Inc.
rem
rem Licensed under the Apache License, Version 2.0 (the "License");
rem you may not use this file except in compliance with the License.
rem You may obtain a copy of the License at
rem
rem     http://www.apache.org/licenses/LICENSE-2.0
rem
rem Unless required by applicable law or agreed to in writing, software
rem distributed under the License is distributed on an "AS IS" BASIS,
rem WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
rem See the License for the specific language governing permissions and
rem limitations under the License.

set PROTOCOL=protocol
set OUT_RUST=src

protoc.exe ^
    -I=%PROTOCOL% ^
    --prost_out=%OUT_RUST% ^
    %PROTOCOL%/ffi.proto ^
    %PROTOCOL%/handle.proto ^
    %PROTOCOL%/room.proto ^
    %PROTOCOL%/track.proto ^
    %PROTOCOL%/track_publication.proto ^
    %PROTOCOL%/participant.proto ^
    %PROTOCOL%/video_frame.proto ^
    %PROTOCOL%/audio_frame.proto ^
    %PROTOCOL%/e2ee.proto ^
    %PROTOCOL%/stats.proto ^
    %PROTOCOL%/rpc.proto
