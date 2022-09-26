//
// Created by Théo Monnom on 01/09/2022.
//

#include "livekit/jsep.h"

#include <iomanip>
#include <memory>

#include "libwebrtc-sys/src/jsep.rs.h"
#include "livekit/rtc_error.h"

namespace livekit {

const std::string& serialize_sdp_error(webrtc::SdpParseError error) {
  std::stringstream ss;
  ss << std::hex << std::setfill('0');
  ss << std::setw(8) << (uint32_t)error.line.length();
  ss << std::dec << std::setw(1) << error.line;
  ss << std::dec << std::setw(1) << error.description;
  return ss.str();
}

IceCandidate::IceCandidate(
    std::unique_ptr<webrtc::IceCandidateInterface> ice_candidate)
    : ice_candidate_(std::move(ice_candidate)) {}

rust::String IceCandidate::stringify() const {
  std::string str;
  ice_candidate_->ToString(&str);
  return rust::String{str};
}

std::unique_ptr<webrtc::IceCandidateInterface> IceCandidate::release() {
  return std::move(ice_candidate_);
}

std::unique_ptr<IceCandidate> create_ice_candidate(rust::String sdp_mid,
                                                   int sdp_mline_index,
                                                   rust::String sdp) {
  webrtc::SdpParseError error;
  auto ice_rtc = webrtc::CreateIceCandidate(sdp_mid.c_str(), sdp_mline_index,
                                            sdp.c_str(), &error);
  if (!ice_rtc) {
    throw std::runtime_error(serialize_sdp_error(error));
  }

  return std::make_unique<IceCandidate>(
      std::unique_ptr<webrtc::IceCandidateInterface>(ice_rtc));
}

SessionDescription::SessionDescription(
    std::unique_ptr<webrtc::SessionDescriptionInterface> session_description)
    : session_description_(std::move(session_description)) {}

rust::String SessionDescription::stringify() const {
  std::string str;
  session_description_->ToString(&str);
  return rust::String{str};
}

std::unique_ptr<SessionDescription> SessionDescription::clone() const {
  return std::make_unique<SessionDescription>(session_description_->Clone());
}

std::unique_ptr<webrtc::SessionDescriptionInterface>
SessionDescription::release() {
  return std::move(session_description_);
}

std::unique_ptr<SessionDescription> create_session_description(
    SdpType type,
    rust::String sdp) {
  webrtc::SdpParseError error;
  auto rtc_sdp = webrtc::CreateSessionDescription(
      static_cast<webrtc::SdpType>(type), sdp.c_str(), &error);
  if (!rtc_sdp) {
    throw std::runtime_error(serialize_sdp_error(error));
  }

  return std::make_unique<SessionDescription>(std::move(rtc_sdp));
}

// CreateSdpObserver

NativeCreateSdpObserver::NativeCreateSdpObserver(
    rust::Box<CreateSdpObserverWrapper> observer)
    : observer_(std::move(observer)) {}

void NativeCreateSdpObserver::OnSuccess(
    webrtc::SessionDescriptionInterface* desc) {
  // We have ownership of desc
  observer_->on_success(std::make_unique<SessionDescription>(
      std::unique_ptr<webrtc::SessionDescriptionInterface>(desc)));
}

void NativeCreateSdpObserver::OnFailure(webrtc::RTCError error) {
  observer_->on_failure(to_error(error));
}

std::unique_ptr<NativeCreateSdpObserverHandle>
create_native_create_sdp_observer(
    rust::Box<CreateSdpObserverWrapper> observer) {
  return std::make_unique<NativeCreateSdpObserverHandle>(
      NativeCreateSdpObserverHandle{
          rtc::make_ref_counted<NativeCreateSdpObserver>(std::move(observer))});
}

// SetLocalSdpObserver

NativeSetLocalSdpObserver::NativeSetLocalSdpObserver(
    rust::Box<SetLocalSdpObserverWrapper> observer)
    : observer_(std::move(observer)) {}

void NativeSetLocalSdpObserver::OnSetLocalDescriptionComplete(
    webrtc::RTCError error) {
  observer_->on_set_local_description_complete(to_error(error));
}

std::unique_ptr<NativeSetLocalSdpObserverHandle>
create_native_set_local_sdp_observer(
    rust::Box<SetLocalSdpObserverWrapper> observer) {
  return std::make_unique<NativeSetLocalSdpObserverHandle>(
      NativeSetLocalSdpObserverHandle{
          rtc::make_ref_counted<NativeSetLocalSdpObserver>(
              std::move(observer))});
}

// SetRemoteSdpObserver

NativeSetRemoteSdpObserver::NativeSetRemoteSdpObserver(
    rust::Box<SetRemoteSdpObserverWrapper> observer)
    : observer_(std::move(observer)) {}

void NativeSetRemoteSdpObserver::OnSetRemoteDescriptionComplete(
    webrtc::RTCError error) {
  observer_->on_set_remote_description_complete(to_error(error));
}

std::unique_ptr<NativeSetRemoteSdpObserverHandle>
create_native_set_remote_sdp_observer(
    rust::Box<SetRemoteSdpObserverWrapper> observer) {
  return std::make_unique<NativeSetRemoteSdpObserverHandle>(
      NativeSetRemoteSdpObserverHandle{
          rtc::make_ref_counted<NativeSetRemoteSdpObserver>(
              std::move(observer))});
}

}  // namespace livekit