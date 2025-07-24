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

#include <memory>
#include <utility>

#include "api/audio_codecs/builtin_audio_decoder_factory.h"
#include "api/audio_codecs/builtin_audio_encoder_factory.h"
#include "api/audio/echo_canceller3_factory.h"
#include "api/audio/echo_canceller3_config.h"
#include "api/peer_connection_interface.h"
#include "api/rtc_error.h"
#include "api/enable_media.h"
#include "api/rtc_event_log/rtc_event_log_factory.h"
#include "api/task_queue/default_task_queue_factory.h"
#include "api/video_codecs/builtin_video_decoder_factory.h"
#include "api/video_codecs/builtin_video_encoder_factory.h"
#include "livekit/audio_device.h"
#include "livekit/audio_track.h"
#include "livekit/peer_connection.h"
#include "livekit/rtc_error.h"
#include "livekit/rtp_parameters.h"
#include "livekit/video_decoder_factory.h"
#include "livekit/video_encoder_factory.h"
#include "livekit/webrtc.h"
#include "rtc_base/thread.h"
#include "webrtc-sys/src/peer_connection.rs.h"
#include "webrtc-sys/src/peer_connection_factory.rs.h"

namespace livekit {

class PeerConnectionObserver;

PeerConnectionFactory::PeerConnectionFactory(
    std::shared_ptr<RtcRuntime> rtc_runtime)
    : rtc_runtime_(rtc_runtime), 
    audio_context_(rtc_runtime) {
  RTC_LOG(LS_VERBOSE) << "PeerConnectionFactory::PeerConnectionFactory()";

  webrtc::PeerConnectionFactoryDependencies dependencies;
  dependencies.network_thread = rtc_runtime_->network_thread();
  dependencies.worker_thread = rtc_runtime_->worker_thread();
  dependencies.signaling_thread = rtc_runtime_->signaling_thread();
  dependencies.socket_factory = rtc_runtime_->network_thread()->socketserver();
  dependencies.task_queue_factory = webrtc::CreateDefaultTaskQueueFactory();
  dependencies.event_log_factory = std::make_unique<webrtc::RtcEventLogFactory>();
  dependencies.trials = std::make_unique<webrtc::FieldTrialBasedConfig>();

  cricket::MediaEngineDependencies media_deps;
  media_deps.task_queue_factory = dependencies.task_queue_factory.get();

  media_deps.adm = audio_context_.audio_device(media_deps.task_queue_factory);

  dependencies.video_encoder_factory =
      std::move(std::make_unique<livekit::VideoEncoderFactory>());
  dependencies.video_decoder_factory =
      std::move(std::make_unique<livekit::VideoDecoderFactory>());
  media_deps.audio_encoder_factory = webrtc::CreateBuiltinAudioEncoderFactory();
  media_deps.audio_decoder_factory = webrtc::CreateBuiltinAudioDecoderFactory();

  auto apm = webrtc::AudioProcessingBuilder();
  auto cfg = webrtc::EchoCanceller3Config();
  auto echo_control = std::make_unique<webrtc::EchoCanceller3Factory>(cfg);

  apm.SetEchoControlFactory(std::move(echo_control));
  media_deps.audio_processing = apm.Create();

  media_deps.audio_mixer = audio_context_.audio_mixer();

  media_deps.trials = dependencies.trials.get();

  dependencies.media_engine = cricket::CreateMediaEngine(std::move(media_deps));

  webrtc::EnableMedia(dependencies);
  peer_factory_ =
      webrtc::CreateModularPeerConnectionFactory(std::move(dependencies));


  task_queue_factory_ = dependencies.task_queue_factory.get();

  if (peer_factory_.get() == nullptr) {
    RTC_LOG_ERR(LS_ERROR) << "Failed to create PeerConnectionFactory";
    return;
  }
}

PeerConnectionFactory::~PeerConnectionFactory() {
  RTC_LOG(LS_VERBOSE) << "PeerConnectionFactory::~PeerConnectionFactory()";

  peer_factory_ = nullptr;
}

std::shared_ptr<PeerConnection> PeerConnectionFactory::create_peer_connection(
    RtcConfiguration config,
    rust::Box<PeerConnectionObserverWrapper> observer) const {
  std::shared_ptr<PeerConnection> pc = std::make_shared<PeerConnection>(
      rtc_runtime_, peer_factory_, std::move(observer));

  if (!pc->Initialize(to_native_rtc_configuration(config))) {
    throw std::runtime_error(serialize_error(to_error(webrtc::RTCError(
        webrtc::RTCErrorType::INTERNAL_ERROR, "failed to initialize pc"))));
  }

  return pc;
}

std::shared_ptr<VideoTrack> PeerConnectionFactory::create_video_track(
    rust::String label,
    std::shared_ptr<VideoTrackSource> source) const {
  return std::static_pointer_cast<VideoTrack>(
      rtc_runtime_->get_or_create_media_stream_track(
          peer_factory_->CreateVideoTrack(source->get(), label.c_str())));
}

std::shared_ptr<AudioTrack> PeerConnectionFactory::create_audio_track(
    rust::String label,
    std::shared_ptr<AudioTrackSource> source) const {
  return std::static_pointer_cast<AudioTrack>(
      rtc_runtime_->get_or_create_media_stream_track(
          peer_factory_->CreateAudioTrack(label.c_str(), source->get().get())));
}

RtpCapabilities PeerConnectionFactory::rtp_sender_capabilities(
    MediaType type) const {
  return to_rust_rtp_capabilities(peer_factory_->GetRtpSenderCapabilities(
      static_cast<cricket::MediaType>(type)));
}

RtpCapabilities PeerConnectionFactory::rtp_receiver_capabilities(
    MediaType type) const {
  return to_rust_rtp_capabilities(peer_factory_->GetRtpReceiverCapabilities(
      static_cast<cricket::MediaType>(type)));
}

std::shared_ptr<PeerConnectionFactory> create_peer_connection_factory() {
  return std::make_shared<PeerConnectionFactory>(RtcRuntime::create());
}

}  // namespace livekit
