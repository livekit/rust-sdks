#ifndef LIVEKIT_CAPI_H
#define LIVEKIT_CAPI_H

#include <stdbool.h>
#include <stdint.h>

#define LK_EXPORT __attribute__((visibility("default")))

#ifdef __cplusplus
extern "C" {
#endif

// Opaque types, mapping to C++ classes
typedef void lkRefCounted;
typedef void lkPeerFactory;
typedef void lkPeer;
typedef void lkDataChannel;
typedef void lkRtpTransceiver;

typedef enum {
  LK_ICE_TRANSPORT_TYPE_NONE,
  LK_ICE_TRANSPORT_TYPE_RELAY,
  LK_ICE_TRANSPORT_TYPE_NO_HOST,
  LK_ICE_TRANSPORT_TYPE_ALL,
} lkIceTransportType;

typedef enum {
  LK_GATHERING_POLICY_ONCE,
  LK_GATHERING_POLICY_CONTINUALLY,
} lkContinualGatheringPolicy;

typedef enum {
  LK_PEER_STATE_NEW,
  LK_PEER_STATE_CONNECTING,
  LK_PEER_STATE_CONNECTED,
  LK_PEER_STATE_DISCONNECTED,
  LK_PEER_STATE_FAILED,
  LK_PEER_STATE_CLOSED,
} lkPeerState;

typedef enum {
  LK_SIGNALING_STATE_STABLE,
  LK_SIGNALING_STATE_HAVE_LOCAL_OFFER,
  LK_SIGNALING_STATE_HAVE_LOCAL_PRANSWER,
  LK_SIGNALING_STATE_HAVE_REMOTE_OFFER,
  LK_SIGNALING_STATE_HAVE_REMOTE_PRANSWER,
  LK_SIGNALING_STATE_CLOSED,
} lkSignalingState;

typedef enum {
  LK_ICE_STATE_NEW,
  LK_ICE_STATE_CHECKING,
  LK_ICE_STATE_CONNECTED,
  LK_ICE_STATE_COMPLETED,
  LK_ICE_STATE_FAILED,
  LK_ICE_STATE_DISCONNECTED,
  LK_ICE_STATE_CLOSED,
} lkIceState;

typedef enum {
  LK_SDP_TYPE_OFFER,
  LK_SDP_TYPE_PRANSWER,
  LK_SDP_TYPE_ANSWER,
  LK_SDP_TYPE_ROLLBACK,
} lkSdpType;

typedef enum {
  LK_DC_STATE_CONNECTING,
  LK_DC_STATE_OPEN,
  LK_DC_STATE_CLOSING,
  LK_DC_STATE_CLOSED,
} lkDcState;

typedef struct {
  const char* sdpMid;
  int sdpMLineIndex;
  const char* sdp;
} lkIceCandidate;

typedef struct {
  void (*onSignalingChange)(lkSignalingState state, void* userdata);
  void (*onIceCandidate)(const lkIceCandidate* candidate, void* userdata);
  void (*onDataChannel)(const lkDataChannel* dc, void* userdata);
  void (*onTrack)(const lkRtpTransceiver* transceiver, void* userdata);
  void (*onConnectionChange)(lkPeerState state, void* userdata);
  void (*onIceCandidateError)(const char* address,
                              int port,
                              const char* url,
                              int error_code,
                              const char* error_text,
                              void* userdata);
} lkPeerObserver;

typedef struct {
  void (*onStateChange)(void* userdata);
  void (*onMessage)(const uint8_t* data,
                    uint64_t size,
                    bool binary,
                    void* userdata);
  void (*onBufferedAmountChange)(uint64_t sentDataSize, void* userdata);
} lkDataChannelObserver;

typedef struct {
  const char** urls;
  int urlsCount;
  const char* username;
  const char* password;
} lkIceServer;

typedef struct {
  lkIceServer* iceServers;
  int iceServersCount;
  lkIceTransportType iceTransportType;
  lkContinualGatheringPolicy gatheringPolicy;
} lkRtcConfiguration;

typedef struct {
  bool reliable;
  bool ordered;
  int maxRetransmits;
} lkDataChannelInit;

typedef struct {
  const char* message;
} lkRtcError;

typedef struct {
  void (*onSuccess)(void* userdata);
  void (*onFailure)(const lkRtcError* error, void* userdata);
} lkSetSdpObserver;

typedef struct {
  void (*onSuccess)(lkSdpType type, const char* sdp, void* userdata);
  void (*onFailure)(const lkRtcError* error, void* userdata);
} lkCreateSdpObserver;

typedef struct {
  bool iceRestart;
  bool useRtpMux;
} lkOfferAnswerOptions;

LK_EXPORT int lkInitialize();
LK_EXPORT int lkDispose();

/* PeerConnection API */

LK_EXPORT void lkAddRef(lkRefCounted* rc);

LK_EXPORT void lkReleaseRef(lkRefCounted* rc);

LK_EXPORT lkPeerFactory* lkCreatePeerFactory();
LK_EXPORT lkPeer* lkCreatePeer(lkPeerFactory* factory,
                               const lkRtcConfiguration* config,
                               const lkPeerObserver* observer,
                               void* userdata);

LK_EXPORT lkDataChannel* lkCreateDataChannel(lkPeer* peer,
                                             const char* label,
                                             const lkDataChannelInit* init);

LK_EXPORT bool lkAddIceCandidate(lkPeer* peer,
                                 const lkIceCandidate* candidate,
                                 void (*onComplete)(lkRtcError* error,
                                                    void* userdata),
                                 void* userdata);

LK_EXPORT bool lkSetLocalDescription(lkPeer* peer,
                                     lkSdpType type,
                                     const char* sdp,
                                     const lkSetSdpObserver* observer,
                                     void* userdata);

LK_EXPORT bool lkSetRemoteDescription(lkPeer* peer,
                                      lkSdpType type,
                                      const char* sdp,
                                      const lkSetSdpObserver* observer,
                                      void* userdata);

LK_EXPORT bool lkCreateOffer(lkPeer* peer,
                             const lkOfferAnswerOptions* options,
                             const lkCreateSdpObserver* observer,
                             void* userdata);

LK_EXPORT bool lkCreateAnswer(lkPeer* peer,
                              const lkOfferAnswerOptions* options,
                              const lkCreateSdpObserver* observer,
                              void* userdata);

LK_EXPORT bool lkPeerSetConfig(lkPeer* peer, const lkRtcConfiguration* config);

LK_EXPORT bool lkPeerClose(lkPeer* peer);

/* DataChannel API */
LK_EXPORT void lkDcRegisterObserver(lkDataChannel* dc,
                                    const lkDataChannelObserver* observer,
                                    void* userdata);

LK_EXPORT void lkDcUnregisterObserver(lkDataChannel* dc);

LK_EXPORT lkDcState lkDcGetState(lkDataChannel* dc);

LK_EXPORT int lkDcGetId(lkDataChannel* dc);

LK_EXPORT void lkDcSendAsync(lkDataChannel* dc,
                             const uint8_t* data,
                             uint64_t size,
                             bool binary,
                             void (*onComplete)(lkRtcError* error,
                                                void* userdata),
                             void* userdata);

LK_EXPORT void lkDcClose(lkDataChannel* dc);

#ifdef __cplusplus
}
#endif

#endif  // LIVEKIT_CAPI_H
