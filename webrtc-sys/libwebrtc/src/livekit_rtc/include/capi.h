#ifndef LIVEKIT_CAPI_H
#define LIVEKIT_CAPI_H

#include <stdbool.h>
#include <stdint.h>

#define LK_EXPORT __attribute__((visibility("default")))

#ifdef __cplusplus
extern "C" {
#endif

typedef void lkPlatformImageBuffer;

// Opaque types, mapping to C++ classes
typedef void lkRefCountedObject;

typedef lkRefCountedObject lkString;
typedef lkRefCountedObject lkData;
typedef lkRefCountedObject lkVectorGeneric;
typedef lkRefCountedObject lkPeerFactory;
typedef lkRefCountedObject lkPeer;
typedef lkRefCountedObject lkDataChannel;
typedef lkRefCountedObject lkRtpTransceiver;
typedef lkRefCountedObject lkRtpReceiver;
typedef lkRefCountedObject lkRtpSender;
typedef lkRefCountedObject lkMediaStreamTrack;
typedef lkRefCountedObject lkMediaStream;
typedef lkRefCountedObject lkSessionDescription;
typedef lkRefCountedObject lkIceCandidate;
typedef lkRefCountedObject lkRtpCapabilities;
typedef lkRefCountedObject lkRtcVideoTrack;
typedef lkRefCountedObject lkRtcAudioTrack;
typedef lkRefCountedObject lkVideoTrackSource;
typedef lkRefCountedObject lkAudioTrackSource;
typedef lkRefCountedObject lkNativeAudioSink;
typedef lkRefCountedObject lkNativeAudioStream;
typedef lkRefCountedObject lkVideoFrame;
typedef lkRefCountedObject lkVideoFrameBuffer;
typedef lkRefCountedObject lkPlanarYuvBuffer;
typedef lkRefCountedObject lkPlanarYuv8Buffer;
typedef lkRefCountedObject lkBiplanarYuvBuffer;
typedef lkRefCountedObject lkPlanarYuv16BBuffer;
typedef lkRefCountedObject lkBiplanarYuv8Buffer;
typedef lkRefCountedObject lkI420Buffer;
typedef lkRefCountedObject lkI420ABuffer;
typedef lkRefCountedObject lkI422Buffer;
typedef lkRefCountedObject lkI444Buffer;
typedef lkRefCountedObject lkI010Buffer;
typedef lkRefCountedObject lkNV12Buffer;
typedef lkRefCountedObject lkNativeVideoSink;
typedef lkRefCountedObject lkVideoFrameBuilder;
typedef lkRefCountedObject lkRtpEncodingParameters;
typedef lkRefCountedObject lkRtpTransceiverInit;
typedef lkRefCountedObject lkRtpCapabilities;
typedef lkRefCountedObject lkRtpCodecCapability;
typedef lkRefCountedObject lkRtpHeaderExtensionCapability;
typedef lkRefCountedObject lkRtpParameters;
typedef lkRefCountedObject lkRtpCodecParameters;
typedef lkRefCountedObject lkRtpHeaderExtensionParameters;
typedef lkRefCountedObject lkRtcpParameters;
typedef lkRefCountedObject lkRtpTransceiverInit;
typedef lkRefCountedObject lkDesktopFrame;
typedef lkRefCountedObject lkFrameCryptor;
typedef lkRefCountedObject lkNativeAudioFrame;

typedef enum {
  LK_MEDIA_TYPE_AUDIO,
  LK_MEDIA_TYPE_VIDEO,
  LK_MEDIA_TYPE_DATA,
  LK_MEDIA_TYPE_UNSUPPORTED,
} lkMediaType;

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
  void (*onTrack)(const lkRtpTransceiver* transceiver,
                  const lkRtpReceiver* receiver,
                  const lkVectorGeneric* streams,
                  const lkMediaStreamTrack* track,
                  void* userdata);
  void (*onRemoveTrack)(const lkRtpReceiver* receiver, void* userdata);
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
  void (*onMessage)(const uint8_t* data, uint64_t size, bool binary, void* userdata);
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

typedef struct {
  uint32_t width;
  uint32_t height;
} lkVideoResolution;

typedef enum {
  LK_CONTENT_HINT_NONE,
  LK_CONTENT_HINT_FLUID,
  LK_CONTENT_HINT_DETAILed,
  LK_CONTENT_HINT_TEXT,
} lkContentHint;

typedef struct {
  double minFps;
  double maxFps;
} lkVideoTrackSourceConstraints;

typedef struct {
  void (*onFrame)(const lkVideoFrame* frame, void* userdata);
  void (*onDiscardedFrame)(void* userdata);
  void (*onConstraintsChanged)(lkVideoTrackSourceConstraints* resolution, void* userdata);
} lkVideoSinkCallabacks;

typedef enum {
  LK_VIDEO_BUFFER_TYPE_NATIVE,
  LK_VIDEO_BUFFER_TYPE_I420,
  LK_VIDEO_BUFFER_TYPE_I420A,
  LK_VIDEO_BUFFER_TYPE_I422,
  LK_VIDEO_BUFFER_TYPE_I444,
  LK_VIDEO_BUFFER_TYPE_I010,
  LK_VIDEO_BUFFER_TYPE_NV12,
} lkVideoBufferType;

typedef enum {
  LK_VIDEO_ROTATION_0,
  LK_VIDEO_ROTATION_90,
  LK_VIDEO_ROTATION_180,
  LK_VIDEO_ROTATION_270,
} lkVideoRotation;

typedef enum {
  LK_RTP_TRANSCEIVER_DIRECTION_SENDRECV,
  LK_RTP_TRANSCEIVER_DIRECTION_SENDONLY,
  LK_RTP_TRANSCEIVER_DIRECTION_RECVONLY,
  LK_RTP_TRANSCEIVER_DIRECTION_INACTIVE,
  LK_RTP_TRANSCEIVER_DIRECTION_STOPPED,
} lkRtpTransceiverDirection;

typedef enum {
  FEC_MECHANISM_ULPFEC = 1,
  FEC_MECHANISM_RED = 2,
} FecMechanism;

typedef enum {
  kVeryLow,
  kLow,
  kMedium,
  kHigh,
} lkNetworkPriority;

LK_EXPORT int lkInitialize();

LK_EXPORT int lkDispose();

/* PeerConnection API */

LK_EXPORT void lkAddRef(lkRefCountedObject* rc);

LK_EXPORT void lkReleaseRef(lkRefCountedObject* rc);

LK_EXPORT lkString* lkCreateString(const char* str);

LK_EXPORT int lkStringGetLength(lkString* str);

LK_EXPORT int lkStringGetData(lkString* str, char* buffer, int bufferSize);

LK_EXPORT lkData* lkCreateData(const uint8_t* data, uint32_t size);

LK_EXPORT int lkDataGetSize(lkData* data);

LK_EXPORT const uint8_t* lkDataGetData(lkData* data);

LK_EXPORT lkVectorGeneric* lkCreateVectorGeneric();

LK_EXPORT uint32_t lkVectorGenericGetSize(lkVectorGeneric* vec);

LK_EXPORT lkRefCountedObject* lkVectorGenericGetAt(lkVectorGeneric* vec, uint32_t index);

LK_EXPORT uint32_t lkVectorGenericPushBack(lkVectorGeneric* vec, lkRefCountedObject* value);

LK_EXPORT lkPeerFactory* lkCreatePeerFactory();

LK_EXPORT lkRtpCapabilities* lkGetRtpSenderCapabilities(lkPeerFactory* factory, lkMediaType type);

LK_EXPORT lkRtpCapabilities* lkGetRtpReceiverCapabilities(lkPeerFactory* factory, lkMediaType type);

LK_EXPORT lkVectorGeneric* lkRtpCapabilitiesGetCodecs(lkRtpCapabilities* capabilities);

LK_EXPORT lkVectorGeneric* lkRtpCapabilitiesGetHeaderExtensions(lkRtpCapabilities* capabilities);

LK_EXPORT lkPeer* lkCreatePeer(lkPeerFactory* factory,
                               const lkRtcConfiguration* config,
                               const lkPeerObserver* observer,
                               void* userdata);

LK_EXPORT lkDataChannel* lkCreateDataChannel(lkPeer* peer,
                                             const char* label,
                                             const lkDataChannelInit* init);

LK_EXPORT bool lkAddIceCandidate(lkPeer* peer,
                                 lkIceCandidate* candidate,
                                 void (*onComplete)(lkRtcError* error, void* userdata),
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

LK_EXPORT lkRtpSender* lkPeerAddTrack(lkPeer* peer,
                                      lkMediaStreamTrack* track,
                                      const char** streamIds,
                                      int streamIdCount,
                                      lkRtcError* error);

LK_EXPORT bool lkPeerRemoveTrack(lkPeer* peer, lkRtpSender* sender, lkRtcError* error);

LK_EXPORT lkRtpTransceiver* lkPeerAddTransceiver(lkPeer* peer,
                                                 lkMediaStreamTrack* track,
                                                 lkRtpTransceiverInit* init,
                                                 lkRtcError* error);

LK_EXPORT lkRtpTransceiver* lkPeerAddTransceiverForMedia(lkPeer* peer,
                                                         lkMediaType type,
                                                         lkRtpTransceiverInit* init,
                                                         lkRtcError* error);

LK_EXPORT lkVectorGeneric* lkPeerGetTransceivers(lkPeer* peer);

LK_EXPORT lkVectorGeneric* lkPeerGetSenders(lkPeer* peer);

LK_EXPORT lkVectorGeneric* lkPeerGetReceivers(lkPeer* peer);

LK_EXPORT bool lkPeerSetConfig(lkPeer* peer, const lkRtcConfiguration* config);

LK_EXPORT void lkPeerRestartIce(lkPeer* peer);

LK_EXPORT lkPeerState lkGetPeerState(lkPeer* peer);

LK_EXPORT lkIceGatheringState lkPeerGetIceGatheringState(lkPeer* peer);

LK_EXPORT lkIceState lkPeerGetIceConnectionState(lkPeer* peer);

LK_EXPORT lkSignalingState lkPeerGetSignalingState(lkPeer* peer);

LK_EXPORT const lkSessionDescription* lkPeerGetCurrentLocalDescription(lkPeer* peer);

LK_EXPORT const lkSessionDescription* lkPeerGetCurrentRemoteDescription(lkPeer* peer);

LK_EXPORT bool lkPeerClose(lkPeer* peer);

/* DataChannel API */
LK_EXPORT void lkDcRegisterObserver(lkDataChannel* dc,
                                    const lkDataChannelObserver* observer,
                                    void* userdata);

LK_EXPORT void lkDcUnregisterObserver(lkDataChannel* dc);

LK_EXPORT lkDcState lkDcGetState(lkDataChannel* dc);

LK_EXPORT int lkDcGetId(lkDataChannel* dc);

LK_EXPORT lkString* lkDcGetLabel(lkDataChannel* dc);

LK_EXPORT uint64_t lkDcGetBufferedAmount(lkDataChannel* dc);

LK_EXPORT void lkDcSendAsync(lkDataChannel* dc,
                             const uint8_t* data,
                             uint64_t size,
                             bool binary,
                             void (*onComplete)(lkRtcError* error, void* userdata),
                             void* userdata);

LK_EXPORT void lkDcClose(lkDataChannel* dc);

LK_EXPORT lkSessionDescription* lkCreateSessionDescription(lkSdpType type, const char* sdp);

LK_EXPORT lkSdpType lkSessionDescriptionGetType(lkSessionDescription* desc);

LK_EXPORT lkString* lkSessionDescriptionGetSdp(lkSessionDescription* desc);

LK_EXPORT lkIceCandidate* lkCreateIceCandidate(const char* mid, int mlineIndex, const char* sdp);

LK_EXPORT int lkIceCandidateGetMlineIndex(lkIceCandidate* candidate);

LK_EXPORT lkString* lkIceCandidateGetMid(lkIceCandidate* candidate);

LK_EXPORT lkString* lkIceCandidateGetSdp(lkIceCandidate* candidate);

LK_EXPORT lkNativeAudioSink* lkCreateNativeAudioSink(int sample_rate,
                                                     int num_channels,
                                                     void (*onAudioData)(int16_t* audioData,
                                                                         uint32_t sampleRate,
                                                                         uint32_t numberOfChannels,
                                                                         int numberOfFrames,
                                                                         void* userdata),
                                                     void* userdata);

LK_EXPORT lkAudioTrackSource* lkCreateAudioTrackSource(lkAudioSourceOptions options,
                                                       int sample_rate,
                                                       int num_channels,
                                                       int queue_size_ms);

LK_EXPORT void lkAudioTrackSourceSetAudioOptions(lkAudioTrackSource* source,
                                                 const lkAudioSourceOptions* options);

LK_EXPORT lkAudioSourceOptions lkAudioTrackSourceGetAudioOptions(lkAudioTrackSource* source);

LK_EXPORT bool lkAudioTrackSourceCaptureFrame(lkAudioTrackSource* source,
                                              const int16_t* audio_data,
                                              uint32_t sample_rate,
                                              uint32_t number_of_channels,
                                              int number_of_frames,
                                              void* userdata,
                                              void (*onComplete)(void* userdata));

LK_EXPORT void lkAudioTrackSourceClearBuffer(lkAudioTrackSource* source);

LK_EXPORT int lkAudioTrackSourceGetSampleRate(lkAudioTrackSource* source);

LK_EXPORT int lkAudioTrackSourceGetNumChannels(lkAudioTrackSource* source);

LK_EXPORT int lkAudioTrackSourceAddSink(lkAudioTrackSource* source, lkNativeAudioSink* sink);

LK_EXPORT int lkAudioTrackSourceRemoveSink(lkAudioTrackSource* source, lkNativeAudioSink* sink);

LK_EXPORT lkString* lkMediaStreamTrackGetId(lkMediaStreamTrack* track);

LK_EXPORT bool lkMediaStreamTrackIsEnabled(lkMediaStreamTrack* track);

LK_EXPORT void lkMediaStreamTrackSetEnabled(lkMediaStreamTrack* track, bool enabled);

LK_EXPORT lkRtcTrackState lkMediaStreamTrackGetState(lkMediaStreamTrack* track);

LK_EXPORT lkMediaStreamTrackKind lkMediaStreamTrackGetKind(lkMediaStreamTrack* track);

LK_EXPORT lkRtcAudioTrack* lkPeerFactoryCreateAudioTrack(lkPeerFactory* factory,
                                                         const char* id,
                                                         lkAudioTrackSource* source);

LK_EXPORT lkRtcVideoTrack* lkPeerFactoryCreateVideoTrack(lkPeerFactory* factory,
                                                         const char* id,
                                                         lkVideoTrackSource* source);

LK_EXPORT void lkAudioTrackAddSink(lkRtcAudioTrack* track, lkNativeAudioSink* sink);

LK_EXPORT void lkAudioTrackRemoveSink(lkRtcVideoTrack* track, lkNativeAudioSink* sink);

LK_EXPORT lkString* lkMediaStreamGetId(lkMediaStream* stream);

LK_EXPORT lkVectorGeneric* lkMediaStreamGetAudioTracks(lkMediaStream* stream);

LK_EXPORT lkVectorGeneric* lkMediaStreamGetVideoTracks(lkMediaStream* stream);

LK_EXPORT lkNativeVideoSink* lkCreateNativeVideoSink(const lkVideoSinkCallabacks* callbacks,
                                                     void* userdata);

LK_EXPORT void lkVideoTrackAddSink(lkRtcVideoTrack* source, lkNativeVideoSink* sink);

LK_EXPORT void lkVideoTrackRemoveSink(lkRtcVideoTrack* source, lkNativeVideoSink* sink);

LK_EXPORT lkVideoTrackSource* lkCreateVideoTrackSource(lkVideoResolution resolution);

LK_EXPORT lkVideoResolution lkVideoTrackSourceGetResolution(lkVideoTrackSource* source);

LK_EXPORT void lkVideoTrackSourceOnCaptureFrame(lkVideoTrackSource* source, lkVideoFrame* frame);

LK_EXPORT lkVideoBufferType lkVideoFrameBufferGetType(lkVideoFrameBuffer* frameBuffer);

LK_EXPORT uint32_t lkVideoFrameBufferGetWidth(lkVideoFrameBuffer* frameBuffer);

LK_EXPORT uint32_t lkVideoFrameBufferGetHeight(lkVideoFrameBuffer* frameBuffer);

LK_EXPORT lkI420Buffer* lkI420BufferNew(
    uint32_t width, uint32_t height, uint32_t stride_y, uint32_t stride_u, uint32_t stride_v);

LK_EXPORT lkI420Buffer* lkVideoFrameBufferToI420(lkVideoFrameBuffer* frameBuffer);

LK_EXPORT lkI420Buffer* lkVideoFrameBufferGetI420(lkVideoFrameBuffer* frameBuffer);

LK_EXPORT lkI420ABuffer* lkVideoFrameBufferGetI420A(lkVideoFrameBuffer* frameBuffer);

LK_EXPORT lkI422Buffer* lkVideoFrameBufferGetI422(lkVideoFrameBuffer* frameBuffer);

LK_EXPORT lkI444Buffer* lkVideoFrameBufferGetI444(lkVideoFrameBuffer* frameBuffer);

LK_EXPORT lkI010Buffer* lkVideoFrameBufferGetI010(lkVideoFrameBuffer* frameBuffer);

LK_EXPORT lkNV12Buffer* lkVideoFrameBufferGetNV12(lkVideoFrameBuffer* frameBuffer);

LK_EXPORT uint32_t lkI420BufferGetChromaWidth(lkI420Buffer* buffer);

LK_EXPORT uint32_t lkI420BufferGetChromaHeight(lkI420Buffer* buffer);

LK_EXPORT uint32_t lkI420BufferGetStrideY(lkI420Buffer* buffer);

LK_EXPORT uint32_t lkI420BufferGetStrideU(lkI420Buffer* buffer);

LK_EXPORT uint32_t lkI420BufferGetStrideV(lkI420Buffer* buffer);

LK_EXPORT const uint8_t* lkI420BufferGetDataY(lkI420Buffer* buffer);

LK_EXPORT const uint8_t* lkI420BufferGetDataU(lkI420Buffer* buffer);

LK_EXPORT const uint8_t* lkI420BufferGetDataV(lkI420Buffer* buffer);

LK_EXPORT lkI420Buffer* lkI420BufferScale(lkI420Buffer* buffer, int scaledWidth, int scaledHeight);

LK_EXPORT uint32_t lkI420ABufferGetChromaWidth(lkI420ABuffer* buffer);

LK_EXPORT uint32_t lkI420ABufferGetChromaHeight(lkI420ABuffer* buffer);

LK_EXPORT uint32_t lkI420ABufferGetStrideY(lkI420ABuffer* buffer);

LK_EXPORT uint32_t lkI420ABufferGetStrideU(lkI420ABuffer* buffer);

LK_EXPORT uint32_t lkI420ABufferGetStrideV(lkI420ABuffer* buffer);

LK_EXPORT uint32_t lkI420ABufferGetStrideA(lkI420ABuffer* buffer);

LK_EXPORT const uint8_t* lkI420ABufferGetDataY(lkI420ABuffer* buffer);

LK_EXPORT const uint8_t* lkI420ABufferGetDataU(lkI420ABuffer* buffer);

LK_EXPORT const uint8_t* lkI420ABufferGetDataV(lkI420ABuffer* buffer);

LK_EXPORT const uint8_t* lkI420ABufferGetDataA(lkI420ABuffer* buffer);

LK_EXPORT lkI420ABuffer* lkI420ABufferScale(lkI420ABuffer* buffer,
                                            int scaledWidth,
                                            int scaledHeight);

LK_EXPORT lkI422Buffer* lkI422BufferNew(
    uint32_t width, uint32_t height, uint32_t stride_y, uint32_t stride_u, uint32_t stride_v);

LK_EXPORT uint32_t lkI422BufferGetChromaWidth(lkI422Buffer* buffer);

LK_EXPORT uint32_t lkI422BufferGetChromaHeight(lkI422Buffer* buffer);

LK_EXPORT uint32_t lkI422BufferGetStrideY(lkI422Buffer* buffer);

LK_EXPORT uint32_t lkI422BufferGetStrideU(lkI422Buffer* buffer);

LK_EXPORT uint32_t lkI422BufferGetStrideV(lkI422Buffer* buffer);

LK_EXPORT const uint8_t* lkI422BufferGetDataY(lkI422Buffer* buffer);

LK_EXPORT const uint8_t* lkI422BufferGetDataU(lkI422Buffer* buffer);

LK_EXPORT const uint8_t* lkI422BufferGetDataV(lkI422Buffer* buffer);

LK_EXPORT lkI422Buffer* lkI422BufferScale(lkI422Buffer* buffer, int scaledWidth, int scaledHeight);

LK_EXPORT lkI444Buffer* lkI444BufferNew(
    uint32_t width, uint32_t height, uint32_t stride_y, uint32_t stride_u, uint32_t stride_v);
LK_EXPORT uint32_t lkI444BufferGetChromaWidth(lkI444Buffer* buffer);

LK_EXPORT uint32_t lkI444BufferGetChromaHeight(lkI444Buffer* buffer);

LK_EXPORT uint32_t lkI444BufferGetStrideY(lkI444Buffer* buffer);

LK_EXPORT uint32_t lkI444BufferGetStrideU(lkI444Buffer* buffer);

LK_EXPORT uint32_t lkI444BufferGetStrideV(lkI444Buffer* buffer);

LK_EXPORT const uint8_t* lkI444BufferGetDataY(lkI444Buffer* buffer);

LK_EXPORT const uint8_t* lkI444BufferGetDataU(lkI444Buffer* buffer);

LK_EXPORT const uint8_t* lkI444BufferGetDataV(lkI444Buffer* buffer);

LK_EXPORT lkI444Buffer* lkI444BufferScale(lkI444Buffer* buffer, int scaledWidth, int scaledHeight);

LK_EXPORT lkI010Buffer* lkI010BufferNew(
    uint32_t width, uint32_t height, uint32_t stride_y, uint32_t stride_u, uint32_t stride_v);
LK_EXPORT uint32_t lkI010BufferGetChromaWidth(lkI010Buffer* buffer);

LK_EXPORT uint32_t lkI010BufferGetChromaHeight(lkI010Buffer* buffer);

LK_EXPORT uint32_t lkI010BufferGetStrideY(lkI010Buffer* buffer);

LK_EXPORT uint32_t lkI010BufferGetStrideU(lkI010Buffer* buffer);

LK_EXPORT uint32_t lkI010BufferGetStrideV(lkI010Buffer* buffer);

LK_EXPORT const uint16_t* lkI010BufferGetDataY(lkI010Buffer* buffer);

LK_EXPORT const uint16_t* lkI010BufferGetDataU(lkI010Buffer* buffer);

LK_EXPORT const uint16_t* lkI010BufferGetDataV(lkI010Buffer* buffer);

LK_EXPORT lkI010Buffer* lkI010BufferScale(lkI010Buffer* buffer, int scaledWidth, int scaledHeight);

LK_EXPORT lkNV12Buffer* lkNV12BufferNew(uint32_t width,
                                        uint32_t height,
                                        uint32_t stride_y,
                                        uint32_t stride_uv);

LK_EXPORT uint32_t lkNV12BufferGetChromaWidth(lkNV12Buffer* buffer);

LK_EXPORT uint32_t lkNV12BufferGetChromaHeight(lkNV12Buffer* buffer);

LK_EXPORT uint32_t lkNV12BufferGetStrideY(lkNV12Buffer* buffer);

LK_EXPORT uint32_t lkNV12BufferGetStrideUV(lkNV12Buffer* buffer);

LK_EXPORT const uint8_t* lkNV12BufferGetDataY(lkNV12Buffer* buffer);

LK_EXPORT const uint8_t* lkNV12BufferGetDataUV(lkNV12Buffer* buffer);

LK_EXPORT lkNV12Buffer* lkNV12BufferScale(lkNV12Buffer* buffer, int scaledWidth, int scaledHeight);

LK_EXPORT void lkVideoFrameBufferToARGB(lkVideoFrameBuffer* frameBuffer,
                                        lkVideoBufferType type,
                                        uint8_t* argbBuffer,
                                        uint32_t stride,
                                        uint32_t width,
                                        uint32_t height);

LK_EXPORT lkVideoFrameBuffer* lkNewNativeBufferFromPlatformImageBuffer(
    lkPlatformImageBuffer* buffer);

LK_EXPORT lkPlatformImageBuffer* lkNativeBufferToPlatformImageBuffer(
    lkVideoFrameBuffer* frameBuffer);

LK_EXPORT lkVideoFrameBuilder* lkCreateVideoFrameBuilder();

LK_EXPORT void lkVideoFrameBuilderSetVideoFrameBuffer(lkVideoFrameBuilder* builder,
                                                      lkVideoFrameBuffer* buffer);

LK_EXPORT void lkVideoFrameBuilderSetTimestampUs(lkVideoFrameBuilder* builder, int64_t timestampNs);

LK_EXPORT void lkVideoFrameBuilderSetRotation(lkVideoFrameBuilder* builder,
                                              lkVideoRotation rotation);

LK_EXPORT void lkVideoFrameBuilderSetId(lkVideoFrameBuilder* builder, uint16_t id);

LK_EXPORT lkVideoFrame* lkVideoFrameBuilderBuild(lkVideoFrameBuilder* builder);

LK_EXPORT lkVideoRotation lkVideoFrameGetRotation(const lkVideoFrame* frame);

LK_EXPORT int64_t lkVideoFrameGetTimestampUs(const lkVideoFrame* frame);

LK_EXPORT uint16_t lkVideoFrameGetId(const lkVideoFrame* frame);

LK_EXPORT lkVideoFrameBuffer* lkVideoFrameGetBuffer(const lkVideoFrame* frame);

LK_EXPORT lkMediaStreamTrack* lkRtpSenderGetTrack(lkRtpSender* sender);

LK_EXPORT bool lkRtpSenderSetTrack(lkRtpSender* sender, lkMediaStreamTrack* track);

LK_EXPORT lkString* lkRtpTransceiverGetMid(lkRtpTransceiver* transceiver);

LK_EXPORT lkRtpTransceiverDirection lkRtpTransceiverGetDirection(lkRtpTransceiver* transceiver);

LK_EXPORT lkRtpTransceiverDirection lkRtpTransceiverCurrentDirection(lkRtpTransceiver* transceiver);

LK_EXPORT lkRtpSender* lkRtpTransceiverGetSender(lkRtpTransceiver* transceiver);

LK_EXPORT lkRtpReceiver* lkRtpTransceiverGetReceiver(lkRtpTransceiver* transceiver);

LK_EXPORT void lkRtpTransceiverStop(lkRtpTransceiver* transceiver);

LK_EXPORT lkMediaStreamTrack* lkRtpReceiverGetTrack(lkRtpReceiver* receiver);

LK_EXPORT void lkPeerGetStats(lkPeer* peer,
                              void (*onComplete)(const char* statsJson, void* userdata),
                              void* userdata);

LK_EXPORT void lkRtpSenderGetStats(lkRtpSender* sender,
                                   void (*onComplete)(const char* statsJson, void* userdata),
                                   void* userdata);

LK_EXPORT void lkRtpReceiverGetStats(lkRtpReceiver* receiver,
                                     void (*onComplete)(const char* statsJson, void* userdata),
                                     void* userdata);

LK_EXPORT lkRtpCodecCapability* lkRtpCodecCapabilityCreate();

LK_EXPORT void lkRtpCodecCapabilitySetMimeType(lkRtpCodecCapability* codec, const char* mimeType);

LK_EXPORT void lkRtpCodecCapabilitySetClockRate(lkRtpCodecCapability* codec, uint32_t clockRate);

LK_EXPORT void lkRtpCodecCapabilitySetChannels(lkRtpCodecCapability* codec, uint16_t channels);

LK_EXPORT void lkRtpCodecCapabilitySetSdpFmtpLine(lkRtpCodecCapability* codec,
                                                  const char* sdpFmtpLine);

LK_EXPORT uint16_t lkRtpCodecCapabilityGetChannels(lkRtpCodecCapability* codec);

LK_EXPORT uint32_t lkRtpCodecCapabilityGetClockRate(lkRtpCodecCapability* codec);

LK_EXPORT lkString* lkRtpCodecCapabilityGetMimeType(lkRtpCodecCapability* codec);

LK_EXPORT lkString* lkRtpCodecCapabilityGetSdpFmtpLine(lkRtpCodecCapability* codec);

LK_EXPORT lkString* lkRtpHeaderExtensionCapabilityGetUri(lkRtpHeaderExtensionCapability* ext);

LK_EXPORT lkRtpTransceiverDirection
    lkRtpHeaderExtensionCapabilityGetDirection(lkRtpHeaderExtensionCapability* ext);

/*
lkRtcpParametersGetCname
lkRtcpParametersGetReducedSize
lkRtpCodecParametersGetPayloadType
lkRtpCodecParametersGetMimeType
lkRtpCodecParametersGetClockRate
lkRtpCodecParametersGetChannels
lkRtpHeaderExtensionParametersGetUri
lkRtpHeaderExtensionParametersGetId
lkRtpHeaderExtensionParametersGetEncrypted
lkRtpParametersGetCodecs
lkRtpParametersGetRtcp
lkRtpParametersGetHeaderExtensions
*/

LK_EXPORT lkString* lkRtcpParametersGetCname(lkRtcpParameters* rtcp);

LK_EXPORT bool lkRtcpParametersGetReducedSize(lkRtcpParameters* rtcp);

LK_EXPORT uint8_t lkRtpCodecParametersGetPayloadType(lkRtpCodecParameters* codec);

LK_EXPORT lkString* lkRtpCodecParametersGetMimeType(lkRtpCodecParameters* codec);

LK_EXPORT uint32_t lkRtpCodecParametersGetClockRate(lkRtpCodecParameters* codec);

LK_EXPORT uint16_t lkRtpCodecParametersGetChannels(lkRtpCodecParameters* codec);

LK_EXPORT lkString* lkRtpHeaderExtensionParametersGetUri(lkRtpHeaderExtensionParameters* ext);

LK_EXPORT uint8_t lkRtpHeaderExtensionParametersGetId(lkRtpHeaderExtensionParameters* ext);

LK_EXPORT bool lkRtpHeaderExtensionParametersGetEncrypted(lkRtpHeaderExtensionParameters* ext);

LK_EXPORT lkVectorGeneric* lkRtpParametersGetCodecs(lkRtpParameters* params);

LK_EXPORT lkRtcpParameters* lkRtpParametersGetRtcp(lkRtpParameters* params);

LK_EXPORT lkVectorGeneric* lkRtpParametersGetHeaderExtensions(lkRtpParameters* params);

LK_EXPORT lkRtpParameters* lkRtpSenderGetParameters(lkRtpSender* sender);

LK_EXPORT bool lkRtpSenderSetParameters(lkRtpSender* sender,
                                        lkRtpParameters* params,
                                        lkRtcError* error);

LK_EXPORT lkRtpParameters* lkRtpReceiverGetParameters(lkRtpReceiver* receiver);

LK_EXPORT lkRtpTransceiverInit* lkRtpTransceiverInitCreate();

LK_EXPORT void lkRtpTransceiverInitSetDirection(lkRtpTransceiverInit* init,
                                                lkRtpTransceiverDirection direction);

LK_EXPORT void lkRtpTransceiverInitSetStreamIds(lkRtpTransceiverInit* init,
                                                lkVectorGeneric* streamIds);

LK_EXPORT lkRtpTransceiverDirection lkRtpTransceiverInitGetDirection(lkRtpTransceiverInit* init);

LK_EXPORT void lkRtpTransceiverInitSetSendEncodingsdings(lkRtpTransceiverInit* init,
                                                         lkVectorGeneric* encodings);

LK_EXPORT bool lkRtpTransceiverSetCodecPreferences(lkRtpTransceiver* transceiver,
                                                   lkVectorGeneric* codecs,
                                                   lkRtcError* error);

LK_EXPORT bool lkRtpTransceiverStopWithError(lkRtpTransceiver* transceiver, lkRtcError* error);

LK_EXPORT lkRtpEncodingParameters* lkRtpEncodingParametersCreate();

LK_EXPORT void lkRtpEncodingParametersSetActive(lkRtpEncodingParameters* encoding, bool active);

LK_EXPORT void lkRtpEncodingParametersSetMaxBitrateBps(lkRtpEncodingParameters* encoding,
                                                       int64_t maxBitrateBps);

LK_EXPORT void lkRtpEncodingParametersSetMinBitrateBps(lkRtpEncodingParameters* encoding,
                                                       int64_t minBitrateBps);

LK_EXPORT void lkRtpEncodingParametersSetMaxFramerate(lkRtpEncodingParameters* encoding,
                                                      double maxFramerate);

LK_EXPORT void lkRtpEncodingParametersSetScaleResolutionDownBy(lkRtpEncodingParameters* encoding,
                                                               double scaleResolutionDownBy);

LK_EXPORT void lkRtpEncodingParametersSetRid(lkRtpEncodingParameters* encoding, const char* rid);

LK_EXPORT void lkRtpEncodingParametersSetScalabilityMode(lkRtpEncodingParameters* encoding,
                                                         const char* scalabilityMode);

LK_EXPORT lkRtpParameters* lkRtpParametersCreate();

LK_EXPORT lkRtcpParameters* lkRtcpParametersCreate();

LK_EXPORT void lkRtpParametersSetCodecs(lkRtpParameters* params, lkVectorGeneric* codecs);

LK_EXPORT void lkRtpParametersSetRtcp(lkRtpParameters* params, lkRtcpParameters* rtcp);

LK_EXPORT void lkRtcpParametersSetReducedSize(lkRtcpParameters* rtcp, bool reducedSize);

LK_EXPORT void lkRtcpParametersSetCname(lkRtcpParameters* rtcp, const char* cname);

LK_EXPORT void lkRtpParametersSetHeaderExtensions(lkRtpParameters* params,
                                                  lkVectorGeneric* headerExtensions);

LK_EXPORT lkRtpCodecParameters* lkRtpCodecParametersCreate();

LK_EXPORT void lkRtpCodecParametersSetPayloadType(lkRtpCodecParameters* codec,
                                                  uint32_t payloadType);

LK_EXPORT void lkRtpCodecParametersSetMimeType(lkRtpCodecParameters* codec, const char* mimeType);

LK_EXPORT void lkRtpCodecParametersSetClockRate(lkRtpCodecParameters* codec, uint32_t clockRate);

LK_EXPORT void lkRtpCodecParametersSetChannels(lkRtpCodecParameters* codec, uint32_t channels);

LK_EXPORT lkRtpHeaderExtensionParameters* lkRtpHeaderExtensionParametersCreate();

LK_EXPORT void lkRtpHeaderExtensionParametersSetUri(lkRtpHeaderExtensionParameters* ext,
                                                    const char* uri);

LK_EXPORT void lkRtpHeaderExtensionParametersSetId(lkRtpHeaderExtensionParameters* ext,
                                                   uint32_t id);

LK_EXPORT void lkRtpHeaderExtensionParametersSetEncrypted(lkRtpHeaderExtensionParameters* ext,
                                                          bool encrypted);

#ifdef __cplusplus
}
#endif

#endif  // LIVEKIT_CAPI_H
