#ifndef LIVEKIT_PEER_H
#define LIVEKIT_PEER_H

#include "api/peer_connection_interface.h"
#include "api/scoped_refptr.h"
#include "livekit/audio_device.h"
#include "livekit/capi.h"
#include "livekit/data_channel.h"

namespace livekit {

webrtc::PeerConnectionInterface::RTCConfiguration toNativeConfig(
    const lkRtcConfiguration& config);

class PeerObserver : public webrtc::PeerConnectionObserver,
                     public rtc::RefCountInterface {
 public:
  PeerObserver(const lkPeerObserver* observer, void* userdata)
      : observer_(observer), userdata_(userdata) {}

  void OnSignalingChange(
      webrtc::PeerConnectionInterface::SignalingState new_state) override;
  void OnDataChannel(
      rtc::scoped_refptr<webrtc::DataChannelInterface> data_channel) override;
  void OnIceGatheringChange(
      webrtc::PeerConnectionInterface::IceGatheringState new_state) override;
  void OnIceCandidate(const webrtc::IceCandidateInterface* candidate) override;
  void OnTrack(
      rtc::scoped_refptr<webrtc::RtpTransceiverInterface> transceiver) override;
  void OnIceCandidateError(const std::string& address,
                           int port,
                           const std::string& url,
                           int error_code,
                           const std::string& error_text) override;
  void OnConnectionChange(
      webrtc::PeerConnectionInterface::PeerConnectionState new_state) override;

 private:
  const lkPeerObserver* observer_;
  void* userdata_;
};

class Peer : public rtc::RefCountInterface {
 public:
  Peer(rtc::scoped_refptr<webrtc::PeerConnectionInterface> pc,
       rtc::scoped_refptr<PeerObserver> observer)
      : observer_(observer), peer_connection_(pc) {}

  rtc::scoped_refptr<DataChannel> CreateDataChannel(
      const char* label,
      const lkDataChannelInit* init);

  bool AddIceCandidate(const lkIceCandidate* candidate,
                       void (*onComplete)(lkRtcError* error, void* userdata),
                       void* userdata);

  bool SetLocalDescription(lkSdpType type,
                           const char* sdp,
                           const lkSetSdpObserver* observer,
                           void* userdata);

  bool SetRemoteDescription(lkSdpType type,
                            const char* sdp,
                            const lkSetSdpObserver* observer,
                            void* userdata);

  bool CreateOffer(const lkOfferAnswerOptions& options,
                   const lkCreateSdpObserver* observer,
                   void* userdata);

  bool CreateAnswer(const lkOfferAnswerOptions& options,
                    const lkCreateSdpObserver* observer,
                    void* userdata);

  bool SetConfig(const lkRtcConfiguration* config);

  bool Close();

 private:
  rtc::scoped_refptr<PeerObserver> observer_;
  rtc::scoped_refptr<webrtc::PeerConnectionInterface> peer_connection_;
};

class PeerFactory : public rtc::RefCountInterface {
 public:
  PeerFactory();
  ~PeerFactory();

  rtc::scoped_refptr<Peer> CreatePeer(const lkRtcConfiguration* config,
                                      const lkPeerObserver* observer,
                                      void* userdata);

 private:
  std::unique_ptr<rtc::Thread> network_thread_;
  std::unique_ptr<rtc::Thread> worker_thread_;
  std::unique_ptr<rtc::Thread> signaling_thread_;

  rtc::scoped_refptr<AudioDevice> audio_device_;
  rtc::scoped_refptr<webrtc::PeerConnectionFactoryInterface> peer_factory_;
};

}  // namespace livekit

#endif  // LIVEKIT_PEER_H
