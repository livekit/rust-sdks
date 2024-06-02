#include "livekit/peer.h"

#include <iostream>
#include <memory>

#include "api/audio_codecs/builtin_audio_decoder_factory.h"
#include "api/audio_codecs/builtin_audio_encoder_factory.h"
#include "api/jsep.h"
#include "api/make_ref_counted.h"
#include "api/peer_connection_interface.h"
#include "api/rtc_error.h"
#include "api/rtc_event_log/rtc_event_log_factory.h"
#include "api/scoped_refptr.h"
#include "api/set_local_description_observer_interface.h"
#include "api/set_remote_description_observer_interface.h"
#include "api/task_queue/default_task_queue_factory.h"
#include "livekit/audio_device.h"
#include "livekit/capi.h"
#include "livekit/data_channel.h"
#include "livekit/transceiver.h"
#include "livekit/utils.h"
#include "livekit/video_decoder.h"
#include "livekit/video_encoder.h"
#include "media/engine/webrtc_media_engine.h"
#include "rtc_base/ssl_adapter.h"
#include "rtc_base/thread.h"

#ifdef WEBRTC_WIN
#include "rtc_base/win32.h"
#include "rtc_base/win32_socket_init.h"
#endif

namespace livekit {

class SetRemoteSdpObserver
    : public webrtc::SetRemoteDescriptionObserverInterface {
 public:
  SetRemoteSdpObserver(const lkSetSdpObserver* observer, void* userdata)
      : observer_(observer), userdata_(userdata) {}

  void OnSetRemoteDescriptionComplete(webrtc::RTCError error) override {
    if (error.ok()) {
      observer_->onSuccess(userdata_);
    } else {
      lkRtcError err = toRtcError(error);
      observer_->onFailure(&err, userdata_);
    }
  }

 private:
  const lkSetSdpObserver* observer_;
  void* userdata_;
};

class SetLocalSdpObserver
    : public webrtc::SetLocalDescriptionObserverInterface {
 public:
  SetLocalSdpObserver(const lkSetSdpObserver* observer, void* userdata)
      : observer_(observer), userdata_(userdata) {}

  void OnSetLocalDescriptionComplete(webrtc::RTCError error) override {
    if (error.ok()) {
      observer_->onSuccess(userdata_);
    } else {
      lkRtcError err = toRtcError(error);
      observer_->onFailure(&err, userdata_);
    }
  }

 private:
  const lkSetSdpObserver* observer_;
  void* userdata_;
};

class CreateSdpObserver : public webrtc::CreateSessionDescriptionObserver {
 public:
  CreateSdpObserver(const lkCreateSdpObserver* observer, void* userdata)
      : observer_(observer), userdata_(userdata) {}

  void OnSuccess(webrtc::SessionDescriptionInterface* desc) override {
    std::string sdp;
    desc->ToString(&sdp);
    observer_->onSuccess(static_cast<lkSdpType>(desc->GetType()), sdp.c_str(),
                         userdata_);
  }

  void OnFailure(webrtc::RTCError error) override {
    lkRtcError err = toRtcError(error);
    observer_->onFailure(&err, userdata_);
  }

 private:
  const lkCreateSdpObserver* observer_;
  void* userdata_;
};

void PeerObserver::OnSignalingChange(
    webrtc::PeerConnectionInterface::SignalingState new_state) {
  observer_->onSignalingChange(static_cast<lkSignalingState>(new_state),
                               userdata_);
}

void PeerObserver::OnDataChannel(
    rtc::scoped_refptr<webrtc::DataChannelInterface> data_channel) {
  rtc::scoped_refptr<DataChannel> lkDc =
      rtc::make_ref_counted<DataChannel>(data_channel);
  observer_->onDataChannel(reinterpret_cast<lkDataChannel*>(lkDc.get()),
                           userdata_);
}

void PeerObserver::OnIceGatheringChange(
    webrtc::PeerConnectionInterface::IceGatheringState new_state) {}

void PeerObserver::OnIceCandidate(
    const webrtc::IceCandidateInterface* candidate) {
  std::string sdp;
  candidate->ToString(&sdp);

  lkIceCandidate lkCandidate{};
  lkCandidate.sdpMid = candidate->sdp_mid().c_str();
  lkCandidate.sdpMLineIndex = candidate->sdp_mline_index();
  lkCandidate.sdp = sdp.c_str();

  observer_->onIceCandidate(&lkCandidate, userdata_);
}

void PeerObserver::OnTrack(
    rtc::scoped_refptr<webrtc::RtpTransceiverInterface> transceiver) {
  rtc::scoped_refptr<RtpTransceiver> lkTransceiver =
      rtc::make_ref_counted<RtpTransceiver>(transceiver);

  observer_->onTrack(reinterpret_cast<lkRtpTransceiver*>(lkTransceiver.get()),
                     userdata_);
}

void PeerObserver::OnConnectionChange(
    webrtc::PeerConnectionInterface::PeerConnectionState new_state) {
  observer_->onConnectionChange(static_cast<lkPeerState>(new_state), userdata_);
}

void PeerObserver::OnIceCandidateError(const std::string& address,
                                       int port,
                                       const std::string& url,
                                       int error_code,
                                       const std::string& error_text) {
  observer_->onIceCandidateError(address.c_str(), port, url.c_str(), error_code,
                                 error_text.c_str(), userdata_);
}

PeerFactory::PeerFactory() {
  network_thread_ = rtc::Thread::CreateWithSocketServer();
  network_thread_->SetName("lk_network_thread", &network_thread_);
  network_thread_->Start();
  worker_thread_ = rtc::Thread::Create();
  worker_thread_->SetName("lk_worker_thread", &worker_thread_);
  worker_thread_->Start();
  signaling_thread_ = rtc::Thread::Create();
  signaling_thread_->SetName("lk_signaling_thread", &signaling_thread_);
  signaling_thread_->Start();

  webrtc::PeerConnectionFactoryDependencies dependencies;
  dependencies.network_thread = network_thread_.get();
  dependencies.worker_thread = worker_thread_.get();
  dependencies.signaling_thread = signaling_thread_.get();
  dependencies.socket_factory = network_thread_->socketserver();
  dependencies.task_queue_factory = webrtc::CreateDefaultTaskQueueFactory();
  dependencies.event_log_factory = std::make_unique<webrtc::RtcEventLogFactory>(
      dependencies.task_queue_factory.get());
  dependencies.call_factory = webrtc::CreateCallFactory();
  dependencies.trials = std::make_unique<webrtc::FieldTrialBasedConfig>();

  cricket::MediaEngineDependencies media_deps;
  media_deps.task_queue_factory = dependencies.task_queue_factory.get();

  audio_device_ = worker_thread_->BlockingCall([&] {
    return rtc::make_ref_counted<livekit::AudioDevice>(
        media_deps.task_queue_factory);
  });

  media_deps.adm = audio_device_;
  media_deps.video_encoder_factory =
      std::make_unique<livekit::VideoEncoderFactory>();
  media_deps.video_decoder_factory =
      std::make_unique<livekit::VideoDecoderFactory>();
  media_deps.audio_encoder_factory = webrtc::CreateBuiltinAudioEncoderFactory();
  media_deps.audio_decoder_factory = webrtc::CreateBuiltinAudioDecoderFactory();
  media_deps.audio_processing = webrtc::AudioProcessingBuilder().Create();
  media_deps.trials = dependencies.trials.get();

  dependencies.media_engine = cricket::CreateMediaEngine(std::move(media_deps));

  peer_factory_ =
      webrtc::CreateModularPeerConnectionFactory(std::move(dependencies));

  if (!peer_factory_) {
    RTC_LOG_ERR(LS_ERROR) << "Failed to create PeerConnectionFactory";
    return;
  }
}

PeerFactory::~PeerFactory() {
  peer_factory_ = nullptr;
  audio_device_ = nullptr;
  worker_thread_->Stop();
  signaling_thread_->Stop();
  network_thread_->Stop();
}

rtc::scoped_refptr<Peer> PeerFactory::CreatePeer(
    const lkRtcConfiguration* config,
    const lkPeerObserver* observer,
    void* userdata) {
  rtc::scoped_refptr<PeerObserver> obs =
      rtc::make_ref_counted<PeerObserver>(observer, userdata);
  webrtc::PeerConnectionInterface::RTCConfiguration rtcConfig =
      toNativeConfig(*config);

  webrtc::PeerConnectionDependencies deps{obs.get()};
  webrtc::RTCErrorOr<rtc::scoped_refptr<webrtc::PeerConnectionInterface>> res =
      peer_factory_->CreatePeerConnectionOrError(rtcConfig, std::move(deps));

  if (!res.ok()) {
    RTC_LOG_ERR(LS_ERROR) << "Failed to create PeerConnection: "
                          << res.error().message();
    return nullptr;
  }

  return rtc::make_ref_counted<Peer>(res.value(), obs);
}

rtc::scoped_refptr<DataChannel> Peer::CreateDataChannel(
    const char* label,
    const lkDataChannelInit* init) {
  webrtc::DataChannelInit dcInit = toNativeDataChannelInit(*init);

  webrtc::RTCErrorOr<rtc::scoped_refptr<webrtc::DataChannelInterface>> res =
      peer_connection_->CreateDataChannelOrError(label, &dcInit);

  if (!res.ok()) {
    RTC_LOG_ERR(LS_ERROR) << "Failed to create DataChannel: "
                          << res.error().message();
    return nullptr;
  }

  return rtc::make_ref_counted<DataChannel>(res.value());
}

bool Peer::AddIceCandidate(const lkIceCandidate* candidate,
                           void (*onComplete)(lkRtcError* error,
                                              void* userdata),
                           void* userdata) {
  webrtc::SdpParseError error{};
  std::unique_ptr<webrtc::IceCandidateInterface> c =
      std::unique_ptr<webrtc::IceCandidateInterface>(webrtc::CreateIceCandidate(
          candidate->sdpMid, candidate->sdpMLineIndex, candidate->sdp, &error));

  if (!c) {
    RTC_LOG_ERR(LS_ERROR) << "Failed to parse SDP: " << error.line << " - "
                          << error.description;
    return false;
  }

  peer_connection_->AddIceCandidate(std::move(c), [&](webrtc::RTCError err) {
    if (err.ok()) {
      onComplete(nullptr, userdata);
    } else {
      lkRtcError lkErr = toRtcError(err);
      onComplete(&lkErr, userdata);
    }
  });
  return true;
}

bool Peer::SetLocalDescription(lkSdpType type,
                               const char* sdp,
                               const lkSetSdpObserver* observer,
                               void* userdata) {
  webrtc::SdpParseError error{};
  std::unique_ptr<webrtc::SessionDescriptionInterface> desc =
      webrtc::CreateSessionDescription(static_cast<webrtc::SdpType>(type), sdp,
                                       &error);

  if (!desc) {
    RTC_LOG_ERR(LS_ERROR) << "Failed to parse SDP: " << error.line << " - "
                          << error.description;
    return false;
  }

  rtc::scoped_refptr<SetLocalSdpObserver> setSdpObserver =
      rtc::make_ref_counted<SetLocalSdpObserver>(observer, userdata);

  peer_connection_->SetLocalDescription(std::move(desc),
                                        std::move(setSdpObserver));
  return true;
}

bool Peer::SetRemoteDescription(lkSdpType type,
                                const char* sdp,
                                const lkSetSdpObserver* observer,
                                void* userdata) {
  webrtc::SdpParseError error{};
  std::unique_ptr<webrtc::SessionDescriptionInterface> desc =
      webrtc::CreateSessionDescription(static_cast<webrtc::SdpType>(type), sdp,
                                       &error);

  if (!desc) {
    RTC_LOG_ERR(LS_ERROR) << "Failed to parse SDP: " << error.line << " - "
                          << error.description;
    return false;
  }

  rtc::scoped_refptr<SetRemoteSdpObserver> setSdpObserver =
      rtc::make_ref_counted<SetRemoteSdpObserver>(observer, userdata);

  peer_connection_->SetRemoteDescription(std::move(desc),
                                         std::move(setSdpObserver));
  return true;
}

bool Peer::CreateOffer(const lkOfferAnswerOptions& options,
                       const lkCreateSdpObserver* observer,
                       void* userdata) {
  webrtc::PeerConnectionInterface::RTCOfferAnswerOptions rtcOptions =
      toNativeOfferAnswerOptions(options);

  rtc::scoped_refptr<webrtc::CreateSessionDescriptionObserver>
      createSdpObserver =
          rtc::make_ref_counted<CreateSdpObserver>(observer, userdata);

  peer_connection_->CreateOffer(createSdpObserver.get(), rtcOptions);
  return true;
}

bool Peer::CreateAnswer(const lkOfferAnswerOptions& options,
                        const lkCreateSdpObserver* observer,
                        void* userdata) {
  webrtc::PeerConnectionInterface::RTCOfferAnswerOptions rtcOptions =
      toNativeOfferAnswerOptions(options);

  rtc::scoped_refptr<webrtc::CreateSessionDescriptionObserver>
      createSdpObserver =
          rtc::make_ref_counted<CreateSdpObserver>(observer, userdata);

  peer_connection_->CreateAnswer(createSdpObserver.get(), rtcOptions);
  return true;
}

bool Peer::SetConfig(const lkRtcConfiguration* config) {
  webrtc::PeerConnectionInterface::RTCConfiguration rtcConfig =
      toNativeConfig(*config);
  webrtc::RTCError err = peer_connection_->SetConfiguration(rtcConfig);
  if (!err.ok()) {
    RTC_LOG_ERR(LS_ERROR) << "Failed to set configuration: " << err.message();
    return false;
  }
  return true;
}

bool Peer::Close() {
  peer_connection_->Close();
  return true;
}

webrtc::PeerConnectionInterface::RTCConfiguration toNativeConfig(
    const lkRtcConfiguration& config) {
  webrtc::PeerConnectionInterface::RTCConfiguration rtc_config{};

  for (int i = 0; i < config.iceServersCount; ++i) {
    const lkIceServer& s = config.iceServers[i];

    webrtc::PeerConnectionInterface::IceServer ice_server;
    ice_server.username = s.username;
    ice_server.password = s.password;

    for (int j = 0; j < s.urlsCount; ++j)
      ice_server.urls.emplace_back(s.urls[j]);

    rtc_config.servers.push_back(ice_server);
  }

  rtc_config.continual_gathering_policy =
      static_cast<webrtc::PeerConnectionInterface::ContinualGatheringPolicy>(
          config.gatheringPolicy);

  rtc_config.type =
      static_cast<webrtc::PeerConnectionInterface::IceTransportsType>(
          config.iceTransportType);

  return rtc_config;
}

}  // namespace livekit
