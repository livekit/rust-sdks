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

#ifndef LIVEKIT_DESKTOP_CAPTURER_H
#define LIVEKIT_DESKTOP_CAPTURER_H

#include <memory>

#include "api/make_ref_counted.h"
#include "api/scoped_refptr.h"
#include "livekit_rtc/include/capi.h"
#include "modules/desktop_capture/desktop_capturer.h"

namespace livekit_ffi {

typedef enum {
  Screen,
  Window,
  Generic,
} SourceType;

typedef enum {
  Success,
  ErrorPermanent,
  ErrorTemporary,
} CaptureResult;

typedef struct {
  bool allow_sck_system_picker;
  SourceType source_type;
  bool include_cursor;
} DesktopCapturerOptions;

class DesktopSource : public webrtc::RefCountInterface {
 public:
  DesktopSource(uint64_t id, const std::string& title, int64_t display_id)
      : id_(id), title_(title), display_id_(display_id) {}

  uint64_t id() const { return id_; }
  std::string title() const { return title_; }
  int64_t display_id() const { return display_id_; }

 private:
  uint64_t id_;
  std::string title_;
  int64_t display_id_;
};

using lkDesktopCapturerCallback = void (*)(lkDesktopFrame* frame,
                                           lkCaptureResult result,
                                           void* userdata);

class DesktopCapturer : public webrtc::RefCountInterface,
                        public webrtc::DesktopCapturer::Callback {
 public:
  explicit DesktopCapturer(std::unique_ptr<webrtc::DesktopCapturer> capturer)
      : capturer(std::move(capturer)) {}

  void OnCaptureResult(webrtc::DesktopCapturer::Result result,
                       std::unique_ptr<webrtc::DesktopFrame> frame) final;

  lkVectorGeneric* get_source_list() const;
  bool select_source(uint64_t id) const { return capturer->SelectSource(id); }
  void start(lkDesktopCapturerCallback callback, void* userdata);
  void capture_frame() const { capturer->CaptureFrame(); }

 private:
  std::unique_ptr<webrtc::DesktopCapturer> capturer;
  lkDesktopCapturerCallback callback_ = nullptr;
  void* userdata_ = nullptr;
};

class DesktopFrame : public webrtc::RefCountInterface {
 public:
  DesktopFrame(std::unique_ptr<webrtc::DesktopFrame> frame)
      : frame(std::move(frame)) {}
  int32_t width() const { return frame->size().width(); }

  int32_t height() const { return frame->size().height(); }

  int32_t left() const { return frame->rect().left(); }

  int32_t top() const { return frame->rect().top(); }

  int32_t stride() const { return frame->stride(); }

  const uint8_t* data() const { return frame->data(); }

 private:
  std::unique_ptr<webrtc::DesktopFrame> frame;
};

webrtc::scoped_refptr<DesktopCapturer> new_desktop_capturer(
    const lkDesktopCapturerOptions* options);
}  // namespace livekit_ffi

#endif  // LIVEKIT_DESKTOP_CAPTURER_H
