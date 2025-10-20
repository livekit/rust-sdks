/*
 * Copyright 2025 LiveKit
 *
 * Licensed under the Apache License, Version 2.0 (the “License”);
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an “AS IS” BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

#include "livekit/desktop_capturer.h"

using SourceList = webrtc::DesktopCapturer::SourceList;

namespace livekit {

std::unique_ptr<DesktopCapturer> new_desktop_capturer(
    rust::Box<DesktopCapturerCallbackWrapper> callback,
    DesktopCapturerOptions options) {
  webrtc::DesktopCaptureOptions webrtc_options =
      webrtc::DesktopCaptureOptions::CreateDefault();
#ifdef __APPLE__
  webrtc_options.set_allow_sck_capturer(true);
  webrtc_options.set_allow_sck_system_picker(options.allow_sck_system_picker);
#endif
#ifdef _WIN64
  if (options.window_capturer) {
    webrtc_options.set_allow_wgc_screen_capturer(true);
  } else {
    webrtc_options.set_allow_wgc_window_capturer(true);
    // https://github.com/webrtc-sdk/webrtc/blob/m137_release/modules/desktop_capture/desktop_capture_options.h#L133-L142
    webrtc_options.set_enumerate_current_process_windows(false);
  }
  webrtc_options.set_allow_directx_capturer(true);
#endif
#ifdef WEBRTC_USE_PIPEWIRE
  webrtc_options.set_allow_pipewire(true);
#endif

  webrtc_options.set_prefer_cursor_embedded(options.include_cursor);

  std::unique_ptr<webrtc::DesktopCapturer> capturer = nullptr;
  if (options.window_capturer) {
    capturer = webrtc::DesktopCapturer::CreateWindowCapturer(webrtc_options);
  } else {
    capturer = webrtc::DesktopCapturer::CreateScreenCapturer(webrtc_options);
  }
  if (!capturer) {
    return nullptr;
  }
  return std::make_unique<DesktopCapturer>(std::move(callback),
                                           std::move(capturer));
}

DesktopCapturer::DesktopCapturer(
    rust::Box<DesktopCapturerCallbackWrapper> callback,
    std::unique_ptr<webrtc::DesktopCapturer> capturer)
    : callback(std::move(callback)), capturer(std::move(capturer)) {}

void DesktopCapturer::OnCaptureResult(
    webrtc::DesktopCapturer::Result result,
    std::unique_ptr<webrtc::DesktopFrame> frame) {
  CaptureResult ret_result = CaptureResult::Success;
  switch (result) {
    case webrtc::DesktopCapturer::Result::SUCCESS:
      ret_result = CaptureResult::Success;
      break;
    case webrtc::DesktopCapturer::Result::ERROR_PERMANENT:
      ret_result = CaptureResult::ErrorPermanent;
      break;
    case webrtc::DesktopCapturer::Result::ERROR_TEMPORARY:
      ret_result = CaptureResult::ErrorTemporary;
      break;
    default:
      break;
  }
  callback->on_capture_result(ret_result,
                              std::make_unique<DesktopFrame>(std::move(frame)));
}

rust::Vec<Source> DesktopCapturer::get_source_list() const {
  SourceList list{};
  bool res = capturer->GetSourceList(&list);
  rust::Vec<Source> source_list{};
  if (res) {
    for (auto& source : list) {
      source_list.push_back(Source{static_cast<uint64_t>(source.id),
                                   source.title, source.display_id});
    }
  }
  return source_list;
}
}  // namespace livekit