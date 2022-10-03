//
// Created by Théo Monnom on 03/08/2022.
//

#include "livekit/peer_connection_factory.h"

#include <utility>

#include "api/audio_codecs/builtin_audio_decoder_factory.h"
#include "api/audio_codecs/builtin_audio_encoder_factory.h"
#include "api/rtc_event_log/rtc_event_log_factory.h"
#include "api/task_queue/default_task_queue_factory.h"
#include "api/video_codecs/builtin_video_decoder_factory.h"
#include "api/video_codecs/builtin_video_encoder_factory.h"
#include "libwebrtc-sys/src/peer_connection_factory.rs.h"
#include "livekit/rtc_error.h"
#include "media/engine/webrtc_media_engine.h"

namespace livekit {

PeerConnectionFactory::PeerConnectionFactory(
    std::shared_ptr<RTCRuntime> rtc_runtime)
    : rtc_runtime_(std::move(rtc_runtime)) {
  RTC_LOG(LS_INFO) << "PeerConnectionFactory::PeerConnectionFactory()";

  webrtc::PeerConnectionFactoryDependencies dependencies;
  dependencies.network_thread = rtc_runtime_->network_thread();
  dependencies.worker_thread = rtc_runtime_->worker_thread();
  dependencies.signaling_thread = rtc_runtime_->signaling_thread();
  dependencies.task_queue_factory = webrtc::CreateDefaultTaskQueueFactory();
  dependencies.event_log_factory = std::make_unique<webrtc::RtcEventLogFactory>(
      dependencies.task_queue_factory.get());

  cricket::MediaEngineDependencies media_deps;
  media_deps.task_queue_factory = dependencies.task_queue_factory.get();
  media_deps.video_encoder_factory = webrtc::CreateBuiltinVideoEncoderFactory();
  media_deps.video_decoder_factory = webrtc::CreateBuiltinVideoDecoderFactory();
  media_deps.audio_encoder_factory = webrtc::CreateBuiltinAudioEncoderFactory();
  media_deps.audio_decoder_factory = webrtc::CreateBuiltinAudioDecoderFactory();

  dependencies.media_engine = cricket::CreateMediaEngine(std::move(media_deps));

  peer_factory_ =
      webrtc::CreateModularPeerConnectionFactory(std::move(dependencies));

  if (peer_factory_.get() == nullptr) {
    RTC_LOG_ERR(LS_ERROR) << "Failed to create PeerConnectionFactory";
    return;
  }
}

PeerConnectionFactory::~PeerConnectionFactory() {
  RTC_LOG(LS_INFO) << "PeerConnectionFactory::~PeerConnectionFactory()";
}

std::unique_ptr<PeerConnection> PeerConnectionFactory::create_peer_connection(
    std::unique_ptr<webrtc::PeerConnectionInterface::RTCConfiguration> config,
    NativePeerConnectionObserver& observer) const {
  webrtc::PeerConnectionDependencies deps{&observer};
  auto result =
      peer_factory_->CreatePeerConnectionOrError(*config, std::move(deps));

  if (!result.ok()) {
    throw std::runtime_error(serialize_error(to_error(result.error())));
  }

  return std::make_unique<PeerConnection>(rtc_runtime_, result.value());
}

std::unique_ptr<PeerConnectionFactory> create_peer_connection_factory(
    std::shared_ptr<RTCRuntime> rtc_runtime) {
  return std::make_unique<PeerConnectionFactory>(std::move(rtc_runtime));
}

std::unique_ptr<NativeRTCConfiguration> create_rtc_configuration(
    RTCConfiguration conf) {
  auto rtc =
      std::make_unique<webrtc::PeerConnectionInterface::RTCConfiguration>();
  for (auto& item : conf.ice_servers) {
    webrtc::PeerConnectionInterface::IceServer ice_server;
    ice_server.username = item.username.c_str();
    ice_server.password = item.password.c_str();

    for (auto& url : item.urls) {
      ice_server.urls.emplace_back(url.c_str());
    }
    rtc->servers.push_back(ice_server);
    rtc->continual_gathering_policy =
        static_cast<webrtc::PeerConnectionInterface::ContinualGatheringPolicy>(
            conf.continual_gathering_policy);

    rtc->type = static_cast<webrtc::PeerConnectionInterface::IceTransportsType>(
        conf.ice_transport_type);
  }

  return rtc;
}
}  // namespace livekit