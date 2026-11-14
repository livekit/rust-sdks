/*
 * Copyright 2026 LiveKit, Inc.
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

#include "api/scoped_refptr.h"
#include "livekit/adm_proxy.h"
#include "rust/cxx.h"

namespace livekit_ffi {

class RtcRuntime;

class AudioDeviceController {
 public:
  AudioDeviceController(std::shared_ptr<RtcRuntime> rtc_runtime,
                        webrtc::scoped_refptr<AdmProxy> adm_proxy);

  // Device enumeration
  int16_t playout_devices() const;
  int16_t recording_devices() const;
  rust::String playout_device_name(uint16_t index) const;
  rust::String recording_device_name(uint16_t index) const;
  rust::String playout_device_guid(uint16_t index) const;
  rust::String recording_device_guid(uint16_t index) const;

  // Device selection
  bool set_playout_device(uint16_t index) const;
  bool set_recording_device(uint16_t index) const;
  bool set_playout_device_by_guid(rust::String guid) const;
  bool set_recording_device_by_guid(rust::String guid) const;

  // The device last selected successfully, as a token the matching restore
  // call accepts: the device's GUID (or "index:<n>" where the platform has no
  // GUIDs), "windows:<type>" for a Windows default device, or empty if none
  // was selected and the platform default is in use.
  rust::String playout_device_selection() const;
  rust::String recording_device_selection() const;
  // Selects the device a token from *_device_selection() names, looking a GUID
  // up in the current device list. An empty token selects the platform default.
  bool restore_playout_device_selection(rust::String selection) const;
  bool restore_recording_device_selection(rust::String selection) const;

  // Recording control
  bool stop_recording() const;
  bool init_recording() const;
  bool start_recording() const;
  bool recording_is_initialized() const;

  // Playout control
  bool stop_playout() const;
  bool init_playout() const;
  bool start_playout() const;
  bool playout_is_initialized() const;

  // Built-in audio processing
  bool builtin_aec_is_available() const;
  bool builtin_agc_is_available() const;
  bool builtin_ns_is_available() const;
  bool enable_builtin_aec(bool enable) const;
  bool enable_builtin_agc(bool enable) const;
  bool enable_builtin_ns(bool enable) const;

  // ADM recording control
  void set_adm_recording_enabled(bool enabled) const;
  bool adm_recording_enabled() const;

  // ADM playout control
  void set_adm_playout_enabled(bool enabled) const;
  bool adm_playout_enabled() const;

  // Platform ADM lifecycle management
  bool acquire_platform_adm() const;
  void release_platform_adm() const;
  int platform_adm_ref_count() const;
  bool is_platform_adm_active() const;

 private:
  // Select a device and remember it. The caller holds selection_mutex_.
  bool select_playout_device(uint16_t index) const;
  bool select_recording_device(uint16_t index) const;
  // Select a device by GUID, "index:<n>" ID or name. The caller holds
  // selection_mutex_.
  bool select_playout_device_by_id(const std::string& target_guid) const;
  bool select_recording_device_by_id(const std::string& target_guid) const;

  // The AdmProxy marshals its calls onto the runtime's worker thread, keep
  // the runtime (and its threads) alive as long as Rust can reach the proxy.
  // Declaration order matters: rtc_runtime_ must be declared before
  // adm_proxy_ so it is destroyed last. ~AdmProxy does a BlockingCall onto
  // the worker thread, which must still be running at that point.
  std::shared_ptr<RtcRuntime> rtc_runtime_;
  webrtc::scoped_refptr<AdmProxy> adm_proxy_;

  // The last successful selections, kept as stable identities (see
  // *_device_selection()) so they survive devices being added or removed.
  // selection_mutex_ is held across each selection, from the native call to
  // remembering its result, so concurrent selections can't remember a device
  // other than the one selected. Holding it while the proxy blocks on the
  // worker thread is safe: the worker never calls back into this controller
  // (see AdmProxy).
  mutable std::mutex selection_mutex_;
  mutable std::string playout_selection_;
  mutable std::string recording_selection_;
};

}  // namespace livekit_ffi
