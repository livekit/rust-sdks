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

#include "livekit/peer_connection_factory.h"

#include <memory>
#include <utility>
#include <vector>

#include "api/audio_codecs/builtin_audio_decoder_factory.h"
#include "api/audio_codecs/builtin_audio_encoder_factory.h"
#include "api/audio/builtin_audio_processing_builder.h"
#include "api/create_modular_peer_connection_factory.h"
#include "api/environment/environment_factory.h"
#include "api/field_trials_view.h"
#include "api/peer_connection_interface.h"
#include "api/rtc_error.h"
#include "api/enable_media.h"
#include "api/rtc_event_log/rtc_event_log_factory.h"
#include "api/task_queue/default_task_queue_factory.h"
#include "api/video_codecs/builtin_video_decoder_factory.h"
#include "api/video_codecs/builtin_video_encoder_factory.h"
#include "api/audio/audio_device.h"
#include "api/audio_options.h"
#include "livekit/adm_proxy.h"
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

namespace livekit_ffi {
namespace {

constexpr char kForcePlayoutDelayFieldTrial[] =
    "WebRTC-ForcePlayoutDelay/min_ms:0,max_ms:0/";
constexpr char kForcePlayoutDelayValue[] = "min_ms:0,max_ms:0";

class ZeroPlayoutDelayFieldTrials final : public webrtc::FieldTrialsView {
 public:
  std::string Lookup(absl::string_view key) const override {
    return key == "WebRTC-ForcePlayoutDelay" ? kForcePlayoutDelayValue : "";
  }

  std::unique_ptr<webrtc::FieldTrialsView> CreateCopy() const override {
    return std::make_unique<ZeroPlayoutDelayFieldTrials>();
  }
};

// Enables SPED (DTLS-in-STUN) via the WebRTC-IceHandshakeDtls field trial.
// SNAP (SCTP-INIT-in-SDP) is intentionally NOT set here: it maps to the
// immutable enable_sctp_snap RTCConfiguration field, so it must be carried on
// the RtcConfiguration (see RtcConfiguration.enable_sctp_snap) to stay
// consistent across create + set_configuration. Enabling it via a field trial
// makes set_configuration fail ("Modifying the configuration in an unsupported
// way").
class EnableWarpFieldTrials final : public webrtc::FieldTrialsView {
 public:
  std::string Lookup(absl::string_view key) const override {
    if (key == "WebRTC-IceHandshakeDtls") {
      return "Enabled";
    }
    return "";
  }

  std::unique_ptr<webrtc::FieldTrialsView> CreateCopy() const override {
    return std::make_unique<EnableWarpFieldTrials>();
  }
};

// An Environment accepts a single FieldTrialsView, so to enable several
// independent trial groups (e.g. zero-playout-delay AND WARP) we combine their
// views into one: Lookup delegates to each sub-view and returns the first
// non-empty result. The groups' keys are disjoint, so ordering is irrelevant.
class CompositeFieldTrials final : public webrtc::FieldTrialsView {
 public:
  explicit CompositeFieldTrials(
      std::vector<std::unique_ptr<webrtc::FieldTrialsView>> views)
      : views_(std::move(views)) {}

  std::string Lookup(absl::string_view key) const override {
    for (const auto& view : views_) {
      std::string value = view->Lookup(key);
      if (!value.empty()) {
        return value;
      }
    }
    return "";
  }

  std::unique_ptr<webrtc::FieldTrialsView> CreateCopy() const override {
    std::vector<std::unique_ptr<webrtc::FieldTrialsView>> copies;
    copies.reserve(views_.size());
    for (const auto& view : views_) {
      copies.push_back(view->CreateCopy());
    }
    return std::make_unique<CompositeFieldTrials>(std::move(copies));
  }

 private:
  std::vector<std::unique_ptr<webrtc::FieldTrialsView>> views_;
};

// zero_playout_delay and enable_warp are independent and may both be enabled;
// their field-trial views are composed into one.
webrtc::Environment CreateEnvironment(bool zero_playout_delay,
                                      bool enable_warp) {
  std::vector<std::unique_ptr<webrtc::FieldTrialsView>> views;
  if (zero_playout_delay) {
    views.push_back(std::make_unique<ZeroPlayoutDelayFieldTrials>());
  }
  if (enable_warp) {
    views.push_back(std::make_unique<EnableWarpFieldTrials>());
  }

  if (views.empty()) {
    return webrtc::CreateEnvironment();
  }
  return webrtc::CreateEnvironment(
      std::make_unique<CompositeFieldTrials>(std::move(views)));
}

}  // namespace

class PeerConnectionObserver;

PeerConnectionFactory::PeerConnectionFactory(
    std::shared_ptr<RtcRuntime> rtc_runtime)
    : PeerConnectionFactory(std::move(rtc_runtime), false, false) {}

PeerConnectionFactory::PeerConnectionFactory(
    std::shared_ptr<RtcRuntime> rtc_runtime,
    bool zero_playout_delay)
    : PeerConnectionFactory(std::move(rtc_runtime), zero_playout_delay, false) {}

PeerConnectionFactory::PeerConnectionFactory(
    std::shared_ptr<RtcRuntime> rtc_runtime,
    bool zero_playout_delay,
    bool enable_warp)
    : rtc_runtime_(rtc_runtime),
      env_(CreateEnvironment(zero_playout_delay, enable_warp)) {
  webrtc::PeerConnectionFactoryDependencies dependencies;
  dependencies.network_thread = rtc_runtime_->network_thread();
  dependencies.worker_thread = rtc_runtime_->worker_thread();
  dependencies.signaling_thread = rtc_runtime_->signaling_thread();
  dependencies.socket_factory = rtc_runtime_->network_thread()->socketserver();
  dependencies.event_log_factory = std::make_unique<webrtc::RtcEventLogFactory>();
  dependencies.env = env_;

  if (zero_playout_delay) {
    RTC_LOG(LS_INFO) << "WebRTC zero playout delay enabled with field trial: "
                     << kForcePlayoutDelayFieldTrial;
  }

  if (enable_warp) {
    RTC_LOG(LS_INFO) << "WebRTC WARP: SPED enabled via field trial "
                        "WebRTC-IceHandshakeDtls/Enabled/ (SNAP via "
                        "RtcConfiguration.enable_sctp_snap)";
  }

  // Create AdmProxy - it creates and initializes Platform ADM internally
  adm_proxy_ = rtc_runtime_->worker_thread()->BlockingCall([&] {
    return webrtc::make_ref_counted<livekit_ffi::AdmProxy>(
        env_, rtc_runtime_->worker_thread());
  });
  audio_device_ = std::make_shared<AudioDeviceController>(adm_proxy_);

  dependencies.adm = adm_proxy_;

  dependencies.video_encoder_factory =
      std::move(std::make_unique<livekit_ffi::VideoEncoderFactory>());
  dependencies.video_decoder_factory =
      std::move(std::make_unique<livekit_ffi::VideoDecoderFactory>());
  dependencies.audio_encoder_factory = webrtc::CreateBuiltinAudioEncoderFactory();
  dependencies.audio_decoder_factory = webrtc::CreateBuiltinAudioDecoderFactory();
  dependencies.audio_processing_builder = std::make_unique<webrtc::BuiltinAudioProcessingBuilder>();

  webrtc::EnableMedia(dependencies);
  peer_factory_ =
      webrtc::CreateModularPeerConnectionFactory(std::move(dependencies));

  if (peer_factory_.get() == nullptr) {
    RTC_LOG_ERR(LS_ERROR) << "Failed to create PeerConnectionFactory";
    return;
  }
}

PeerConnectionFactory::~PeerConnectionFactory() {
  RTC_LOG(LS_VERBOSE) << "PeerConnectionFactory::~PeerConnectionFactory()";

  peer_factory_ = nullptr;
  audio_device_ = nullptr;
  rtc_runtime_->worker_thread()->BlockingCall(
      [this] { adm_proxy_ = nullptr; });
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

std::shared_ptr<AudioTrack> PeerConnectionFactory::create_device_audio_track(
    rust::String label) const {
  // Create an audio source that uses the ADM for capture
  webrtc::AudioOptions audio_options;
  audio_options.echo_cancellation = true;
  audio_options.auto_gain_control = true;
  audio_options.noise_suppression = true;

  webrtc::scoped_refptr<webrtc::AudioSourceInterface> audio_source =
      peer_factory_->CreateAudioSource(audio_options);

  if (!audio_source) {
    RTC_LOG(LS_ERROR) << "Failed to create device audio source";
    return nullptr;
  }

  return std::static_pointer_cast<AudioTrack>(
      rtc_runtime_->get_or_create_media_stream_track(
          peer_factory_->CreateAudioTrack(label.c_str(), audio_source.get())));
}

RtpCapabilities PeerConnectionFactory::rtp_sender_capabilities(
    MediaType type) const {
  return to_rust_rtp_capabilities(peer_factory_->GetRtpSenderCapabilities(
      static_cast<webrtc::MediaType>(type)));
}

RtpCapabilities PeerConnectionFactory::rtp_receiver_capabilities(
    MediaType type) const {
  return to_rust_rtp_capabilities(peer_factory_->GetRtpReceiverCapabilities(
      static_cast<webrtc::MediaType>(type)));
}

std::shared_ptr<AudioDeviceController> PeerConnectionFactory::audio_device() const {
  return audio_device_;
}

bool PeerConnectionFactory::zero_playout_delay_enabled() const {
  return env_.field_trials().Lookup("WebRTC-ForcePlayoutDelay") ==
         kForcePlayoutDelayValue;
}

std::shared_ptr<PeerConnectionFactory> create_peer_connection_factory() {
  return std::make_shared<PeerConnectionFactory>(RtcRuntime::create());
}

std::shared_ptr<PeerConnectionFactory>
create_peer_connection_factory_with_zero_playout_delay() {
  return std::make_shared<PeerConnectionFactory>(RtcRuntime::create(), true);
}

std::shared_ptr<PeerConnectionFactory>
create_peer_connection_factory_with_options(bool zero_playout_delay,
                                            bool enable_warp) {
  return std::make_shared<PeerConnectionFactory>(RtcRuntime::create(),
                                                 zero_playout_delay,
                                                 enable_warp);
}

}  // namespace livekit_ffi
