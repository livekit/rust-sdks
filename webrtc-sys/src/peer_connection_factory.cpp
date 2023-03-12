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

#include "livekit/peer_connection_factory.h"

#include <utility>

#include "api/audio_codecs/builtin_audio_decoder_factory.h"
#include "api/audio_codecs/builtin_audio_encoder_factory.h"
#include "api/rtc_event_log/rtc_event_log_factory.h"
#include "api/task_queue/default_task_queue_factory.h"
#include "api/video_codecs/builtin_video_decoder_factory.h"
#include "api/video_codecs/builtin_video_encoder_factory.h"
#include "livekit/rtc_error.h"
#include "livekit/rtp_parameters.h"
#include "livekit/video_decoder_factory.h"
#include "livekit/video_encoder_factory.h"
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
  dependencies.socket_factory = rtc_runtime_->network_thread()->socketserver();
  dependencies.task_queue_factory = webrtc::CreateDefaultTaskQueueFactory();
  dependencies.event_log_factory = std::make_unique<webrtc::RtcEventLogFactory>(
      dependencies.task_queue_factory.get());
  dependencies.call_factory = webrtc::CreateCallFactory();
  dependencies.trials = std::make_unique<webrtc::FieldTrialBasedConfig>();

  cricket::MediaEngineDependencies media_deps;
  media_deps.task_queue_factory = dependencies.task_queue_factory.get();
  media_deps.video_encoder_factory =
      std::move(std::make_unique<livekit::VideoEncoderFactory>());
  media_deps.video_decoder_factory =
      std::move(std::make_unique<livekit::VideoDecoderFactory>());
  media_deps.audio_encoder_factory = webrtc::CreateBuiltinAudioEncoderFactory();
  media_deps.audio_decoder_factory = webrtc::CreateBuiltinAudioDecoderFactory();
  media_deps.audio_processing = webrtc::AudioProcessingBuilder().Create();
  media_deps.trials = dependencies.trials.get();

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

std::shared_ptr<PeerConnection> PeerConnectionFactory::create_peer_connection(
    std::unique_ptr<webrtc::PeerConnectionInterface::RTCConfiguration> config,
    NativePeerConnectionObserver* observer) const {
  webrtc::PeerConnectionDependencies deps{observer};
  auto result =
      peer_factory_->CreatePeerConnectionOrError(*config, std::move(deps));

  if (!result.ok()) {
    throw std::runtime_error(serialize_error(to_error(result.error())));
  }

  return std::make_shared<PeerConnection>(rtc_runtime_, result.value());
}

std::shared_ptr<VideoTrack> PeerConnectionFactory::create_video_track(
    rust::String label,
    std::shared_ptr<AdaptedVideoTrackSource> source) const {
  return std::make_shared<VideoTrack>(
      peer_factory_->CreateVideoTrack(label.c_str(), source->get().get()));
}

RtpCapabilities PeerConnectionFactory::get_rtp_sender_capabilities(
    MediaType type) const {
  return to_rust_rtp_capabilities(peer_factory_->GetRtpSenderCapabilities(
      static_cast<cricket::MediaType>(type)));
}

RtpCapabilities PeerConnectionFactory::get_rtp_receiver_capabilities(
    MediaType type) const {
  return to_rust_rtp_capabilities(peer_factory_->GetRtpReceiverCapabilities(
      static_cast<cricket::MediaType>(type)));
}

std::shared_ptr<PeerConnectionFactory> create_peer_connection_factory(
    std::shared_ptr<RTCRuntime> rtc_runtime) {
  return std::make_shared<PeerConnectionFactory>(std::move(rtc_runtime));
}

std::unique_ptr<NativeRTCConfiguration> create_rtc_configuration(
    RTCConfiguration conf) {
  auto rtc =
      std::make_unique<webrtc::PeerConnectionInterface::RTCConfiguration>();

  for (auto item : conf.ice_servers) {
    webrtc::PeerConnectionInterface::IceServer ice_server;
    ice_server.username = item.username.c_str();
    ice_server.password = item.password.c_str();

    for (auto url : item.urls) {
      ice_server.urls.emplace_back(url.c_str());
    }

    rtc->servers.push_back(ice_server);
  }

  rtc->continual_gathering_policy =
      static_cast<webrtc::PeerConnectionInterface::ContinualGatheringPolicy>(
          conf.continual_gathering_policy);

  rtc->type = static_cast<webrtc::PeerConnectionInterface::IceTransportsType>(
      conf.ice_transport_type);

  return rtc;
}
}  // namespace livekit
