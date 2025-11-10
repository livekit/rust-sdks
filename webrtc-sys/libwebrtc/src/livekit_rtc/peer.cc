#include "livekit_rtc/peer.h"

#include <iostream>
#include <memory>

#include "api/audio/builtin_audio_processing_builder.h"
#include "api/audio_codecs/builtin_audio_decoder_factory.h"
#include "api/audio_codecs/builtin_audio_encoder_factory.h"
#include "api/create_peerconnection_factory.h"
#include "api/jsep.h"
#include "api/make_ref_counted.h"
#include "api/peer_connection_interface.h"
#include "api/rtc_error.h"
#include "api/rtc_event_log/rtc_event_log_factory.h"
#include "api/scoped_refptr.h"
#include "api/set_local_description_observer_interface.h"
#include "api/set_remote_description_observer_interface.h"
#include "api/task_queue/default_task_queue_factory.h"
#include "livekit_rtc/audio_device.h"
#include "livekit_rtc/capi.h"
#include "livekit_rtc/data_channel.h"
#include "livekit_rtc/transceiver.h"
#include "livekit_rtc/utils.h"
#include "livekit_rtc/video_decoder.h"
#include "livekit_rtc/video_encoder.h"
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
    webrtc::scoped_refptr<webrtc::DataChannelInterface> data_channel) {
  webrtc::scoped_refptr<DataChannel> lkDc =
      webrtc::make_ref_counted<DataChannel>(data_channel);
  observer_->onDataChannel(reinterpret_cast<lkDataChannel*>(lkDc.get()),
                           userdata_);
}

void PeerObserver::OnIceGatheringChange(
    webrtc::PeerConnectionInterface::IceGatheringState new_state) {}

void PeerObserver::OnIceCandidate(
    const webrtc::IceCandidateInterface* candidate) {
  std::string sdp;
  candidate->ToString(&sdp);
  std::string mid = candidate->sdp_mid();
  lkIceCandidate lkCandidate{};
  lkCandidate.sdpMid = mid.c_str();
  lkCandidate.sdpMLineIndex = candidate->sdp_mline_index();
  lkCandidate.sdp = sdp.c_str();
  observer_->onIceCandidate(&lkCandidate, userdata_);
}

void PeerObserver::OnTrack(
    webrtc::scoped_refptr<webrtc::RtpTransceiverInterface> transceiver) {
  webrtc::scoped_refptr<RtpTransceiver> lkTransceiver =
      webrtc::make_ref_counted<RtpTransceiver>(transceiver);

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
  task_queue_factory_ = webrtc::CreateDefaultTaskQueueFactory();

  network_thread_ = webrtc::Thread::CreateWithSocketServer();
  network_thread_->SetName("lk_network_thread", &network_thread_);
  network_thread_->Start();
  worker_thread_ = webrtc::Thread::Create();
  worker_thread_->SetName("lk_worker_thread", &worker_thread_);
  worker_thread_->Start();
  signaling_thread_ = webrtc::Thread::Create();
  signaling_thread_->SetName("lk_signaling_thread", &signaling_thread_);
  signaling_thread_->Start();

  worker_thread_->BlockingCall([&] {
    audio_device_ = webrtc::make_ref_counted<livekit::AudioDevice>(
        task_queue_factory_.get());
  });

  peer_factory_ = webrtc::CreatePeerConnectionFactory(
        network_thread_.get(), worker_thread_.get(), signaling_thread_.get(),
        audio_device_, webrtc::CreateBuiltinAudioEncoderFactory(),
        webrtc::CreateBuiltinAudioDecoderFactory(),
        std::make_unique<livekit::VideoEncoderFactory>(),
        std::make_unique<livekit::VideoDecoderFactory>(),
        nullptr, nullptr /*TODO: add cusom audio processor */, 
        nullptr, nullptr
  );

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

webrtc::scoped_refptr<Peer> PeerFactory::CreatePeer(
    const lkRtcConfiguration* config,
    const lkPeerObserver* observer,
    void* userdata) {
  webrtc::scoped_refptr<PeerObserver> obs =
      webrtc::make_ref_counted<PeerObserver>(observer, userdata);
  webrtc::PeerConnectionInterface::RTCConfiguration rtcConfig =
      toNativeConfig(*config);

  webrtc::PeerConnectionDependencies deps{obs.get()};
  webrtc::RTCErrorOr<webrtc::scoped_refptr<webrtc::PeerConnectionInterface>> res =
      peer_factory_->CreatePeerConnectionOrError(rtcConfig, std::move(deps));

  if (!res.ok()) {
    RTC_LOG_ERR(LS_ERROR) << "Failed to create PeerConnection: "
                          << res.error().message();
    return nullptr;
  }

  return webrtc::make_ref_counted<Peer>(res.value(), obs);
}

webrtc::scoped_refptr<DataChannel> Peer::CreateDataChannel(
    const char* label,
    const lkDataChannelInit* init) {
  webrtc::DataChannelInit dcInit = toNativeDataChannelInit(*init);

  webrtc::RTCErrorOr<webrtc::scoped_refptr<webrtc::DataChannelInterface>> res =
      peer_connection_->CreateDataChannelOrError(label, &dcInit);

  if (!res.ok()) {
    RTC_LOG_ERR(LS_ERROR) << "Failed to create DataChannel: "
                          << res.error().message();
    return nullptr;
  }

  return webrtc::make_ref_counted<DataChannel>(res.value());
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

  webrtc::scoped_refptr<SetLocalSdpObserver> setSdpObserver =
      webrtc::make_ref_counted<SetLocalSdpObserver>(observer, userdata);

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

  webrtc::scoped_refptr<SetRemoteSdpObserver> setSdpObserver =
      webrtc::make_ref_counted<SetRemoteSdpObserver>(observer, userdata);

  peer_connection_->SetRemoteDescription(std::move(desc),
                                         std::move(setSdpObserver));
  return true;
}

bool Peer::CreateOffer(const lkOfferAnswerOptions& options,
                       const lkCreateSdpObserver* observer,
                       void* userdata) {
  webrtc::PeerConnectionInterface::RTCOfferAnswerOptions rtcOptions =
      toNativeOfferAnswerOptions(options);

  webrtc::scoped_refptr<webrtc::CreateSessionDescriptionObserver>
      createSdpObserver =
          webrtc::make_ref_counted<CreateSdpObserver>(observer, userdata);

  peer_connection_->CreateOffer(createSdpObserver.get(), rtcOptions);
  return true;
}

bool Peer::CreateAnswer(const lkOfferAnswerOptions& options,
                        const lkCreateSdpObserver* observer,
                        void* userdata) {
  webrtc::PeerConnectionInterface::RTCOfferAnswerOptions rtcOptions =
      toNativeOfferAnswerOptions(options);

  webrtc::scoped_refptr<webrtc::CreateSessionDescriptionObserver>
      createSdpObserver =
          webrtc::make_ref_counted<CreateSdpObserver>(observer, userdata);

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
