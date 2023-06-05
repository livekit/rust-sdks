/*
 * Copyright 2023 LiveKit
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

#pragma once

#include <memory>

#include "api/jsep.h"
#include "api/ref_counted_base.h"
#include "api/set_local_description_observer_interface.h"
#include "api/set_remote_description_observer_interface.h"
#include "livekit/rtc_error.h"
#include "rust/cxx.h"

namespace livekit {
class IceCandidate;
class SessionDescription;
};  // namespace livekit
#include "webrtc-sys/src/jsep.rs.h"

namespace livekit {

class AsyncContext;

class IceCandidate {
 public:
  explicit IceCandidate(
      std::unique_ptr<webrtc::IceCandidateInterface> ice_candidate);

  rust::String sdp_mid() const;
  int sdp_mline_index() const;
  rust::String candidate() const;  // TODO(theomonnom) Return livekit::Candidate
                                   // instead of rust::String

  rust::String stringify() const;
  std::unique_ptr<webrtc::IceCandidateInterface> release();

 private:
  std::unique_ptr<webrtc::IceCandidateInterface> ice_candidate_;
};

std::shared_ptr<IceCandidate> create_ice_candidate(rust::String sdp_mid,
                                                   int sdp_mline_index,
                                                   rust::String sdp);

static std::shared_ptr<IceCandidate> _shared_ice_candidate() {
  return nullptr;  // Ignore
}

class SessionDescription {
 public:
  explicit SessionDescription(
      std::unique_ptr<webrtc::SessionDescriptionInterface> session_description);

  SdpType sdp_type() const;
  rust::String stringify() const;
  std::unique_ptr<SessionDescription> clone() const;
  std::unique_ptr<webrtc::SessionDescriptionInterface> release();

 private:
  std::unique_ptr<webrtc::SessionDescriptionInterface> session_description_;
};

std::unique_ptr<SessionDescription> create_session_description(
    SdpType type,
    rust::String sdp);

static std::unique_ptr<SessionDescription> _unique_session_description() {
  return nullptr;  // Ignore
}

class NativeCreateSdpObserver
    : public webrtc::CreateSessionDescriptionObserver {
 public:
  NativeCreateSdpObserver(
      rust::Box<AsyncContext> ctx,
      rust::Fn<void(rust::Box<AsyncContext> ctx,
                    std::unique_ptr<SessionDescription>)> on_success,
      rust::Fn<void(rust::Box<AsyncContext> ctx, RtcError)> on_error);

  void OnSuccess(webrtc::SessionDescriptionInterface* desc) override;
  void OnFailure(webrtc::RTCError error) override;

 private:
  rust::Box<AsyncContext> ctx_;
  rust::Fn<void(rust::Box<AsyncContext>, std::unique_ptr<SessionDescription>)>
      on_success_;
  rust::Fn<void(rust::Box<AsyncContext>, RtcError)> on_error_;
};

class NativeSetLocalSdpObserver
    : public webrtc::SetLocalDescriptionObserverInterface {
 public:
  NativeSetLocalSdpObserver(
      rust::Box<AsyncContext> ctx,
      rust::Fn<void(rust::Box<AsyncContext>, RtcError)> on_complete);

  void OnSetLocalDescriptionComplete(webrtc::RTCError error) override;

 private:
  rust::Box<AsyncContext> ctx_;
  rust::Fn<void(rust::Box<AsyncContext>, RtcError)> on_complete_;
};

class NativeSetRemoteSdpObserver
    : public webrtc::SetRemoteDescriptionObserverInterface {
 public:
  NativeSetRemoteSdpObserver(
      rust::Box<AsyncContext> ctx,
      rust::Fn<void(rust::Box<AsyncContext>, RtcError)> on_complete);

  void OnSetRemoteDescriptionComplete(webrtc::RTCError error) override;

 private:
  rust::Box<AsyncContext> ctx_;
  rust::Fn<void(rust::Box<AsyncContext>, RtcError)> on_complete_;
};

}  // namespace livekit
