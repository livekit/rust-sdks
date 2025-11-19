#include "livekit_rtc/capi.h"

#include "api/make_ref_counted.h"
#include "livekit_rtc/audio_track.h"
#include "livekit_rtc/data_channel.h"
#include "livekit_rtc/ice_candidate.h"
#include "livekit_rtc/peer.h"
#include "livekit_rtc/session_description.h"
#include "rtc_base/logging.h"
#include "rtc_base/ref_count.h"
#include "rtc_base/ssl_adapter.h"

void lkAddRef(lkRefCounted* rc) {
  reinterpret_cast<webrtc::RefCountInterface*>(rc)->AddRef();
}

void lkReleaseRef(lkRefCounted* rc) {
  reinterpret_cast<webrtc::RefCountInterface*>(rc)->Release();
}

int lkInitialize() {
  if (!webrtc::InitializeSSL()) {
    RTC_LOG_ERR(LS_ERROR) << "Failed to InitializeSSL()";
    return 0;
  }

#ifdef WEBRTC_WIN
  WSADATA data;
  WSAStartup(MAKEWORD(1, 0), &data);
#endif

  return 1;
}

int lkDispose() {
  if (!webrtc::CleanupSSL()) {
    RTC_LOG_ERR(LS_ERROR) << "Failed to CleanupSSL()";
    return 0;
  }

#ifdef WEBRTC_WIN
  WSACleanup();
#endif

  return 1;
}

lkPeerFactory* lkCreatePeerFactory() {
  return reinterpret_cast<lkPeerFactory*>(
      webrtc::make_ref_counted<livekit::PeerFactory>().release());
}

lkPeer* lkCreatePeer(lkPeerFactory* factory,
                     const lkRtcConfiguration* config,
                     const lkPeerObserver* observer,
                     void* userdata) {
  return reinterpret_cast<lkPeer*>(
      reinterpret_cast<livekit::PeerFactory*>(factory)
          ->CreatePeer(config, observer, userdata)
          .release());
}

lkDataChannel* lkCreateDataChannel(lkPeer* peer,
                                   const char* label,
                                   const lkDataChannelInit* init) {
  return reinterpret_cast<lkDataChannel*>(reinterpret_cast<livekit::Peer*>(peer)
                                              ->CreateDataChannel(label, init)
                                              .release());
}

bool lkAddIceCandidate(lkPeer* peer,
                       lkIceCandidate* candidate,
                       void (*onComplete)(lkRtcError* error, void* userdata),
                       void* userdata) {
  return reinterpret_cast<livekit::Peer*>(peer)->AddIceCandidate(
      candidate, onComplete, userdata);
}

bool lkSetLocalDescription(lkPeer* peer,
                           const lkSessionDescription* desc,
                           const lkSetSdpObserver* observer,
                           void* userdata) {
  return reinterpret_cast<livekit::Peer*>(peer)->SetLocalDescription(
      desc, observer, userdata);
}

bool lkSetRemoteDescription(lkPeer* peer,
                            const lkSessionDescription* desc,
                            const lkSetSdpObserver* observer,
                            void* userdata) {
  return reinterpret_cast<livekit::Peer*>(peer)->SetRemoteDescription(
      desc, observer, userdata);
}

bool lkCreateOffer(lkPeer* peer,
                   const lkOfferAnswerOptions* options,
                   const lkCreateSdpObserver* observer,
                   void* userdata) {
  return reinterpret_cast<livekit::Peer*>(peer)->CreateOffer(*options, observer,
                                                             userdata);
}

bool lkCreateAnswer(lkPeer* peer,
                    const lkOfferAnswerOptions* options,
                    const lkCreateSdpObserver* observer,
                    void* userdata) {
  return reinterpret_cast<livekit::Peer*>(peer)->CreateAnswer(
      *options, observer, userdata);
}

bool lkPeerSetConfig(lkPeer* peer, const lkRtcConfiguration* config) {
  return reinterpret_cast<livekit::Peer*>(peer)->SetConfig(config);
}

bool lkPeerClose(lkPeer* peer) {
  return reinterpret_cast<livekit::Peer*>(peer)->Close();
}

void lkDcRegisterObserver(lkDataChannel* dc,
                          const lkDataChannelObserver* observer,
                          void* userdata) {
  reinterpret_cast<livekit::DataChannel*>(dc)->RegisterObserver(observer,
                                                                userdata);
}

void lkDcUnregisterObserver(lkDataChannel* dc) {
  reinterpret_cast<livekit::DataChannel*>(dc)->UnregisterObserver();
}

lkDcState lkDcGetState(lkDataChannel* dc) {
  return reinterpret_cast<livekit::DataChannel*>(dc)->State();
}

int lkDcGetId(lkDataChannel* dc) {
  return reinterpret_cast<livekit::DataChannel*>(dc)->Id();
}

int lkDcGetLabelLength(lkDataChannel* dc) {
  auto label = reinterpret_cast<livekit::DataChannel*>(dc)->label();
  return static_cast<int>(label.size());
}

int lkDcGetLabel(lkDataChannel* dc, char* buffer, int bufferSize) {
  auto label = reinterpret_cast<livekit::DataChannel*>(dc)->label();
  int len = static_cast<int>(label.size());
  if (bufferSize > 0) {
    int copySize = (len < bufferSize) ? len : bufferSize;
    memcpy(buffer, label.c_str(), copySize);
  }
  return len;
}

uint64_t lkDcGetBufferedAmount(lkDataChannel* dc) {
  return reinterpret_cast<livekit::DataChannel*>(dc)->buffered_amount();
}

void lkDcSendAsync(lkDataChannel* dc,
                   const uint8_t* data,
                   uint64_t size,
                   bool binary,
                   void (*onComplete)(lkRtcError* error, void* userdata),
                   void* userdata) {
  reinterpret_cast<livekit::DataChannel*>(dc)->SendAsync(data, size, binary,
                                                         onComplete, userdata);
}

void lkDcClose(lkDataChannel* dc) {
  reinterpret_cast<livekit::DataChannel*>(dc)->Close();
}

lkSessionDescription* lkCreateSessionDescription(lkSdpType type,
                                                 const char* sdp) {
  auto desc = livekit::SessionDescription::Create(
      std::string(sdp), static_cast<webrtc::SdpType>(type));
  if (!desc) {
    return nullptr;
  }
  return reinterpret_cast<lkSessionDescription*>(desc.release());
}

lkSdpType lkSessionDescriptionGetType(lkSessionDescription* desc) {
  return static_cast<lkSdpType>(
      reinterpret_cast<livekit::SessionDescription*>(desc)->GetType());
}

int lkSessionDescriptionGetSdpLength(lkSessionDescription* desc) {
  std::string sdp =
      reinterpret_cast<livekit::SessionDescription*>(desc)->ToString();
  return sdp.length();
}

int lkSessionDescriptionGetSdp(lkSessionDescription* desc,
                               char* buffer,
                               int bufferSize) {
  std::string sdp =
      reinterpret_cast<livekit::SessionDescription*>(desc)->ToString();
  int len = static_cast<int>(sdp.size());
  if (bufferSize > 0) {
    int copySize = (len < bufferSize) ? len : bufferSize;
    memcpy(buffer, sdp.c_str(), copySize);
  }
  return len;
}

lkIceCandidate* lkCreateIceCandidate(const char* mid,
                                     int mlineIndex,
                                     const char* sdp) {
  auto candidate = livekit::IceCandidate::Create(std::string(mid), mlineIndex,
                                                 std::string(sdp));
  if (!candidate) {
    return nullptr;
  }
  return reinterpret_cast<lkIceCandidate*>(candidate.release());
}

int lkIceCandidateGetMlineIndex(lkIceCandidate* candidate) {
  return reinterpret_cast<livekit::IceCandidate*>(candidate)->mline_index();
}

int lkIceCandidateGetMidLength(lkIceCandidate* candidate) {
  auto mid = reinterpret_cast<livekit::IceCandidate*>(candidate)->mid();
  return static_cast<int>(mid.size());
}

int lkIceCandidateGetMid(lkIceCandidate* candidate,
                         char* buffer,
                         int bufferSize) {
  auto mid = reinterpret_cast<livekit::IceCandidate*>(candidate)->mid();
  int len = static_cast<int>(mid.size());
  if (bufferSize > 0) {
    int copySize = (len < bufferSize) ? len : bufferSize;
    memcpy(buffer, mid.c_str(), copySize);
  }
  return len;
}

int lkIceCandidateGetSdpLength(lkIceCandidate* candidate) {
  std::string sdp = reinterpret_cast<livekit::IceCandidate*>(candidate)->sdp();
  return sdp.length();
}

int lkIceCandidateGetSdp(lkIceCandidate* candidate,
                         char* buffer,
                         int bufferSize) {
  std::string sdp = reinterpret_cast<livekit::IceCandidate*>(candidate)->sdp();
  int len = static_cast<int>(sdp.size());
  if (bufferSize > 0) {
    int copySize = (len < bufferSize) ? len : bufferSize;
    memcpy(buffer, sdp.c_str(), copySize);
  }
  return len;
}

void lkPeerRestartIce(lkPeer* peer) {
  reinterpret_cast<livekit::Peer*>(peer)->RestartIce();
}

lkPeerState lkGetPeerState(lkPeer* peer) {
  return static_cast<lkPeerState>(
      reinterpret_cast<livekit::Peer*>(peer)->GetPeerState());
}

lkIceGatheringState lkPeerGetIceGatheringState(lkPeer* peer) {
  return static_cast<lkIceGatheringState>(
      reinterpret_cast<livekit::Peer*>(peer)->GetIceGatheringState());
}

lkIceState lkPeerGetIceConnectionState(lkPeer* peer) {
  return static_cast<lkIceState>(
      reinterpret_cast<livekit::Peer*>(peer)->GetIceConnectionState());
}

lkSignalingState lkPeerGetSignalingState(lkPeer* peer) {
  return static_cast<lkSignalingState>(
      reinterpret_cast<livekit::Peer*>(peer)->GetSignalingState());
}

const lkSessionDescription* lkPeerGetCurrentLocalDescription(lkPeer* peer) {
  return reinterpret_cast<livekit::Peer*>(peer)->GetCurrentLocalDescription();
}

const lkSessionDescription* lkPeerGetCurrentRemoteDescription(lkPeer* peer) {
  return reinterpret_cast<livekit::Peer*>(peer)->GetCurrentRemoteDescription();
}

lkRtpCapabilities* lkGetRtpSenderCapabilities(lkPeerFactory* factory) {
  auto peer_factory = reinterpret_cast<livekit::PeerFactory*>(factory)
                          ->GetPeerConnectionFactory();
  return nullptr;
}

lkRtpCapabilities* lkGetRtpReceiverCapabilities(lkPeerFactory* factory) {
  auto peer_factory = reinterpret_cast<livekit::PeerFactory*>(factory)
                          ->GetPeerConnectionFactory();
  return nullptr;
}

lkRtcVideoTrack* CreateVideoTrack(const char* id, lkVideoTrackSource* source) {
  return nullptr;
}

lkRtcAudioTrack* CreateAudioTrack(const char* id, lkAudioTrackSource* source) {
  return nullptr;
}

lkNativeAudioSink* lkCreateNativeAudioSink(lkNativeAudioSinkObserver* observer,
                                           void* userdata,
                                           int sample_rate,
                                           int num_channels) {
  return reinterpret_cast<lkNativeAudioSink*>(
      webrtc::make_ref_counted<livekit::NativeAudioSink>(
          observer, userdata, sample_rate, num_channels)
          .release());
}

lkAudioTrackSource* lkCreateAudioTrackSource(lkAudioSourceOptions options,
                                             int sample_rate,
                                             int num_channels,
                                             int queue_size_ms) {
  return reinterpret_cast<lkAudioTrackSource*>(
      livekit::AudioTrackSource::Create(options, sample_rate, num_channels,
                                        queue_size_ms)
          .release());
}

void lkAudioTrackSourceSetAudioOptions(lkAudioTrackSource* source,
                                       const lkAudioSourceOptions* options) {
  reinterpret_cast<livekit::AudioTrackSource*>(source)->set_audio_options(
      *options);
}

lkAudioSourceOptions lkAudioTrackSourceGetAudioOptions(
    lkAudioTrackSource* source) {
  return reinterpret_cast<livekit::AudioTrackSource*>(source)->audio_options();
}

bool lkAudioTrackSourceCaptureFrame(lkAudioTrackSource* source,
                                    const int16_t* audio_data,
                                    uint32_t sample_rate,
                                    uint32_t number_of_channels,
                                    int number_of_frames,
                                    void* userdata,
                                    void (*onComplete)(void* userdata)) {
  std::vector<int16_t> audio_vector(
      audio_data, audio_data + number_of_channels * number_of_frames);
  return reinterpret_cast<livekit::AudioTrackSource*>(source)->capture_frame(
      audio_vector, sample_rate, number_of_channels, number_of_frames, userdata,
      onComplete);
}

void lkAudioTrackSourceClearBuffer(lkAudioTrackSource* source) {
  reinterpret_cast<livekit::AudioTrackSource*>(source)->clear_buffer();
}

int lkAudioTrackSourceGetSampleRate(lkAudioTrackSource* source) {
  return reinterpret_cast<livekit::AudioTrackSource*>(source)->sample_rate();
}

int lkAudioTrackSourceGetNumChannels(lkAudioTrackSource* source) {
  return reinterpret_cast<livekit::AudioTrackSource*>(source)->num_channels();
}