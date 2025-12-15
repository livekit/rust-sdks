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
#include "livekit_rtc/audio_track.h"
#include "livekit_rtc/data_channel.h"
#include "livekit_rtc/ice_candidate.h"
#include "livekit_rtc/rtp_receiver.h"
#include "livekit_rtc/rtp_sender.h"
#include "livekit_rtc/rtp_transceiver.h"
#include "livekit_rtc/session_description.h"
#include "livekit_rtc/utils.h"
#include "livekit_rtc/video_decoder_factory.h"
#include "livekit_rtc/video_encoder_factory.h"
#include "livekit_rtc/video_track.h"
#include "media/engine/webrtc_media_engine.h"
#include "rtc_base/logging.h"
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
    observer_->onSuccess(
        reinterpret_cast<lkSessionDescription*>(
            SessionDescription::Create(sdp, desc->GetType()).release()),
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
  observer_->onDataChannel(reinterpret_cast<lkDataChannel*>(lkDc.release()),
                           userdata_);
}

void PeerObserver::OnIceGatheringChange(
    webrtc::PeerConnectionInterface::IceGatheringState new_state) {
  observer_->onIceGatheringChange(static_cast<lkIceGatheringState>(new_state),
                                  userdata_);
}

void PeerObserver::OnStandardizedIceConnectionChange(
    webrtc::PeerConnectionInterface::IceConnectionState new_state) {
  observer_->onStandardizedIceConnectionChange(
      static_cast<lkIceState>(new_state), userdata_);
}

void PeerObserver::OnRenegotiationNeeded() {
  observer_->onRenegotiationNeeded(userdata_);
}

void PeerObserver::OnIceCandidate(
    const webrtc::IceCandidateInterface* candidate) {
  std::string sdp;
  candidate->ToString(&sdp);
  std::string mid = candidate->sdp_mid();
  observer_->onIceCandidate(
      reinterpret_cast<lkIceCandidate*>(
          IceCandidate::Create(mid, candidate->sdp_mline_index(), sdp)
              .release()),
      userdata_);
}

void PeerObserver::OnTrack(
    webrtc::scoped_refptr<webrtc::RtpTransceiverInterface> transceiver) {
  webrtc::scoped_refptr<RtpTransceiver> lkTransceiver =
      webrtc::make_ref_counted<RtpTransceiver>(transceiver, peer_connection_);
  webrtc::scoped_refptr<RtpReceiver> lkReceiver =
      webrtc::make_ref_counted<RtpReceiver>(transceiver->receiver(),
                                            peer_connection_);
  webrtc::scoped_refptr<MediaStreamTrack> lkTrack =
      webrtc::make_ref_counted<MediaStreamTrack>(
          transceiver->receiver()->track());
  auto lkStreamArray = webrtc::make_ref_counted<
      livekit::LKVector<webrtc::scoped_refptr<livekit::MediaStream>>>();
  auto streams = transceiver->receiver()->streams();
  for (const auto& stream : streams) {
    lkStreamArray->push_back(
        webrtc::make_ref_counted<livekit::MediaStream>(stream));
  }
  observer_->onTrack(
      reinterpret_cast<lkRtpTransceiver*>(lkTransceiver.release()),
      reinterpret_cast<lkRtpReceiver*>(lkReceiver.release()),
      reinterpret_cast<lkVectorGeneric*>(lkStreamArray.get()),
      reinterpret_cast<lkMediaStreamTrack*>(lkTrack.release()), userdata_);
}

void PeerObserver::OnRemoveTrack(
    webrtc::scoped_refptr<webrtc::RtpReceiverInterface> receiver) {
  webrtc::scoped_refptr<RtpReceiver> lkReceiver =
      webrtc::make_ref_counted<RtpReceiver>(receiver, peer_connection_);
  observer_->onRemoveTrack(
      reinterpret_cast<lkRtpReceiver*>(lkReceiver.release()), userdata_);
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
      std::make_unique<livekit::VideoDecoderFactory>(), nullptr,
      nullptr /*TODO: add cusom audio processor */, nullptr, nullptr);

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
  webrtc::RTCErrorOr<webrtc::scoped_refptr<webrtc::PeerConnectionInterface>>
      res = peer_factory_->CreatePeerConnectionOrError(rtcConfig,
                                                       std::move(deps));

  if (!res.ok()) {
    RTC_LOG_ERR(LS_ERROR) << "Failed to create PeerConnection: "
                          << res.error().message();
    return nullptr;
  }
  obs->set_peer_connection(res.value());
  return webrtc::make_ref_counted<Peer>(
      webrtc::scoped_refptr<PeerFactory>(this), res.value(), obs);
}

lkRtcVideoTrack* PeerFactory::CreateVideoTrack(const char* id,
                                               lkVideoTrackSource* source) {
  auto videoSource = reinterpret_cast<livekit::VideoTrackSource*>(source);
  auto track = peer_factory_->CreateVideoTrack(videoSource->video_source(), id);
  if (track) {
    return reinterpret_cast<lkRtcVideoTrack*>(
        webrtc::make_ref_counted<livekit::VideoTrack>(track).release());
  }
  return nullptr;
}

lkRtcAudioTrack* PeerFactory::CreateAudioTrack(const char* id,
                                               lkAudioTrackSource* source) {
  auto audioSource = reinterpret_cast<livekit::AudioTrackSource*>(source);
  auto track = peer_factory_->CreateAudioTrack(id, audioSource->audio_source());
  if (track) {
    return reinterpret_cast<lkRtcAudioTrack*>(
        webrtc::make_ref_counted<livekit::AudioTrack>(track).release());
  }
  return nullptr;
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

lkRtpSender* Peer::AddTrack(lkMediaStreamTrack* track,
                            lkString** streamIds,
                            int streamIdCount,
                            lkRtcError** error) {
  auto mediaTrack =
      reinterpret_cast<livekit::MediaStreamTrack*>(track)->rtc_track();
  std::vector<std::string> std_stream_ids;
  for (int i = 0; i < streamIdCount; ++i) {
    std_stream_ids.push_back(
        reinterpret_cast<livekit::LKString*>(streamIds[i])->get());
  }
  webrtc::RTCErrorOr<webrtc::scoped_refptr<webrtc::RtpSenderInterface>> res =
      peer_connection_->AddTrack(mediaTrack, std_stream_ids);
  if (!res.ok()) {
    lkRtcError err = toRtcError(res.error());
    *error = reinterpret_cast<lkRtcError*>(new lkRtcError(err));
    return nullptr;
  }
  return reinterpret_cast<lkRtpSender*>(
      webrtc::make_ref_counted<livekit::RtpSender>(res.value(),
                                                   peer_connection_)
          .release());
}

lkRtpTransceiver* Peer::AddTransceiver(lkMediaStreamTrack* track,
                                       lkRtpTransceiverInit* init,
                                       lkRtcError* error) {
  auto mediaTrack =
      reinterpret_cast<livekit::MediaStreamTrack*>(track)->rtc_track();
  auto transceiverInit = reinterpret_cast<livekit::RtpTransceiverInit*>(init);
  webrtc::RTCErrorOr<webrtc::scoped_refptr<webrtc::RtpTransceiverInterface>>
      res = peer_connection_->AddTransceiver(mediaTrack,
                                             transceiverInit->rtc_init);
  if (!res.ok()) {
    lkRtcError err = toRtcError(res.error());
    *error = err;
    return nullptr;
  }
  return reinterpret_cast<lkRtpTransceiver*>(
      webrtc::make_ref_counted<livekit::RtpTransceiver>(res.value(),
                                                        peer_connection_)
          .release());
}

lkRtpTransceiver* Peer::AddTransceiverForMedia(lkMediaType type,
                                               lkRtpTransceiverInit* init,
                                               lkRtcError* error) {
  auto mediaType = static_cast<webrtc::MediaType>(type);
  auto transceiverInit = reinterpret_cast<livekit::RtpTransceiverInit*>(init);
  webrtc::RTCErrorOr<webrtc::scoped_refptr<webrtc::RtpTransceiverInterface>>
      res = peer_connection_->AddTransceiver(mediaType,
                                                    transceiverInit->rtc_init);
  if (!res.ok()) {
    lkRtcError err = toRtcError(res.error());
    *error = err;
    return nullptr;
  }
  return reinterpret_cast<lkRtpTransceiver*>(
      webrtc::make_ref_counted<livekit::RtpTransceiver>(res.value(),
                                                        peer_connection_)
          .release());
}

bool Peer::AddIceCandidate(const lkIceCandidate* candidate,
                           void (*onComplete)(lkRtcError* error,
                                              void* userdata),
                           void* userdata) {
  auto lkCandidatePtr =
      reinterpret_cast<const livekit::IceCandidate*>(candidate);

  peer_connection_->AddIceCandidate(lkCandidatePtr->Clone(),
                                    [&](webrtc::RTCError err) {
                                      if (err.ok()) {
                                        onComplete(nullptr, userdata);
                                      } else {
                                        lkRtcError lkErr = toRtcError(err);
                                        onComplete(&lkErr, userdata);
                                      }
                                    });
  return true;
}

bool Peer::SetLocalDescription(const lkSessionDescription* desc,
                               const lkSetSdpObserver* observer,
                               void* userdata) {
  auto jsepDesc = reinterpret_cast<const livekit::SessionDescription*>(desc);

  peer_connection_->SetLocalDescription(
      jsepDesc->Clone(),
      webrtc::make_ref_counted<SetLocalSdpObserver>(observer, userdata));
  return true;
}

bool Peer::SetRemoteDescription(const lkSessionDescription* desc,
                                const lkSetSdpObserver* observer,
                                void* userdata) {
  auto jsepDesc = reinterpret_cast<const livekit::SessionDescription*>(desc);
  peer_connection_->SetRemoteDescription(
      jsepDesc->Clone(),
      webrtc::make_ref_counted<SetRemoteSdpObserver>(observer, userdata));
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

void Peer::RestartIce() {
  peer_connection_->RestartIce();
}

lkSessionDescription* Peer::GetCurrentLocalDescription() const {
  auto desc = peer_connection_->current_local_description();
  if (!desc) {
    return nullptr;
  }
  return reinterpret_cast<lkSessionDescription*>(
      SessionDescription::Create(desc).release());
}

lkSessionDescription* Peer::GetCurrentRemoteDescription() const {
  auto desc = peer_connection_->current_remote_description();
  if (!desc) {
    return nullptr;
  }
  return reinterpret_cast<lkSessionDescription*>(
      SessionDescription::Create(desc).release());
}

lkVectorGeneric* Peer::GetSenders() {
  auto track_array =
      webrtc::make_ref_counted<LKVector<webrtc::scoped_refptr<RtpSender>>>();
  auto senders = peer_connection_->GetSenders();
  for (const auto& sender : senders) {
    webrtc::scoped_refptr<RtpSender> lkSender =
        webrtc::make_ref_counted<RtpSender>(sender, peer_connection_);
    track_array->push_back(lkSender);
  }
  return reinterpret_cast<lkVectorGeneric*>(track_array.release());
}

lkVectorGeneric* Peer::GetReceivers() {
  auto track_array =
      webrtc::make_ref_counted<LKVector<webrtc::scoped_refptr<RtpReceiver>>>();
  auto receivers = peer_connection_->GetReceivers();
  for (const auto& receiver : receivers) {
    webrtc::scoped_refptr<RtpReceiver> lkReceiver =
        webrtc::make_ref_counted<RtpReceiver>(receiver, peer_connection_);
    track_array->push_back(lkReceiver);
  }
  return reinterpret_cast<lkVectorGeneric*>(track_array.release());
}

lkVectorGeneric* Peer::GetTransceivers() {
  auto track_array = webrtc::make_ref_counted<
      LKVector<webrtc::scoped_refptr<RtpTransceiver>>>();
  auto transceivers = peer_connection_->GetTransceivers();
  for (const auto& transceiver : transceivers) {
    webrtc::scoped_refptr<RtpTransceiver> lkReceiver =
        webrtc::make_ref_counted<RtpTransceiver>(transceiver, peer_connection_);
    track_array->push_back(lkReceiver);
  }
  return reinterpret_cast<lkVectorGeneric*>(track_array.release());
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

lkRtpCapabilities* PeerFactory::GetRtpSenderCapabilities(lkMediaType type) {
  auto rtc_caps = peer_factory_->GetRtpSenderCapabilities(
      static_cast<webrtc::MediaType>(type));

  return reinterpret_cast<lkRtpCapabilities*>(
      livekit::RtpCapabilities::FromNative(rtc_caps).release());
}

lkRtpCapabilities* PeerFactory::GetRtpReceiverCapabilities(lkMediaType type) {
  auto rtc_caps = peer_factory_->GetRtpReceiverCapabilities(
      static_cast<webrtc::MediaType>(type));

  return reinterpret_cast<lkRtpCapabilities*>(
      livekit::RtpCapabilities::FromNative(rtc_caps).release());
}

}  // namespace livekit
