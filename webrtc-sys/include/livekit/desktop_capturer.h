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
#include <memory>

#ifndef LK_HEADLESS
#include "modules/desktop_capture/desktop_capturer.h"
#else
#include <vector>
#include <string>
namespace webrtc {
class DesktopFrame {
 public:
  struct Size { int32_t width() const { return 0; } int32_t height() const { return 0; } };
  struct Rect { int32_t left() const { return 0; } int32_t top() const { return 0; } };
  Size size() const { return {}; }
  Rect rect() const { return {}; }
  int32_t stride() const { return 0; }
  const uint8_t* data() const { return nullptr; }
};
class DesktopCapturer {
 public:
  enum class Result { SUCCESS, ERROR_TEMPORARY, ERROR_PERMANENT };
  struct Source {
    int64_t id;
    std::string title;
  };
  typedef std::vector<Source> SourceList;
  class Callback {
   public:
    virtual ~Callback() = default;
    virtual void OnCaptureResult(Result result, std::unique_ptr<DesktopFrame> frame) = 0;
  };
};
}
#endif

#include "rust/cxx.h"


namespace livekit_ffi {
class DesktopFrame;
class DesktopCapturer;
class DesktopCapturerOptions;
class Source;
}  // namespace livekit_ffi

#include "webrtc-sys/src/desktop_capturer.rs.h"

namespace livekit_ffi {

class DesktopCapturer : public webrtc::DesktopCapturer::Callback {
 public:
  explicit DesktopCapturer(std::unique_ptr<webrtc::DesktopCapturer> capturer)
      : capturer(std::move(capturer)), callback(std::nullopt) {}

  void OnCaptureResult(webrtc::DesktopCapturer::Result result,
                       std::unique_ptr<webrtc::DesktopFrame> frame) final;

  rust::Vec<Source> get_source_list() const;
#ifdef LK_HEADLESS
  bool select_source(uint64_t id) const { return false; }
#else
  bool select_source(uint64_t id) const { return capturer->SelectSource(id); }
#endif
  void start(rust::Box<DesktopCapturerCallbackWrapper> callback);
#ifdef LK_HEADLESS
  void capture_frame() const {}
#else
  void capture_frame() const { capturer->CaptureFrame(); }
#endif

 private:
  std::unique_ptr<webrtc::DesktopCapturer> capturer;
  std::optional<rust::Box<DesktopCapturerCallbackWrapper>> callback;
};

class DesktopFrame {
 public:
  DesktopFrame(std::unique_ptr<webrtc::DesktopFrame> frame) : frame(std::move(frame)) {}
  int32_t width() const { return frame->size().width(); }

  int32_t height() const { return frame->size().height(); }

  int32_t left() const { return frame->rect().left(); }

  int32_t top() const { return frame->rect().top(); }

  int32_t stride() const { return frame->stride(); }

  const uint8_t* data() const { return frame->data(); }

 private:
  std::unique_ptr<webrtc::DesktopFrame> frame;
};

std::unique_ptr<DesktopCapturer> new_desktop_capturer(DesktopCapturerOptions options);
}  // namespace livekit_ffi