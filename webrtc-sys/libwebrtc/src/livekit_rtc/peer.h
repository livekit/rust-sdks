#ifndef LIVEKIT_PEER_H
#define LIVEKIT_PEER_H

#include "api/peer_connection_interface.h"
#include "api/scoped_refptr.h"
#include "livekit_rtc/audio_device.h"
#include "livekit_rtc/include/capi.h"
#include "livekit_rtc/data_channel.h"
#include "livekit_rtc/session_description.h"

namespace livekit {

class PeerFactory;

webrtc::PeerConnectionInterface::RTCConfiguration toNativeConfig(const lkRtcConfiguration& config);

class PeerObserver : public webrtc::PeerConnectionObserver, public webrtc::RefCountInterface {
 public:
  PeerObserver(const lkPeerObserver* observer, void* userdata)
      : observer_(observer), userdata_(userdata) {}

  void OnSignalingChange(webrtc::PeerConnectionInterface::SignalingState new_state) override;
  void OnDataChannel(webrtc::scoped_refptr<webrtc::DataChannelInterface> data_channel) override;
  void OnIceGatheringChange(webrtc::PeerConnectionInterface::IceGatheringState new_state) override;
  void OnIceCandidate(const webrtc::IceCandidateInterface* candidate) override;
  void OnTrack(webrtc::scoped_refptr<webrtc::RtpTransceiverInterface> transceiver) override;
  void OnRemoveTrack(webrtc::scoped_refptr<webrtc::RtpReceiverInterface> receiver) override;
  void OnIceCandidateError(const std::string& address,
                           int port,
                           const std::string& url,
                           int error_code,
                           const std::string& error_text) override;
  void OnConnectionChange(webrtc::PeerConnectionInterface::PeerConnectionState new_state) override;

  void OnStandardizedIceConnectionChange(
      webrtc::PeerConnectionInterface::IceConnectionState new_state) override;

  void OnRenegotiationNeeded() override;

  void set_peer_connection(webrtc::scoped_refptr<webrtc::PeerConnectionInterface> pc) {
    peer_connection_ = pc;
  }

 private:
  const lkPeerObserver* observer_;
  void* userdata_;
  webrtc::scoped_refptr<webrtc::PeerConnectionInterface> peer_connection_;
};

class Peer : public webrtc::RefCountInterface {
 public:
  Peer(webrtc::scoped_refptr<PeerFactory> pc_factory,
       webrtc::scoped_refptr<webrtc::PeerConnectionInterface> pc,
       webrtc::scoped_refptr<PeerObserver> observer)
      : pc_factory_(pc_factory), observer_(observer), peer_connection_(pc) {}

  webrtc::scoped_refptr<DataChannel> CreateDataChannel(const char* label,
                                                       const lkDataChannelInit* init);

  bool AddIceCandidate(const lkIceCandidate* candidate,
                       void (*onComplete)(lkRtcError* error, void* userdata),
                       void* userdata);

  bool SetLocalDescription(const lkSessionDescription* desc,
                           const lkSetSdpObserver* observer,
                           void* userdata);

  bool SetRemoteDescription(const lkSessionDescription* desc,
                            const lkSetSdpObserver* observer,
                            void* userdata);

  bool CreateOffer(const lkOfferAnswerOptions& options,
                   const lkCreateSdpObserver* observer,
                   void* userdata);

  bool CreateAnswer(const lkOfferAnswerOptions& options,
                    const lkCreateSdpObserver* observer,
                    void* userdata);

  bool SetConfig(const lkRtcConfiguration* config);

  void RestartIce();

  lkRtpSender* AddTrack(lkMediaStreamTrack* track,
                        lkString** streamIds,
                        int streamIdCount,
                        lkRtcError** error);

  lkPeerState GetPeerState() const {
    return static_cast<lkPeerState>(peer_connection_->peer_connection_state());
  }

  lkIceGatheringState GetIceGatheringState() const {
    return static_cast<lkIceGatheringState>(peer_connection_->ice_gathering_state());
  }

  lkIceState GetIceConnectionState() const {
    return static_cast<lkIceState>(peer_connection_->ice_connection_state());
  }

  lkSignalingState GetSignalingState() const {
    return static_cast<lkSignalingState>(peer_connection_->signaling_state());
  }

  lkSessionDescription* GetCurrentLocalDescription() const;

  lkSessionDescription* GetCurrentRemoteDescription() const;

  lkVectorGeneric* GetSenders();

  lkVectorGeneric* GetReceivers();

  lkVectorGeneric* GetTransceivers();

  bool Close();

 private:
  webrtc::scoped_refptr<PeerFactory> pc_factory_;
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

  webrtc::scoped_refptr<webrtc::PeerConnectionFactoryInterface> GetPeerConnectionFactory() const {
    return peer_factory_;
  }

  lkRtcVideoTrack* CreateVideoTrack(const char* id, lkVideoTrackSource* source);

  lkRtcAudioTrack* CreateAudioTrack(const char* id, lkAudioTrackSource* source);

  webrtc::Thread* network_thread() const { return network_thread_.get(); }

  webrtc::Thread* worker_thread() const { return worker_thread_.get(); }

  webrtc::Thread* signaling_thread() const { return signaling_thread_.get(); }

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
