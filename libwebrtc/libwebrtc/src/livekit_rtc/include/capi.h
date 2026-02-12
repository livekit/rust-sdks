#ifndef LIVEKIT_CAPI_H
#define LIVEKIT_CAPI_H

#include <stdbool.h>
#include <stdint.h>

#ifdef WIN32
#if defined(LIVEKIT_RTC_API_EXPORTS)
#define LK_EXPORT __declspec(dllexport)
#elif defined(LIVEKIT_RTC_API_STATIC)
#define LK_EXPORT
#else
#define LK_EXPORT __declspec(dllimport)
#endif
#else
#define LK_EXPORT __attribute__((visibility("default")))
#endif


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
typedef lkRefCountedObject lkFrameCryptor;
typedef lkRefCountedObject lkNativeAudioFrame;
typedef lkRefCountedObject lkKeyProviderOptions;
typedef lkRefCountedObject lkKeyProvider;
typedef lkRefCountedObject lkDataPacketCryptor;
typedef lkRefCountedObject lkEncryptedPacket;
typedef lkRefCountedObject lkDesktopCapturer;
typedef lkRefCountedObject lkDesktopFrame;
typedef lkRefCountedObject lkDesktopSource;
typedef lkRefCountedObject lkAudioMixer;
typedef lkRefCountedObject lkAudioResampler;
typedef lkRefCountedObject lkAudioProcessingModule;
typedef lkRefCountedObject lkRtcpFeedback;

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

typedef enum {
  AUDIO_FRAME_INFO_NORMAL,
  AUDIO_FRAME_INFO_MUTE,
  AUDIO_FRAME_INFO_ERROR,
} lkAudioFrameInfo;

typedef struct {
  int32_t (*getSsrc)(void* userdata);
  int32_t (*preferredSampleRate)(void* userdata);
  lkAudioFrameInfo (*getAudioFrameWithInfo)(uint32_t targetSampleRate,
                                            lkNativeAudioFrame* frame,
                                            void* userdata);
} lkAudioMixerSourceCallback;

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
  void (*onConstraintsChanged)(lkVideoTrackSourceConstraints* resolution,
                               void* userdata);
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
} lkFecMechanism;

typedef enum {
  RTCP_FEEDBACK_MESSAGE_TYPE_GENERIC_NACK = 0,
  RTCP_FEEDBACK_MESSAGE_TYPE_PLI = 1,
  RTCP_FEEDBACK_MESSAGE_TYPE_FIR = 2,
} lkRtcpFeedbackMessageType;

typedef enum {
  RTCP_FEEDBACK_TYPE_CCM,
  RTCP_FEEDBACK_TYPE_LNTP,
  RTCP_FEEDBACK_TYPE_NACK,
  RTCP_FEEDBACK_TYPE_REMB,
  RTCP_FEEDBACK_TYPE_TRANSPORT_CC,
} lkRtcpFeedbackType;

typedef enum {
  ENCRYPTION_ALGORITHM_AES_GCM,
  ENCRYPTION_ALGORITHM_AES_CBC,
} lkEncryptionAlgorithm;

typedef enum {
  ENCRYPTION_STATE_NEW,
  ENCRYPTION_STATE_OK,
  ENCRYPTION_STATE_ENCRYPTION_FAILED,
  ENCRYPTION_STATE_DECRYPTION_FAILED,
  ENCRYPTION_STATE_MISSING_KEY,
  ENCRYPTION_STATE_KEY_RATCHETED,
  ENCRYPTION_STATE_INTERNAL_ERROR,
} lkEncryptionState;

typedef enum {
  SOURCE_TYPE_SCREEN,
  SOURCE_TYPE_WINDOW,
  SOURCE_TYPE_GENERIC,
} lkSourceType;

typedef enum {
    NETWORK_PRIORITY_VERY_LOW,
    NETWORK_PRIORITY_LOW,
    NETWORK_PRIORITY_MEDIUM,
    NETWORK_PRIORITY_HIGH,
} lkNetworkPriority;

typedef struct {
  bool allow_sck_system_picker;
  lkSourceType source_type;
  bool include_cursor;
} lkDesktopCapturerOptions;

typedef enum {
  CAPTURE_RESULT_SUCCESS,
  CAPTURE_RESULT_ERROR_TEMPORARY,
  CAPTURE_RESULT_ERROR_PERMANENT,
} lkCaptureResult;

typedef enum {
  CAPTURE_ERROR_TEMPORARY,
  CAPTURE_ERROR_PERMANENT,
} lkCaptureError;

LK_EXPORT void lkInitAndroid(void* jvm);

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

LK_EXPORT lkRefCountedObject* lkVectorGenericGetAt(lkVectorGeneric* vec,
                                                   uint32_t index);

LK_EXPORT uint32_t lkVectorGenericPushBack(lkVectorGeneric* vec,
                                           lkRefCountedObject* value);

LK_EXPORT lkPeerFactory* lkCreatePeerFactory();

LK_EXPORT lkRtpCapabilities* lkGetRtpSenderCapabilities(lkPeerFactory* factory,
                                                        lkMediaType type);

LK_EXPORT lkRtpCapabilities* lkGetRtpReceiverCapabilities(
    lkPeerFactory* factory,
    lkMediaType type);

LK_EXPORT lkVectorGeneric* lkRtpCapabilitiesGetCodecs(
    lkRtpCapabilities* capabilities);

LK_EXPORT lkVectorGeneric* lkRtpCapabilitiesGetHeaderExtensions(
    lkRtpCapabilities* capabilities);

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

LK_EXPORT lkRtpSender* lkPeerAddTrack(lkPeer* peer,
                                      lkMediaStreamTrack* track,
                                      const char** streamIds,
                                      int streamIdCount,
                                      lkRtcError* error);

LK_EXPORT bool lkPeerRemoveTrack(lkPeer* peer,
                                 lkRtpSender* sender,
                                 lkRtcError* error);

LK_EXPORT lkRtpTransceiver* lkPeerAddTransceiver(lkPeer* peer,
                                                 lkMediaStreamTrack* track,
                                                 lkRtpTransceiverInit* init,
                                                 lkRtcError* error);

LK_EXPORT lkRtpTransceiver* lkPeerAddTransceiverForMedia(
    lkPeer* peer,
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

LK_EXPORT lkString* lkDcGetLabel(lkDataChannel* dc);

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

LK_EXPORT lkString* lkSessionDescriptionGetSdp(lkSessionDescription* desc);

LK_EXPORT lkIceCandidate* lkCreateIceCandidate(const char* mid,
                                               int mlineIndex,
                                               const char* sdp);

LK_EXPORT int lkIceCandidateGetMlineIndex(lkIceCandidate* candidate);

LK_EXPORT lkString* lkIceCandidateGetMid(lkIceCandidate* candidate);

LK_EXPORT lkString* lkIceCandidateGetSdp(lkIceCandidate* candidate);

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
    lkAudioTrackSource* source,
    const lkAudioSourceOptions* options);

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

LK_EXPORT lkString* lkMediaStreamTrackGetId(lkMediaStreamTrack* track);

LK_EXPORT bool lkMediaStreamTrackIsEnabled(lkMediaStreamTrack* track);

LK_EXPORT void lkMediaStreamTrackSetEnabled(lkMediaStreamTrack* track,
                                            bool enabled);

LK_EXPORT lkRtcTrackState lkMediaStreamTrackGetState(lkMediaStreamTrack* track);

LK_EXPORT lkMediaStreamTrackKind
lkMediaStreamTrackGetKind(lkMediaStreamTrack* track);

LK_EXPORT lkRtcAudioTrack* lkPeerFactoryCreateAudioTrack(
    lkPeerFactory* factory,
    const char* id,
    lkAudioTrackSource* source);

LK_EXPORT lkRtcVideoTrack* lkPeerFactoryCreateVideoTrack(
    lkPeerFactory* factory,
    const char* id,
    lkVideoTrackSource* source);

LK_EXPORT void lkAudioTrackAddSink(lkRtcAudioTrack* track,
                                   lkNativeAudioSink* sink);

LK_EXPORT void lkAudioTrackRemoveSink(lkRtcVideoTrack* track,
                                      lkNativeAudioSink* sink);

LK_EXPORT lkString* lkMediaStreamGetId(lkMediaStream* stream);

LK_EXPORT lkVectorGeneric* lkMediaStreamGetAudioTracks(lkMediaStream* stream);

LK_EXPORT lkVectorGeneric* lkMediaStreamGetVideoTracks(lkMediaStream* stream);

LK_EXPORT lkNativeVideoSink* lkCreateNativeVideoSink(
    const lkVideoSinkCallabacks* callbacks,
    void* userdata);

LK_EXPORT void lkVideoTrackAddSink(lkRtcVideoTrack* source,
                                   lkNativeVideoSink* sink);

LK_EXPORT void lkVideoTrackRemoveSink(lkRtcVideoTrack* source,
                                      lkNativeVideoSink* sink);

LK_EXPORT lkVideoTrackSource* lkCreateVideoTrackSource(
    lkVideoResolution resolution);

LK_EXPORT lkVideoResolution
lkVideoTrackSourceGetResolution(lkVideoTrackSource* source);

LK_EXPORT void lkVideoTrackSourceOnCaptureFrame(lkVideoTrackSource* source,
                                                lkVideoFrame* frame);

LK_EXPORT lkVideoBufferType
lkVideoFrameBufferGetType(lkVideoFrameBuffer* frameBuffer);

LK_EXPORT uint32_t lkVideoFrameBufferGetWidth(lkVideoFrameBuffer* frameBuffer);

LK_EXPORT uint32_t lkVideoFrameBufferGetHeight(lkVideoFrameBuffer* frameBuffer);

LK_EXPORT lkI420Buffer* lkI420BufferNew(uint32_t width,
                                        uint32_t height,
                                        uint32_t stride_y,
                                        uint32_t stride_u,
                                        uint32_t stride_v);

LK_EXPORT lkI420Buffer* lkVideoFrameBufferToI420(
    lkVideoFrameBuffer* frameBuffer);

LK_EXPORT lkI420Buffer* lkVideoFrameBufferGetI420(
    lkVideoFrameBuffer* frameBuffer);

LK_EXPORT lkI420ABuffer* lkVideoFrameBufferGetI420A(
    lkVideoFrameBuffer* frameBuffer);

LK_EXPORT lkI422Buffer* lkVideoFrameBufferGetI422(
    lkVideoFrameBuffer* frameBuffer);

LK_EXPORT lkI444Buffer* lkVideoFrameBufferGetI444(
    lkVideoFrameBuffer* frameBuffer);

LK_EXPORT lkI010Buffer* lkVideoFrameBufferGetI010(
    lkVideoFrameBuffer* frameBuffer);

LK_EXPORT lkNV12Buffer* lkVideoFrameBufferGetNV12(
    lkVideoFrameBuffer* frameBuffer);

LK_EXPORT uint32_t lkI420BufferGetChromaWidth(lkI420Buffer* buffer);

LK_EXPORT uint32_t lkI420BufferGetChromaHeight(lkI420Buffer* buffer);

LK_EXPORT uint32_t lkI420BufferGetStrideY(lkI420Buffer* buffer);

LK_EXPORT uint32_t lkI420BufferGetStrideU(lkI420Buffer* buffer);

LK_EXPORT uint32_t lkI420BufferGetStrideV(lkI420Buffer* buffer);

LK_EXPORT const uint8_t* lkI420BufferGetDataY(lkI420Buffer* buffer);

LK_EXPORT const uint8_t* lkI420BufferGetDataU(lkI420Buffer* buffer);

LK_EXPORT const uint8_t* lkI420BufferGetDataV(lkI420Buffer* buffer);

LK_EXPORT lkI420Buffer* lkI420BufferScale(lkI420Buffer* buffer,
                                          int scaledWidth,
                                          int scaledHeight);

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

LK_EXPORT lkI422Buffer* lkI422BufferNew(uint32_t width,
                                        uint32_t height,
                                        uint32_t stride_y,
                                        uint32_t stride_u,
                                        uint32_t stride_v);

LK_EXPORT uint32_t lkI422BufferGetChromaWidth(lkI422Buffer* buffer);

LK_EXPORT uint32_t lkI422BufferGetChromaHeight(lkI422Buffer* buffer);

LK_EXPORT uint32_t lkI422BufferGetStrideY(lkI422Buffer* buffer);

LK_EXPORT uint32_t lkI422BufferGetStrideU(lkI422Buffer* buffer);

LK_EXPORT uint32_t lkI422BufferGetStrideV(lkI422Buffer* buffer);

LK_EXPORT const uint8_t* lkI422BufferGetDataY(lkI422Buffer* buffer);

LK_EXPORT const uint8_t* lkI422BufferGetDataU(lkI422Buffer* buffer);

LK_EXPORT const uint8_t* lkI422BufferGetDataV(lkI422Buffer* buffer);

LK_EXPORT lkI422Buffer* lkI422BufferScale(lkI422Buffer* buffer,
                                          int scaledWidth,
                                          int scaledHeight);

LK_EXPORT lkI444Buffer* lkI444BufferNew(uint32_t width,
                                        uint32_t height,
                                        uint32_t stride_y,
                                        uint32_t stride_u,
                                        uint32_t stride_v);
LK_EXPORT uint32_t lkI444BufferGetChromaWidth(lkI444Buffer* buffer);

LK_EXPORT uint32_t lkI444BufferGetChromaHeight(lkI444Buffer* buffer);

LK_EXPORT uint32_t lkI444BufferGetStrideY(lkI444Buffer* buffer);

LK_EXPORT uint32_t lkI444BufferGetStrideU(lkI444Buffer* buffer);

LK_EXPORT uint32_t lkI444BufferGetStrideV(lkI444Buffer* buffer);

LK_EXPORT const uint8_t* lkI444BufferGetDataY(lkI444Buffer* buffer);

LK_EXPORT const uint8_t* lkI444BufferGetDataU(lkI444Buffer* buffer);

LK_EXPORT const uint8_t* lkI444BufferGetDataV(lkI444Buffer* buffer);

LK_EXPORT lkI444Buffer* lkI444BufferScale(lkI444Buffer* buffer,
                                          int scaledWidth,
                                          int scaledHeight);

LK_EXPORT lkI010Buffer* lkI010BufferNew(uint32_t width,
                                        uint32_t height,
                                        uint32_t stride_y,
                                        uint32_t stride_u,
                                        uint32_t stride_v);
LK_EXPORT uint32_t lkI010BufferGetChromaWidth(lkI010Buffer* buffer);

LK_EXPORT uint32_t lkI010BufferGetChromaHeight(lkI010Buffer* buffer);

LK_EXPORT uint32_t lkI010BufferGetStrideY(lkI010Buffer* buffer);

LK_EXPORT uint32_t lkI010BufferGetStrideU(lkI010Buffer* buffer);

LK_EXPORT uint32_t lkI010BufferGetStrideV(lkI010Buffer* buffer);

LK_EXPORT const uint16_t* lkI010BufferGetDataY(lkI010Buffer* buffer);

LK_EXPORT const uint16_t* lkI010BufferGetDataU(lkI010Buffer* buffer);

LK_EXPORT const uint16_t* lkI010BufferGetDataV(lkI010Buffer* buffer);

LK_EXPORT lkI010Buffer* lkI010BufferScale(lkI010Buffer* buffer,
                                          int scaledWidth,
                                          int scaledHeight);

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

LK_EXPORT lkNV12Buffer* lkNV12BufferScale(lkNV12Buffer* buffer,
                                          int scaledWidth,
                                          int scaledHeight);

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

LK_EXPORT void lkVideoFrameBuilderSetVideoFrameBuffer(
    lkVideoFrameBuilder* builder,
    lkVideoFrameBuffer* buffer);

LK_EXPORT void lkVideoFrameBuilderSetTimestampUs(lkVideoFrameBuilder* builder,
                                                 int64_t timestampNs);

LK_EXPORT void lkVideoFrameBuilderSetRotation(lkVideoFrameBuilder* builder,
                                              lkVideoRotation rotation);

LK_EXPORT void lkVideoFrameBuilderSetId(lkVideoFrameBuilder* builder,
                                        uint16_t id);

LK_EXPORT lkVideoFrame* lkVideoFrameBuilderBuild(lkVideoFrameBuilder* builder);

LK_EXPORT lkVideoRotation lkVideoFrameGetRotation(const lkVideoFrame* frame);

LK_EXPORT int64_t lkVideoFrameGetTimestampUs(const lkVideoFrame* frame);

LK_EXPORT uint16_t lkVideoFrameGetId(const lkVideoFrame* frame);

LK_EXPORT lkVideoFrameBuffer* lkVideoFrameGetBuffer(const lkVideoFrame* frame);

LK_EXPORT lkMediaStreamTrack* lkRtpSenderGetTrack(lkRtpSender* sender);

LK_EXPORT bool lkRtpSenderSetTrack(lkRtpSender* sender,
                                   lkMediaStreamTrack* track);

LK_EXPORT lkString* lkRtpTransceiverGetMid(lkRtpTransceiver* transceiver);

LK_EXPORT lkRtpTransceiverDirection
lkRtpTransceiverGetDirection(lkRtpTransceiver* transceiver);

LK_EXPORT lkRtpTransceiverDirection
lkRtpTransceiverCurrentDirection(lkRtpTransceiver* transceiver);

LK_EXPORT lkRtpSender* lkRtpTransceiverGetSender(lkRtpTransceiver* transceiver);

LK_EXPORT lkRtpReceiver* lkRtpTransceiverGetReceiver(
    lkRtpTransceiver* transceiver);

LK_EXPORT void lkRtpTransceiverStop(lkRtpTransceiver* transceiver);

LK_EXPORT lkMediaStreamTrack* lkRtpReceiverGetTrack(lkRtpReceiver* receiver);

LK_EXPORT void lkPeerGetStats(lkPeer* peer,
                              void (*onComplete)(const char* statsJson,
                                                 void* userdata),
                              void* userdata);

LK_EXPORT void lkRtpSenderGetStats(lkRtpSender* sender,
                                   void (*onComplete)(const char* statsJson,
                                                      void* userdata),
                                   void* userdata);

LK_EXPORT void lkRtpReceiverGetStats(lkRtpReceiver* receiver,
                                     void (*onComplete)(const char* statsJson,
                                                        void* userdata),
                                     void* userdata);

LK_EXPORT lkRtpCodecCapability* lkRtpCodecCapabilityCreate();

LK_EXPORT void lkRtpCodecCapabilitySetMimeType(lkRtpCodecCapability* codec,
                                               const char* mimeType);

LK_EXPORT void lkRtpCodecCapabilitySetClockRate(lkRtpCodecCapability* codec,
                                                uint32_t clockRate);

LK_EXPORT void lkRtpCodecCapabilitySetChannels(lkRtpCodecCapability* codec,
                                               uint16_t channels);

LK_EXPORT uint16_t lkRtpCodecCapabilityGetChannels(lkRtpCodecCapability* codec);

LK_EXPORT int lkRtpCodecCapabilityGetPreferredPayloadType(
    lkRtpCodecCapability* codec);

LK_EXPORT bool lkRtpCodecCapabilityHasPreferredPayloadType(
    lkRtpCodecCapability* codec);

LK_EXPORT void lkRtpCodecCapabilitySetPreferredPayloadType(
    lkRtpCodecCapability* codec,
    int payloadType);

LK_EXPORT uint32_t
lkRtpCodecCapabilityGetClockRate(lkRtpCodecCapability* codec);

LK_EXPORT lkString* lkRtpCodecCapabilityGetMimeType(
    lkRtpCodecCapability* codec);

LK_EXPORT bool lkRtpCodecCapabilityHasSdpFmtpLine(lkRtpCodecCapability* codec);

LK_EXPORT void lkRtpCodecCapabilitySetSdpFmtpLine(lkRtpCodecCapability* codec,
                                                  const char* sdpFmtpLine);

LK_EXPORT lkString* lkRtpCodecCapabilityGetSdpFmtpLine(
    lkRtpCodecCapability* codec);

LK_EXPORT lkRtcpFeedback* lkRtcpFeedbackCreate(
    lkRtcpFeedbackType type,
    bool hasMessageType,
    lkRtcpFeedbackMessageType messageType);

LK_EXPORT lkRtcpFeedbackType lkRtcpFeedbackGetType(lkRtcpFeedback* feedback);

LK_EXPORT bool lkRtcpFeedbackHasMessageType(lkRtcpFeedback* feedback);

LK_EXPORT lkRtcpFeedbackMessageType
lkRtcpFeedbackGetMessageType(lkRtcpFeedback* feedback);

LK_EXPORT lkVectorGeneric* lkRtpCodecCapabilityGetRtcpFeedbacks(
    lkRtpCodecCapability* codec);

LK_EXPORT void lkRtpCodecCapabilitySetRtcpFeedbacks(
    lkRtpCodecCapability* codec,
    lkVectorGeneric* rtcpFeedbacks);

LK_EXPORT lkString* lkRtpHeaderExtensionCapabilityGetUri(
    lkRtpHeaderExtensionCapability* ext);

LK_EXPORT lkRtpTransceiverDirection
lkRtpHeaderExtensionCapabilityGetDirection(lkRtpHeaderExtensionCapability* ext);

LK_EXPORT lkString* lkRtcpParametersGetCname(lkRtcpParameters* rtcp);

LK_EXPORT bool lkRtcpParametersGetReducedSize(lkRtcpParameters* rtcp);

LK_EXPORT uint8_t
lkRtpCodecParametersGetPayloadType(lkRtpCodecParameters* codec);

LK_EXPORT lkString* lkRtpCodecParametersGetMimeType(
    lkRtpCodecParameters* codec);

LK_EXPORT uint32_t
lkRtpCodecParametersGetClockRate(lkRtpCodecParameters* codec);

LK_EXPORT uint16_t lkRtpCodecParametersGetChannels(lkRtpCodecParameters* codec);

LK_EXPORT lkString* lkRtpHeaderExtensionParametersGetUri(
    lkRtpHeaderExtensionParameters* ext);

LK_EXPORT uint8_t
lkRtpHeaderExtensionParametersGetId(lkRtpHeaderExtensionParameters* ext);

LK_EXPORT bool lkRtpHeaderExtensionParametersGetEncrypted(
    lkRtpHeaderExtensionParameters* ext);

LK_EXPORT lkVectorGeneric* lkRtpParametersGetCodecs(lkRtpParameters* params);

LK_EXPORT lkRtcpParameters* lkRtpParametersGetRtcp(lkRtpParameters* params);

LK_EXPORT lkVectorGeneric* lkRtpParametersGetHeaderExtensions(
    lkRtpParameters* params);

LK_EXPORT lkRtpParameters* lkRtpSenderGetParameters(lkRtpSender* sender);

LK_EXPORT bool lkRtpSenderSetParameters(lkRtpSender* sender,
                                        lkRtpParameters* params,
                                        lkRtcError* error);

LK_EXPORT lkRtpParameters* lkRtpReceiverGetParameters(lkRtpReceiver* receiver);

LK_EXPORT lkRtpTransceiverInit* lkRtpTransceiverInitCreate();

LK_EXPORT void lkRtpTransceiverInitSetDirection(
    lkRtpTransceiverInit* init,
    lkRtpTransceiverDirection direction);

LK_EXPORT void lkRtpTransceiverInitSetStreamIds(lkRtpTransceiverInit* init,
                                                lkVectorGeneric* streamIds);

LK_EXPORT lkRtpTransceiverDirection
lkRtpTransceiverInitGetDirection(lkRtpTransceiverInit* init);

LK_EXPORT void lkRtpTransceiverInitSetSendEncodingsdings(
    lkRtpTransceiverInit* init,
    lkVectorGeneric* encodings);

LK_EXPORT bool lkRtpTransceiverSetCodecPreferences(
    lkRtpTransceiver* transceiver,
    lkVectorGeneric* codecs,
    lkRtcError* error);

LK_EXPORT bool lkRtpTransceiverStopWithError(lkRtpTransceiver* transceiver,
                                             lkRtcError* error);

LK_EXPORT lkRtpEncodingParameters* lkRtpEncodingParametersCreate();

LK_EXPORT void lkRtpEncodingParametersSetActive(
    lkRtpEncodingParameters* encoding,
    bool active);

LK_EXPORT void lkRtpEncodingParametersSetMaxBitrateBps(
    lkRtpEncodingParameters* encoding,
    int64_t maxBitrateBps);

LK_EXPORT void lkRtpEncodingParametersSetMinBitrateBps(
    lkRtpEncodingParameters* encoding,
    int64_t minBitrateBps);

LK_EXPORT void lkRtpEncodingParametersSetBitratePriority(
    lkRtpEncodingParameters* encoding,
    double bitratePriority);

LK_EXPORT void lkRtpEncodingParametersSetNetworkPriority(
    lkRtpEncodingParameters* encoding,
    lkNetworkPriority networkPriority);

LK_EXPORT void lkRtpEncodingParametersSetMaxFramerate(
    lkRtpEncodingParameters* encoding,
    double maxFramerate);

LK_EXPORT void lkRtpEncodingParametersSetScaleResolutionDownBy(
    lkRtpEncodingParameters* encoding,
    double scaleResolutionDownBy);

LK_EXPORT void lkRtpEncodingParametersSetRid(lkRtpEncodingParameters* encoding,
                                             const char* rid);

LK_EXPORT void lkRtpEncodingParametersSetScalabilityMode(
    lkRtpEncodingParameters* encoding,
    const char* scalabilityMode);

LK_EXPORT lkRtpParameters* lkRtpParametersCreate();

LK_EXPORT lkRtcpParameters* lkRtcpParametersCreate();

LK_EXPORT void lkRtpParametersSetCodecs(lkRtpParameters* params,
                                        lkVectorGeneric* codecs);

LK_EXPORT void lkRtpParametersSetRtcp(lkRtpParameters* params,
                                      lkRtcpParameters* rtcp);

LK_EXPORT void lkRtcpParametersSetReducedSize(lkRtcpParameters* rtcp,
                                              bool reducedSize);

LK_EXPORT void lkRtcpParametersSetCname(lkRtcpParameters* rtcp,
                                        const char* cname);

LK_EXPORT void lkRtpParametersSetHeaderExtensions(
    lkRtpParameters* params,
    lkVectorGeneric* headerExtensions);

LK_EXPORT lkRtpCodecParameters* lkRtpCodecParametersCreate();

LK_EXPORT void lkRtpCodecParametersSetPayloadType(lkRtpCodecParameters* codec,
                                                  uint32_t payloadType);

LK_EXPORT void lkRtpCodecParametersSetMimeType(lkRtpCodecParameters* codec,
                                               const char* mimeType);

LK_EXPORT void lkRtpCodecParametersSetClockRate(lkRtpCodecParameters* codec,
                                                uint32_t clockRate);

LK_EXPORT void lkRtpCodecParametersSetChannels(lkRtpCodecParameters* codec,
                                               uint32_t channels);

LK_EXPORT lkRtpHeaderExtensionParameters*
lkRtpHeaderExtensionParametersCreate();

LK_EXPORT void lkRtpHeaderExtensionParametersSetUri(
    lkRtpHeaderExtensionParameters* ext,
    const char* uri);

LK_EXPORT void lkRtpHeaderExtensionParametersSetId(
    lkRtpHeaderExtensionParameters* ext,
    uint32_t id);

LK_EXPORT void lkRtpHeaderExtensionParametersSetEncrypted(
    lkRtpHeaderExtensionParameters* ext,
    bool encrypted);

LK_EXPORT lkKeyProviderOptions* lkKeyProviderOptionsCreate();

LK_EXPORT void lkKeyProviderOptionsSetSharedKey(lkKeyProviderOptions* options,
                                                bool sharedKey);

LK_EXPORT void lkKeyProviderOptionsSetRatchetWindowSize(
    lkKeyProviderOptions* options,
    int32_t windowSize);

LK_EXPORT void lkKeyProviderOptionsSetRatchetSalt(lkKeyProviderOptions* options,
                                                  const uint8_t* salt,
                                                  uint32_t length);

LK_EXPORT void lkKeyProviderOptionsSetFailureTolerance(
    lkKeyProviderOptions* options,
    int32_t tolerance);

LK_EXPORT lkKeyProvider* lkKeyProviderCreate(lkKeyProviderOptions* options);

LK_EXPORT bool lkKeyProviderSetSharedKey(lkKeyProvider* provider,
                                         int keyIndex,
                                         const uint8_t* key,
                                         uint32_t length);

LK_EXPORT lkData* lkKeyProviderRatchetSharedKey(lkKeyProvider* provider,
                                                int keyIndex);

LK_EXPORT void lkKeyProviderSetSifTrailer(lkKeyProvider* provider,
                                          const uint8_t* sif,
                                          uint32_t length);

LK_EXPORT lkData* lkKeyProviderGetSharedKey(lkKeyProvider* provider,
                                            int keyIndex);

LK_EXPORT bool lkKeyProviderSetKey(lkKeyProvider* provider,
                                   const char* participantId,
                                   int keyIndex,
                                   const uint8_t* key,
                                   uint32_t length);

LK_EXPORT lkData* lkKeyProviderRatchetKey(lkKeyProvider* provider,
                                          const char* participantId,
                                          int keyIndex);

LK_EXPORT lkData* lkKeyProviderGetKey(lkKeyProvider* provider,
                                      const char* participantId,
                                      int keyIndex);

LK_EXPORT lkFrameCryptor* lkNewFrameCryptorForRtpSender(
    lkPeerFactory* factory,
    const char* participantId,
    lkEncryptionAlgorithm algorithm,
    lkKeyProvider* provider,
    lkRtpSender* sender,
    void (*onStateChanged)(const char* participantId,
                           lkEncryptionState state,
                           void* userdata),
    void* userdata);

LK_EXPORT lkFrameCryptor* lkNewFrameCryptorForRtpReceiver(
    lkPeerFactory* factory,
    const char* participantId,
    lkEncryptionAlgorithm algorithm,
    lkKeyProvider* provider,
    lkRtpReceiver* receiver,
    void (*onStateChanged)(const char* participantId,
                           lkEncryptionState state,
                           void* userdata),
    void* userdata);

LK_EXPORT void lkFrameCryptorSetEnabled(lkFrameCryptor* fc, bool enabled);

LK_EXPORT bool lkFrameCryptorGetEnabled(lkFrameCryptor* fc);

LK_EXPORT void lkFrameCryptorSetKeyIndex(lkFrameCryptor* fc, int keyIndex);

LK_EXPORT int lkFrameCryptorGetKeyIndex(lkFrameCryptor* fc);

LK_EXPORT lkString* lkFrameCryptorGetParticipantId(lkFrameCryptor* fc);

LK_EXPORT lkDataPacketCryptor* lkNewDataPacketCryptor(
    lkEncryptionAlgorithm algorithm,
    lkKeyProvider* provider);

LK_EXPORT lkEncryptedPacket* lkNewlkEncryptedPacket(const uint8_t* data,
                                                    uint32_t size,
                                                    const uint8_t* iv,
                                                    uint32_t iv_size,
                                                    uint32_t keyIndex);

LK_EXPORT lkData* lkEncryptedPacketGetData(lkEncryptedPacket* packet);

LK_EXPORT lkData* lkEncryptedPacketGetIv(lkEncryptedPacket* packet);

LK_EXPORT uint32_t lkEncryptedPacketGetKeyIndex(lkEncryptedPacket* packet);

LK_EXPORT lkEncryptedPacket* lkDataPacketCryptorEncrypt(
    lkDataPacketCryptor* dc,
    const char* participantId,
    uint32_t keyIndex,
    const char* data,
    uint32_t data_size,
    lkRtcError* errorOut);

LK_EXPORT lkData* lkDataPacketCryptorDecrypt(lkDataPacketCryptor* dc,
                                             const char* participantId,
                                             lkEncryptedPacket* encryptedPacket,
                                             lkRtcError* errorOut);

LK_EXPORT lkAudioMixer* lkCreateAudioMixer();

LK_EXPORT void lkAudioMixerAddSource(lkAudioMixer* mixer,
                                     const lkAudioMixerSourceCallback* source,
                                     void* userdata);

LK_EXPORT void lkAudioMixerRemoveSource(lkAudioMixer* mixer, int32_t ssrc);

LK_EXPORT uint32_t lkAudioMixerMixFrame(lkAudioMixer* mixer,
                                        uint32_t number_of_channels);

LK_EXPORT const int16_t* lkAudioMixerGetData(lkAudioMixer* mixer);

LK_EXPORT void lkNativeAudioFrameUpdateFrame(
    lkNativeAudioFrame* nativeFrame,
    uint32_t timestamp,
    const int16_t* data,
    uint32_t samplesPreChannel,
    int sampleRateHz,
    uint32_t numChannel);

LK_EXPORT lkAudioResampler* lkAudioResamplerCreate();

LK_EXPORT uint32_t lkAudioResamplerResample(lkAudioResampler* resampler,
                                            const int16_t* input,
                                            uint32_t samples_per_channel,
                                            uint32_t num_channels,
                                            uint32_t sample_rate,
                                            uint32_t dst_num_channels,
                                            uint32_t dst_sample_rate);

LK_EXPORT const int16_t* lkAudioResamplerGetData(lkAudioResampler* resampler);

LK_EXPORT lkAudioProcessingModule* lkAudioProcessingModuleCreate(
    bool echo_canceller_enabled,
    bool gain_controller_enabled,
    bool high_pass_filter_enabled,
    bool noise_suppression_enabled);

LK_EXPORT int32_t
lkAudioProcessingModuleProcessStream(lkAudioProcessingModule* apm,
                                     const int16_t* src,
                                     uint32_t src_len,
                                     int16_t* dst,
                                     uint32_t dst_len,
                                     int32_t sample_rate,
                                     int32_t num_channels);

LK_EXPORT int32_t
lkAudioProcessingModuleProcessReverseStream(lkAudioProcessingModule* apm,
                                            const int16_t* src,
                                            uint32_t src_len,
                                            int16_t* dst,
                                            uint32_t dst_len,
                                            int32_t sample_rate,
                                            int32_t num_channels);

LK_EXPORT int32_t
lkAudioProcessingModuleSetStreamDelayMs(lkAudioProcessingModule* apm,
                                        int32_t delay);

LK_EXPORT lkDesktopCapturer* lkCreateDesktopCapturer(
    const lkDesktopCapturerOptions* options);

LK_EXPORT uint64_t lkDesktopSourceGetId(lkDesktopSource* source);

LK_EXPORT lkString* lkDesktopSourceGetTitle(lkDesktopSource* source);

LK_EXPORT int64_t lkDesktopSourceGetDisplayId(lkDesktopSource* source);

LK_EXPORT bool lkDesktopCapturerSelectSource(lkDesktopCapturer* capturer,
                                             uint64_t id);

LK_EXPORT lkVectorGeneric* lkDesktopCapturerGetSourceList(
    lkDesktopCapturer* capturer);

LK_EXPORT void lkDesktopCapturerStart(lkDesktopCapturer* capturer,
                                      void (*callback)(lkDesktopFrame* frame,
                                                       lkCaptureResult result,
                                                       void* userdata),
                                      void* userdata);

LK_EXPORT void lkDesktopCapturerCaptureFrame(lkDesktopCapturer* capturer);

LK_EXPORT int32_t lkDesktopFrameGetWidth(lkDesktopFrame* frame);

LK_EXPORT int32_t lkDesktopFrameGetHeight(lkDesktopFrame* frame);

LK_EXPORT uint32_t lkDesktopFrameGetStride(lkDesktopFrame* frame);

LK_EXPORT int32_t lkDesktopFrameGetLeft(lkDesktopFrame* frame);

LK_EXPORT int32_t lkDesktopFrameGetTop(lkDesktopFrame* frame);

LK_EXPORT const uint8_t* lkDesktopFrameGetData(lkDesktopFrame* frame);

#ifdef __cplusplus
}
#endif

#endif  // LIVEKIT_CAPI_H
