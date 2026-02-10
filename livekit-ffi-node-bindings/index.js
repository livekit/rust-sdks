// Copyright 2026 LiveKit, Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

// @ts-check

module.exports = {
  ...require("./native.js"),
  ...require("./proto/audio_frame_pb.js"),
  ...require("./proto/ffi_pb.js"),
  ...require("./proto/handle_pb.js"),
  ...require("./proto/participant_pb.js"),
  ...require("./proto/room_pb.js"),
  ...require("./proto/track_pb.js"),
  ...require("./proto/track_publication_pb.js"),
  ...require("./proto/video_frame_pb.js"),
  ...require("./proto/e2ee_pb.js"),
  ...require("./proto/stats_pb.js"),
  ...require("./proto/rpc_pb.js"),
  ...require("./proto/data_stream_pb.js"),
};
