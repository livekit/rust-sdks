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

#include <mutex>
#include <string>

#include "modules/desktop_capture/desktop_capturer.h"
#include "rust/cxx.h"

#ifdef WEBRTC_USE_PIPEWIRE
#include "modules/desktop_capture/linux/wayland/screencast_portal.h"
#endif

namespace livekit_ffi {
class DesktopFrame;
class DesktopCapturer;
class DesktopCapturerOptions;
class Source;
}  // namespace livekit_ffi

#include "webrtc-sys/src/desktop_capturer.rs.h"

namespace livekit_ffi {

#ifdef WEBRTC_USE_PIPEWIRE
// Passes the portal's answers on to the capturer, which is made after the portal, and keeps a
// copy of the restore token. The portal writes its token on its own thread just before it
// answers, so the copy is taken there and read under a lock.
class PortalForwarder : public webrtc::ScreenCastPortal::PortalNotifier {
 public:
  webrtc::ScreenCastPortal::PortalNotifier* target = nullptr;
  webrtc::ScreenCastPortal* portal = nullptr;
  void OnScreenCastRequestResult(webrtc::xdg_portal::RequestResponse result,
                                 uint32_t stream_node_id,
                                 int fd) override {
    {
      std::lock_guard<std::mutex> lock(mutex_);
      // A cancelled or failed request leaves the supplied token in the portal: no new one.
      token_ = result == webrtc::xdg_portal::RequestResponse::kSuccess
                   ? portal->RestoreToken()
                   : std::string();
    }
    target->OnScreenCastRequestResult(result, stream_node_id, fd);
  }
  void OnScreenCastSessionClosed() override {
    target->OnScreenCastSessionClosed();
  }
  std::string token() const {
    std::lock_guard<std::mutex> lock(mutex_);
    return token_;
  }

 private:
  mutable std::mutex mutex_;
  std::string token_;
};
#endif

class DesktopCapturer : public webrtc::DesktopCapturer::Callback {
 public:
  explicit DesktopCapturer(std::unique_ptr<webrtc::DesktopCapturer> capturer)
      : capturer(std::move(capturer)), callback(std::nullopt) {}
#ifdef WEBRTC_USE_PIPEWIRE
  // A portal capturer that keeps its restore token (see new_desktop_capturer).
  DesktopCapturer(std::unique_ptr<PortalForwarder> forwarder,
                  std::unique_ptr<webrtc::DesktopCapturer> capturer)
      : forwarder(std::move(forwarder)),
        capturer(std::move(capturer)),
        callback(std::nullopt) {}
#endif

  void OnCaptureResult(webrtc::DesktopCapturer::Result result,
                       std::unique_ptr<webrtc::DesktopFrame> frame) final;

  rust::Vec<Source> get_source_list() const;
  bool select_source(uint64_t id) const { return capturer->SelectSource(id); }
  void start(rust::Box<DesktopCapturerCallbackWrapper> callback);
  void capture_frame() const { capturer->CaptureFrame(); }
  // The portal's restore token: set before start (ignored after), read once the portal has
  // answered. Empty without a persisting portal.
  void set_restore_token(rust::Str token);
  rust::String restore_token() const;

 private:
#ifdef WEBRTC_USE_PIPEWIRE
  // Declared first so it outlives the capturer, whose portal calls it.
  std::unique_ptr<PortalForwarder> forwarder;
#endif
  bool started = false;
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