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
typedef void lkRtpReceiver;
typedef void lkRtpSender;
typedef void lkMediaStreamTrack;
typedef void lkMediaStream;
typedef void lkSessionDescription;
typedef void lkIceCandidate;
typedef void lkRtpCapabilities;
typedef void lkRtcVideoTrack;
typedef void lkRtcAudioTrack;
typedef void lkVideoTrackSource;
typedef void lkAudioTrackSource;
typedef void lkNativeAudioSink;

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
  LK_ICE_GATHERING_NEW,
  LK_ICE_GATHERING_GATHERING,
  LK_ICE_GATHERING_COMPLETE,
} lkIceGatheringState;

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

typedef enum {
  LK_RTC_TRACK_STATE_LIVE,
  LK_RTC_TRACK_STATE_ENDED,
} lkRtcTrackState;

typedef enum {
  LK_MEDIA_STREAM_TRACK_KIND_AUDIO,
  LK_MEDIA_STREAM_TRACK_KIND_VIDEO,
  LK_MEDIA_STREAM_TRACK_KIND_DATA,
  LK_MEDIA_STREAM_TRACK_KIND_UNKNOWN,
} lkMediaStreamTrackKind;

typedef struct {
  void (*onSignalingChange)(lkSignalingState state, void* userdata);
  void (*onIceCandidate)(lkIceCandidate* candidate, void* userdata);
  void (*onDataChannel)(const lkDataChannel* dc, void* userdata);
  void (*onTrack)(const lkRtpTransceiver* transceiver, void* userdata);
  void (*onConnectionChange)(lkPeerState state, void* userdata);
  void (*onStandardizedIceConnectionChange)(lkIceState state, void* userdata);
  void (*onIceGatheringChange)(lkIceGatheringState state, void* userdata);
  void (*onRenegotiationNeeded)(void* userdata);
  void (*onIceCandidateError)(const char* address,
                              int port,
                              const char* url,
                              int error_code,
                              const char* error_text,
                              void* userdata);
} lkPeerObserver;

typedef struct {
  void (*onStateChange)(void* userdata, const lkDcState state);
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
  const char* buf;
  int length;
} lkStringOut;

typedef struct {
  void (*onSuccess)(void* userdata);
  void (*onFailure)(const lkRtcError* error, void* userdata);
} lkSetSdpObserver;

typedef struct {
  void (*onSuccess)(lkSessionDescription* desc, void* userdata);
  void (*onFailure)(const lkRtcError* error, void* userdata);
} lkCreateSdpObserver;

typedef struct {
  bool iceRestart;
  bool useRtpMux;
  bool offerToReceiveAudio;
  bool offerToReceiveVideo;
} lkOfferAnswerOptions;

typedef struct {
  bool echoCancellation;
  bool noiseSuppression;
  bool autoGainControl;
} lkAudioSourceOptions;

LK_EXPORT int lkInitialize();
LK_EXPORT int lkDispose();

/* PeerConnection API */

LK_EXPORT void lkAddRef(lkRefCounted* rc);

LK_EXPORT void lkReleaseRef(lkRefCounted* rc);

LK_EXPORT lkPeerFactory* lkCreatePeerFactory();

LK_EXPORT lkRtpCapabilities* lkGetRtpSenderCapabilities(lkPeerFactory* factory);

LK_EXPORT lkRtpCapabilities* lkGetRtpReceiverCapabilities(
    lkPeerFactory* factory);

LK_EXPORT lkPeer* lkCreatePeer(lkPeerFactory* factory,
                               const lkRtcConfiguration* config,
                               const lkPeerObserver* observer,
                               void* userdata);

LK_EXPORT lkDataChannel* lkCreateDataChannel(lkPeer* peer,
                                             const char* label,
                                             const lkDataChannelInit* init);

LK_EXPORT bool lkAddIceCandidate(lkPeer* peer,
                                 lkIceCandidate* candidate,
                                 void (*onComplete)(lkRtcError* error,
                                                    void* userdata),
                                 void* userdata);

LK_EXPORT bool lkSetLocalDescription(lkPeer* peer,
                                     const lkSessionDescription* desc,
                                     const lkSetSdpObserver* observer,
                                     void* userdata);

LK_EXPORT bool lkSetRemoteDescription(lkPeer* peer,
                                      const lkSessionDescription* desc,
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

LK_EXPORT void lkPeerRestartIce(lkPeer* peer);

LK_EXPORT lkPeerState lkGetPeerState(lkPeer* peer);

LK_EXPORT lkIceGatheringState lkPeerGetIceGatheringState(lkPeer* peer);

LK_EXPORT lkIceState lkPeerGetIceConnectionState(lkPeer* peer);

LK_EXPORT lkSignalingState lkPeerGetSignalingState(lkPeer* peer);

LK_EXPORT const lkSessionDescription* lkPeerGetCurrentLocalDescription(
    lkPeer* peer);

LK_EXPORT const lkSessionDescription* lkPeerGetCurrentRemoteDescription(
    lkPeer* peer);

LK_EXPORT bool lkPeerClose(lkPeer* peer);

/* DataChannel API */
LK_EXPORT void lkDcRegisterObserver(lkDataChannel* dc,
                                    const lkDataChannelObserver* observer,
                                    void* userdata);

LK_EXPORT void lkDcUnregisterObserver(lkDataChannel* dc);

LK_EXPORT lkDcState lkDcGetState(lkDataChannel* dc);

LK_EXPORT int lkDcGetId(lkDataChannel* dc);

LK_EXPORT int lkDcGetLabelLength(lkDataChannel* dc);

LK_EXPORT int lkDcGetLabel(lkDataChannel* dc, char* buffer, int bufferSize);

LK_EXPORT uint64_t lkDcGetBufferedAmount(lkDataChannel* dc);

LK_EXPORT void lkDcSendAsync(lkDataChannel* dc,
                             const uint8_t* data,
                             uint64_t size,
                             bool binary,
                             void (*onComplete)(lkRtcError* error,
                                                void* userdata),
                             void* userdata);

LK_EXPORT void lkDcClose(lkDataChannel* dc);

LK_EXPORT lkSessionDescription* lkCreateSessionDescription(lkSdpType type,
                                                           const char* sdp);

LK_EXPORT lkSdpType lkSessionDescriptionGetType(lkSessionDescription* desc);

LK_EXPORT int lkSessionDescriptionGetSdpLength(lkSessionDescription* desc);

LK_EXPORT int lkSessionDescriptionGetSdp(lkSessionDescription* desc,
                                         char* buffer,
                                         int bufferSize);

LK_EXPORT lkIceCandidate* lkCreateIceCandidate(const char* mid,
                                               int mlineIndex,
                                               const char* sdp);

LK_EXPORT int lkIceCandidateGetMlineIndex(lkIceCandidate* candidate);

LK_EXPORT int lkIceCandidateGetMidLength(lkIceCandidate* candidate);

LK_EXPORT int lkIceCandidateGetMid(lkIceCandidate* candidate,
                                   char* buffer,
                                   int bufferSize);

LK_EXPORT int lkIceCandidateGetSdpLength(lkIceCandidate* candidate);

LK_EXPORT int lkIceCandidateGetSdp(lkIceCandidate* candidate,
                                   char* buffer,
                                   int bufferSize);

LK_EXPORT lkNativeAudioSink* lkCreateNativeAudioSink(
    int sample_rate,
    int num_channels,
    void (*onAudioData)(int16_t* audioData,
                        uint32_t sampleRate,
                        uint32_t numberOfChannels,
                        int numberOfFrames,
                        void* userdata),
    void* userdata);

LK_EXPORT lkAudioTrackSource* lkCreateAudioTrackSource(
    lkAudioSourceOptions options,
    int sample_rate,
    int num_channels,
    int queue_size_ms);

LK_EXPORT void lkAudioTrackSourceSetAudioOptions(
    lkAudioTrackSource* source, const lkAudioSourceOptions* options);

LK_EXPORT lkAudioSourceOptions
    lkAudioTrackSourceGetAudioOptions(lkAudioTrackSource* source);

LK_EXPORT bool lkAudioTrackSourceCaptureFrame(
    lkAudioTrackSource* source,
    const int16_t* audio_data,
    uint32_t sample_rate,
    uint32_t number_of_channels,
    int number_of_frames,
    void* userdata,
    void (*onComplete)(void* userdata));

LK_EXPORT void lkAudioTrackSourceClearBuffer(lkAudioTrackSource* source);

LK_EXPORT int lkAudioTrackSourceGetSampleRate(lkAudioTrackSource* source);

LK_EXPORT int lkAudioTrackSourceGetNumChannels(lkAudioTrackSource* source);

LK_EXPORT int lkAudioTrackSourceAddSink(lkAudioTrackSource* source,
                                        lkNativeAudioSink* sink);

LK_EXPORT int lkAudioTrackSourceRemoveSink(lkAudioTrackSource* source,
                                           lkNativeAudioSink* sink);


LK_EXPORT int lkMediaStreamTrackGetIdLength(lkMediaStreamTrack* track);

LK_EXPORT int lkMediaStreamTrackGetId(lkMediaStreamTrack* track,
                                       char* buffer,
                                       int bufferSize);

LK_EXPORT bool lkMediaStreamTrackIsEnabled(lkMediaStreamTrack* track);

LK_EXPORT void lkMediaStreamTrackSetEnabled(lkMediaStreamTrack* track,
                                            bool enabled);

LK_EXPORT lkRtcTrackState lkMediaStreamTrackGetState(lkMediaStreamTrack* track);

LK_EXPORT lkMediaStreamTrackKind lkMediaStreamTrackGetKind(
    lkMediaStreamTrack* track);


#ifdef __cplusplus
}
#endif

#endif  // LIVEKIT_CAPI_H
