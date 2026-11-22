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

#include "livekit/desktop_capturer.h"

#include "modules/desktop_capture/desktop_capture_options.h"

#ifdef WEBRTC_USE_PIPEWIRE
#include "modules/desktop_capture/linux/wayland/base_capturer_pipewire.h"
#include "modules/desktop_capture/linux/wayland/shared_screencast_stream.h"
#endif

using SourceList = webrtc::DesktopCapturer::SourceList;

namespace livekit_ffi {

std::unique_ptr<DesktopCapturer> new_desktop_capturer(
    DesktopCapturerOptions options) {
  webrtc::DesktopCaptureOptions webrtc_options =
      webrtc::DesktopCaptureOptions::CreateDefault();
#if defined(WEBRTC_MAC) && !defined(WEBRTC_IOS)
  webrtc_options.set_allow_sck_capturer(true);
  webrtc_options.set_allow_sck_system_picker(options.allow_sck_system_picker);
#endif /* defined(WEBRTC_MAC) && !defined(WEBRTC_IOS) */
#ifdef _WIN64
  switch (options.source_type) {
    case SourceType::Screen:
      webrtc_options.set_allow_wgc_screen_capturer(true);
      break;
    case SourceType::Window:
      webrtc_options.set_allow_wgc_window_capturer(true);
      // https://github.com/webrtc-sdk/webrtc/blob/m137_release/modules/desktop_capture/desktop_capture_options.h#L133-L142
      webrtc_options.set_enumerate_current_process_windows(false);
      break;
    default:
      break;
  }
  webrtc_options.set_allow_directx_capturer(true);
#endif /* _WIN64 */
#ifdef WEBRTC_USE_PIPEWIRE
  webrtc_options.set_allow_pipewire(true);
#endif /* WEBRTC_USE_PIPEWIRE */

  // prefer_cursor_embedded indicate that the capturer should try to include the
  // cursor in the frame
  webrtc_options.set_prefer_cursor_embedded(options.include_cursor);

#ifdef WEBRTC_USE_PIPEWIRE
  // On Wayland, a capturer that persists its restore token makes the portal itself rather than
  // letting the capturer do it, so the portal can ask for a token that outlives the app and be
  // given a saved one: libwebrtc's own asks for a token that lasts only while the app runs.
  if (options.persist_restore_token &&
      options.source_type == SourceType::Generic &&
      webrtc::DesktopCapturer::IsRunningUnderWayland()) {
    if (!webrtc_options.screencast_stream()) {
      webrtc_options.set_screencast_stream(
          webrtc::SharedScreenCastStream::CreateDefault());
    }
    auto forwarder = std::make_unique<PortalForwarder>();
    auto portal = std::make_unique<webrtc::ScreenCastPortal>(
        webrtc::CaptureType::kAnyScreenContent, forwarder.get(),
        options.include_cursor);
    portal->SetPersistMode(webrtc::ScreenCastPortal::PersistMode::kPersistent);
    forwarder->portal = portal.get();
    auto pipewire = std::make_unique<webrtc::BaseCapturerPipeWire>(
        webrtc_options, std::move(portal));
    forwarder->target = pipewire.get();
    return std::make_unique<DesktopCapturer>(std::move(forwarder),
                                             std::move(pipewire));
  }
#endif

  std::unique_ptr<webrtc::DesktopCapturer> capturer = nullptr;
  switch (options.source_type) {
    case SourceType::Window:
      capturer = webrtc::DesktopCapturer::CreateWindowCapturer(webrtc_options);
      break;
    case SourceType::Screen:
      capturer = webrtc::DesktopCapturer::CreateScreenCapturer(webrtc_options);
      break;
    case SourceType::Generic:
      capturer = webrtc::DesktopCapturer::CreateGenericCapturer(webrtc_options);
      break;
    default:
      return nullptr;
  }

  if (!capturer) {
    return nullptr;
  }
  return std::make_unique<DesktopCapturer>(std::move(capturer));
}

void DesktopCapturer::start(
    rust::Box<DesktopCapturerCallbackWrapper> callback) {
  this->callback = std::move(callback);
  started = true;
  capturer->Start(this);
}

void DesktopCapturer::OnCaptureResult(
    webrtc::DesktopCapturer::Result result,
    std::unique_ptr<webrtc::DesktopFrame> frame) {
  CaptureResult ret_result = CaptureResult::ErrorPermanent;
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
  if (callback) {
    (*callback)->on_capture_result(
        ret_result, std::make_unique<DesktopFrame>(std::move(frame)));
  }
}

void DesktopCapturer::set_restore_token(rust::Str token) {
#ifdef WEBRTC_USE_PIPEWIRE
  // Before start, the portal has no thread of its own yet.
  if (forwarder && !started) {
    forwarder->portal->SetRestoreToken(std::string(token));
  }
#endif
}

rust::String DesktopCapturer::restore_token() const {
#ifdef WEBRTC_USE_PIPEWIRE
  if (forwarder) {
    return rust::String(forwarder->token());
  }
#endif
  return rust::String();
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
}  // namespace livekit_ffi