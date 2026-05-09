/*
 * Copyright 2025 LiveKit, Inc.
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

#pragma once

#include <cstdint>
#include <memory>
#include <optional>

#include "rust/cxx.h"

namespace livekit_ffi {
class MacosScreenCapturer;
class MacosScreenFrame;
class MacosScreen;
}  // namespace livekit_ffi

#include "webrtc-sys/src/macos_screen_capturer.rs.h"

namespace livekit_ffi {

class MacosScreenFrame {
 public:
  explicit MacosScreenFrame(void* pixel_buffer);
  ~MacosScreenFrame();

  int32_t width() const;
  int32_t height() const;
  uintptr_t pixel_buffer() const;

 private:
  void* pixel_buffer_;
};

class MacosScreenCapturer {
 public:
  MacosScreenCapturer();
  ~MacosScreenCapturer();

  rust::Vec<MacosScreen> get_screen_list() const;
  bool start(uint32_t display_id,
             uint32_t fps,
             rust::Box<MacosScreenCapturerCallbackWrapper> callback);
  void stop();

  void on_frame(void* pixel_buffer);
  void on_error(bool permanent);

 private:
  void* stream_;
  void* output_;
  void* queue_;
  std::optional<rust::Box<MacosScreenCapturerCallbackWrapper>> callback_;
};

std::unique_ptr<MacosScreenCapturer> new_macos_screen_capturer();

}  // namespace livekit_ffi
