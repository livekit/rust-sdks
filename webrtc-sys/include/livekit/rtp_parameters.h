#ifndef CLIENT_SDK_NATIVE_RTP_PARAMETERS_H
#define CLIENT_SDK_NATIVE_RTP_PARAMETERS_H

#include <memory>

#include "api/media_types.h"
#include "api/priority.h"
#include "api/rtp_parameters.h"
#include "api/rtp_transceiver_direction.h"
#include "livekit/rust_types.h"

namespace livekit {

struct KeyValue;

webrtc::RtcpFeedback to_native_rtcp_feedback(RtcpFeedback feedback);
webrtc::RtpCodecCapability to_native_rtp_codec_capability(
    RtpCodecCapability capability);
webrtc::RtpHeaderExtensionCapability to_native_rtp_header_extension_capability(
    RtpHeaderExtensionCapability header);
webrtc::RtpExtension to_native_rtp_extension(RtpExtension ext);
webrtc::RtpFecParameters to_rtp_fec_parameters(RtpFecParameters fec);
webrtc::RtpRtxParameters to_rtp_rtx_parameters(RtpRtxParameters rtx);
webrtc::RtpEncodingParameters to_native_rtp_encoding_paramters(
    RtpEncodingParameters parameters);
webrtc::RtpCodecParameters to_native_rtp_codec_parameters(
    RtpCodecParameters params);
webrtc::RtpCapabilities to_rtp_capabilities(RtpCapabilities capabilities);
webrtc::RtcpParameters to_native_rtcp_paramaters(RtcpParameters params);
webrtc::RtpParameters to_native_rtp_parameters(RtpParameters params);

RtcpFeedback to_rust_rtcp_feedback(webrtc::RtcpFeedback feedback);
RtpCodecCapability to_rust_rtp_codec_capability(
    webrtc::RtpCodecCapability capability);
RtpHeaderExtensionCapability to_rust_rtp_header_extension_capability(
    webrtc::RtpHeaderExtensionCapability header);
RtpExtension to_rust_rtp_extension(webrtc::RtpExtension ext);
RtpFecParameters to_rust_rtp_fec_parameters(webrtc::RtpFecParameters fec);
RtpRtxParameters to_rust_rtp_rtx_parameters(webrtc::RtpRtxParameters param);
RtpEncodingParameters to_rust_rtp_encoding_parameters(
    webrtc::RtpEncodingParameters params);
RtpCodecParameters to_rust_rtp_codec_parameters(
    webrtc::RtpCodecParameters params);
RtpCapabilities to_rust_rtp_capabilities(webrtc::RtpCapabilities capabilities);
RtcpParameters to_rust_rtcp_parameters(webrtc::RtcpParameters params);
RtpParameters to_rust_rtp_parameters(webrtc::RtpParameters params);

}  // namespace livekit
#endif  // CLIENT_SDK_NATIVE_RTP_PARAMETERS_H
