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

#include "api/audio_codecs/builtin_audio_decoder_factory.h"
#include "api/audio_codecs/builtin_audio_encoder_factory.h"
#include "api/audio/builtin_audio_processing_builder.h"
#include "api/create_modular_peer_connection_factory.h"
#include "api/environment/environment_factory.h"
#include "api/peer_connection_interface.h"
#include "api/rtc_error.h"
#include "api/enable_media.h"
#include "api/rtc_event_log/rtc_event_log_factory.h"
#include "api/task_queue/default_task_queue_factory.h"
#include "api/video_codecs/builtin_video_decoder_factory.h"
#include "api/video_codecs/builtin_video_encoder_factory.h"
#include "api/audio/audio_device.h"
#include "api/audio/create_audio_device_module.h"
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

class PeerConnectionObserver;

PeerConnectionFactory::PeerConnectionFactory(
    std::shared_ptr<RtcRuntime> rtc_runtime)
    : rtc_runtime_(rtc_runtime),
    env_(webrtc::EnvironmentFactory().Create()) {
  RTC_LOG(LS_VERBOSE) << "PeerConnectionFactory::PeerConnectionFactory()";

  webrtc::PeerConnectionFactoryDependencies dependencies;
  dependencies.network_thread = rtc_runtime_->network_thread();
  dependencies.worker_thread = rtc_runtime_->worker_thread();
  dependencies.signaling_thread = rtc_runtime_->signaling_thread();
  dependencies.socket_factory = rtc_runtime_->network_thread()->socketserver();
  dependencies.event_log_factory = std::make_unique<webrtc::RtcEventLogFactory>();

  // Create AdmProxy instead of direct AudioDevice
  adm_proxy_ = rtc_runtime_->worker_thread()->BlockingCall([&] {
    return webrtc::make_ref_counted<livekit_ffi::AdmProxy>(
        env_, rtc_runtime_->worker_thread());
  });

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
  RTC_LOG(LS_INFO) << "PeerConnectionFactory::create_device_audio_track() label=" << label.c_str();

  // Create an audio source that uses the ADM for capture
  // This will use the Platform ADM's recording device
  webrtc::AudioOptions audio_options;
  audio_options.echo_cancellation = true;
  audio_options.auto_gain_control = true;
  audio_options.noise_suppression = true;

  RTC_LOG(LS_INFO) << "Creating audio source with EC=" << audio_options.echo_cancellation.value_or(false)
                   << " AGC=" << audio_options.auto_gain_control.value_or(false)
                   << " NS=" << audio_options.noise_suppression.value_or(false);

  webrtc::scoped_refptr<webrtc::AudioSourceInterface> audio_source =
      peer_factory_->CreateAudioSource(audio_options);

  if (!audio_source) {
    RTC_LOG(LS_ERROR) << "Failed to create device audio source";
    return nullptr;
  }

  RTC_LOG(LS_INFO) << "Audio source created successfully, creating audio track";

  auto track = std::static_pointer_cast<AudioTrack>(
      rtc_runtime_->get_or_create_media_stream_track(
          peer_factory_->CreateAudioTrack(label.c_str(), audio_source.get())));

  RTC_LOG(LS_INFO) << "Device audio track created: " << (track ? "success" : "failed");
  return track;
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

// ADM Management Methods

bool PeerConnectionFactory::enable_platform_adm() const {
  RTC_LOG(LS_INFO) << "PeerConnectionFactory::enable_platform_adm()";

  // Create platform ADM on worker thread
  webrtc::scoped_refptr<webrtc::AudioDeviceModule> platform_adm =
      rtc_runtime_->worker_thread()->BlockingCall([this]()
          -> webrtc::scoped_refptr<webrtc::AudioDeviceModule> {
        auto adm = webrtc::CreateAudioDeviceModule(
            env_, webrtc::AudioDeviceModule::kPlatformDefaultAudio);
        RTC_LOG(LS_INFO) << "CreateAudioDeviceModule returned: " << (adm ? "success" : "null");
        return adm;
      });

  if (!platform_adm) {
    RTC_LOG(LS_ERROR) << "Failed to create platform ADM";
    return false;
  }

  // Initialize platform ADM
  int32_t init_result = platform_adm->Init();
  RTC_LOG(LS_INFO) << "Platform ADM Init() returned: " << init_result;
  if (init_result != 0) {
    RTC_LOG(LS_ERROR) << "Failed to initialize platform ADM";
    return false;
  }

  // Log device counts
  RTC_LOG(LS_INFO) << "Platform ADM recording devices: " << platform_adm->RecordingDevices();
  RTC_LOG(LS_INFO) << "Platform ADM playout devices: " << platform_adm->PlayoutDevices();

  // Set it on the proxy
  adm_proxy_->SetPlatformAdm(platform_adm);
  RTC_LOG(LS_INFO) << "Platform ADM set on proxy successfully";
  return true;
}

void PeerConnectionFactory::clear_adm_delegate() const {
  RTC_LOG(LS_INFO) << "PeerConnectionFactory::clear_adm_delegate()";
  adm_proxy_->ClearDelegate();
}

int32_t PeerConnectionFactory::adm_delegate_type() const {
  return static_cast<int32_t>(adm_proxy_->delegate_type());
}

bool PeerConnectionFactory::has_adm_delegate() const {
  return adm_proxy_->has_delegate();
}

int16_t PeerConnectionFactory::playout_devices() const {
  return adm_proxy_->PlayoutDevices();
}

int16_t PeerConnectionFactory::recording_devices() const {
  return adm_proxy_->RecordingDevices();
}

rust::String PeerConnectionFactory::playout_device_name(uint16_t index) const {
  char name[webrtc::kAdmMaxDeviceNameSize] = {0};
  char guid[webrtc::kAdmMaxGuidSize] = {0};
  adm_proxy_->PlayoutDeviceName(index, name, guid);
  return rust::String(name);
}

rust::String PeerConnectionFactory::recording_device_name(uint16_t index) const {
  char name[webrtc::kAdmMaxDeviceNameSize] = {0};
  char guid[webrtc::kAdmMaxGuidSize] = {0};
  adm_proxy_->RecordingDeviceName(index, name, guid);
  return rust::String(name);
}

int32_t PeerConnectionFactory::set_playout_device(uint16_t index) const {
  return adm_proxy_->SetPlayoutDevice(index);
}

int32_t PeerConnectionFactory::set_recording_device(uint16_t index) const {
  return adm_proxy_->SetRecordingDevice(index);
}

int32_t PeerConnectionFactory::stop_recording() const {
  return adm_proxy_->StopRecording();
}

int32_t PeerConnectionFactory::init_recording() const {
  return adm_proxy_->InitRecording();
}

int32_t PeerConnectionFactory::start_recording() const {
  return adm_proxy_->StartRecording();
}

bool PeerConnectionFactory::recording_is_initialized() const {
  return adm_proxy_->RecordingIsInitialized();
}

int32_t PeerConnectionFactory::stop_playout() const {
  return adm_proxy_->StopPlayout();
}

int32_t PeerConnectionFactory::init_playout() const {
  return adm_proxy_->InitPlayout();
}

int32_t PeerConnectionFactory::start_playout() const {
  return adm_proxy_->StartPlayout();
}

bool PeerConnectionFactory::playout_is_initialized() const {
  return adm_proxy_->PlayoutIsInitialized();
}

std::shared_ptr<PeerConnectionFactory> create_peer_connection_factory() {
  return std::make_shared<PeerConnectionFactory>(RtcRuntime::create());
}

}  // namespace livekit_ffi
