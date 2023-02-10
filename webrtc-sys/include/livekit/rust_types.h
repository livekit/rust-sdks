//
// Created by Théo Monnom on 30/08/2022.
//

#ifndef RUST_TYPES_H
#define RUST_TYPES_H

namespace livekit {
struct RTCConfiguration;
struct PeerConnectionObserverWrapper;
struct CreateSdpObserverWrapper;
struct SetLocalSdpObserverWrapper;
struct SetRemoteSdpObserverWrapper;
struct DataChannelObserverWrapper;
struct AddIceCandidateObserverWrapper;
struct VideoFrameSinkWrapper;

// Shared types
enum class PeerConnectionState;
enum class SignalingState;
enum class IceConnectionState;
enum class IceGatheringState;
enum class SdpType;
enum class DataState;
enum class TrackState;
enum class ContentHint;
enum class VideoRotation;
enum class VideoFrameBufferType;
enum class MediaType;
enum class Priority;
enum class RtpTransceiverDirection;
enum class FecMechanism;
enum class RtcpFeedbackType;
enum class RtcpFeedbackMessageType;
enum class DegradationPreference;
enum class RtpExtensionFilter;
struct SdpParseError;
struct RTCOfferAnswerOptions;
struct RTCError;
struct DataChannelInit;
struct DataBuffer;
struct RtpTransceiverInit;
struct RtcpFeedback;
struct RtpCodecCapability;
struct RtpHeaderExtensionCapability;
struct RtpExtension;
struct RtpFecParameters;
struct RtpRtxParameters;
struct RtpEncodingParameters;
struct RtpCodecParameters;
struct RtpCapabilities;
struct RtcpParameters;
struct RtpParameters;
}  // namespace livekit

#endif  // RUST_TYPES_H
