//
// Created by Théo Monnom on 03/08/2022.
//

#pragma once

#include "api/peer_connection_interface.h"
#include "peer_connection.h"
#include "webrtc.h"

namespace livekit {
using NativeRTCConfiguration =
    webrtc::PeerConnectionInterface::RTCConfiguration;

class PeerConnectionFactory;
}  // namespace livekit
#include "webrtc-sys/src/peer_connection_factory.rs.h"

namespace livekit {

class PeerConnectionFactory {
 public:
  explicit PeerConnectionFactory(std::shared_ptr<RTCRuntime> rtc_runtime);
  ~PeerConnectionFactory();

  std::unique_ptr<PeerConnection> create_peer_connection(
      std::unique_ptr<NativeRTCConfiguration> config,
      NativePeerConnectionObserver& observer) const;

 private:
  std::shared_ptr<RTCRuntime> rtc_runtime_;
  rtc::scoped_refptr<webrtc::PeerConnectionFactoryInterface> peer_factory_;
};

std::unique_ptr<PeerConnectionFactory> create_peer_connection_factory(
    std::shared_ptr<RTCRuntime> rtc_runtime);
std::unique_ptr<NativeRTCConfiguration> create_rtc_configuration(
    RTCConfiguration conf);
}  // namespace livekit
