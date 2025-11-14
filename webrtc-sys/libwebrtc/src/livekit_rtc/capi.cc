#include "livekit_rtc/capi.h"

#include "api/make_ref_counted.h"
#include "livekit_rtc/data_channel.h"
#include "livekit_rtc/peer.h"
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
                       const char* sdpMid,
                       int sdpMLineIndex,
                       const char* candidate,
                       void (*onComplete)(lkRtcError* error, void* userdata),
                       void* userdata) {
  auto cand = IceCandidateInterface = webrtc::CreateIceCandidate(
  sdpMid, sdpMLineIndex, candidate, nullptr);
  return reinterpret_cast<livekit::Peer*>(peer)->AddIceCandidate(
      cand, onComplete, userdata);
}

bool lkSetLocalDescription(lkPeer* peer,
                           lkSdpType type,
                           const char* sdp,
                           const lkSetSdpObserver* observer,
                           void* userdata) {
  return reinterpret_cast<livekit::Peer*>(peer)->SetLocalDescription(
      type, sdp, observer, userdata);
}

bool lkSetRemoteDescription(lkPeer* peer,
                            lkSdpType type,
                            const char* sdp,
                            const lkSetSdpObserver* observer,
                            void* userdata) {
  return reinterpret_cast<livekit::Peer*>(peer)->SetRemoteDescription(
      type, sdp, observer, userdata);
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

int lkDcGetLabel(lkDataChannel* dc, char* buffer, int bufferSize) {
  auto label =
      reinterpret_cast<livekit::DataChannel*>(dc)->data_channel_->label();
  int len = static_cast<int>(label.size());
  if (bufferSize > 0) {
    int copySize = (len < bufferSize - 1) ? len : bufferSize - 1;
    memcpy(buffer, label.c_str(), copySize);
    buffer[copySize] = '\0';
  }
  return len;
}

LK_EXPORT uint64_t lkDcGetBufferedAmount(lkDataChannel* dc) {
  return reinterpret_cast<livekit::DataChannel*>(dc)
      ->data_channel_->buffered_amount();
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
