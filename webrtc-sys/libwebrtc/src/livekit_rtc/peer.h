#ifndef LIVEKIT_PEER_H
#define LIVEKIT_PEER_H

#include "api/peer_connection_interface.h"
#include "api/scoped_refptr.h"
#include "livekit_rtc/audio_device.h"
#include "livekit_rtc/capi.h"
#include "livekit_rtc/data_channel.h"

namespace livekit {

webrtc::PeerConnectionInterface::RTCConfiguration toNativeConfig(
    const lkRtcConfiguration& config);

class PeerObserver : public webrtc::PeerConnectionObserver,
                     public webrtc::RefCountInterface {
 public:
  PeerObserver(const lkPeerObserver* observer, void* userdata)
      : observer_(observer), userdata_(userdata) {}

  void OnSignalingChange(
      webrtc::PeerConnectionInterface::SignalingState new_state) override;
  void OnDataChannel(webrtc::scoped_refptr<webrtc::DataChannelInterface>
                         data_channel) override;
  void OnIceGatheringChange(
      webrtc::PeerConnectionInterface::IceGatheringState new_state) override;
  void OnIceCandidate(const webrtc::IceCandidateInterface* candidate) override;
  void OnTrack(webrtc::scoped_refptr<webrtc::RtpTransceiverInterface>
                   transceiver) override;
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

class Peer : public webrtc::RefCountInterface {
 public:
  Peer(webrtc::scoped_refptr<webrtc::PeerConnectionInterface> pc,
       webrtc::scoped_refptr<PeerObserver> observer)
      : observer_(observer), peer_connection_(pc) {}

  webrtc::scoped_refptr<DataChannel> CreateDataChannel(
      const char* label, const lkDataChannelInit* init);

  bool AddIceCandidate(const char* sdpMid,
                       int sdpMLineIndex,
                       const char* candidate,
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
  webrtc::scoped_refptr<PeerObserver> observer_;
  webrtc::scoped_refptr<webrtc::PeerConnectionInterface> peer_connection_;
};

class PeerFactory : public webrtc::RefCountInterface {
 public:
  PeerFactory();
  ~PeerFactory();

  webrtc::scoped_refptr<Peer> CreatePeer(const lkRtcConfiguration* config,
                                         const lkPeerObserver* observer,
                                         void* userdata);

 private:
  std::unique_ptr<webrtc::Thread> network_thread_;
  std::unique_ptr<webrtc::Thread> worker_thread_;
  std::unique_ptr<webrtc::Thread> signaling_thread_;
  std::unique_ptr<webrtc::TaskQueueFactory> task_queue_factory_;

  webrtc::scoped_refptr<AudioDevice> audio_device_;
  webrtc::scoped_refptr<webrtc::PeerConnectionFactoryInterface> peer_factory_;
};

}  // namespace livekit

#endif  // LIVEKIT_PEER_H
