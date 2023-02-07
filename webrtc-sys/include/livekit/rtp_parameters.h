#ifndef CLIENT_SDK_NATIVE_RTP_PARAMETERS_H
#define CLIENT_SDK_NATIVE_RTP_PARAMETERS_H

#include <memory>

#include "api/media_types.h"
#include "api/priority.h"
#include "api/rtp_parameters.h"
#include "api/rtp_transceiver_direction.h"
#include "webrtc-sys/src/rtp_parameters.rs.h"

namespace livekit {

static webrtc::RtcpFeedback to_native_rtcp_feedback(RtcpFeedback feedback) {
  webrtc::RtcpFeedback native;
  native.type = static_cast<webrtc::RtcpFeedbackType>(feedback.feedback_type);
  if (feedback.has_message_type)
    native.message_type =
        static_cast<webrtc::RtcpFeedbackMessageType>(feedback.message_type);

  return native;
}

static webrtc::RtpCodecCapability to_native_rtp_codec_capability(
    RtpCodecCapability capability) {
  webrtc::RtpCodecCapability native;
  // native.mime_type(); IGNORED

  native.name = capability.name.c_str();
  native.kind = static_cast<cricket::MediaType>(capability.kind);

  if (capability.has_clock_rate)
    native.clock_rate = native.clock_rate;

  if (capability.has_preferred_payload_type)
    native.preferred_payload_type = capability.preferred_payload_type;

  if (capability.has_max_ptime)
    native.max_ptime = capability.max_ptime;

  if (capability.has_ptime)
    native.ptime = capability.ptime;

  if (capability.has_num_channels)
    native.num_channels = capability.num_channels;

  for (auto feedback : capability.rtcp_feedback)
    native.rtcp_feedback.push_back(to_native_rtcp_feedback(feedback));

  for (auto pair : capability.parameters)
    native.parameters.insert(pair.key, pair.value);

  for (auto pair : capability.options)
    native.options.insert(pair.key, pair.value);

  native.max_temporal_layer_extensions =
      capability.max_temporal_layer_extensions;

  native.max_spatial_layer_extensions = capability.max_spatial_layer_extensions;

  native.svc_multi_stream_support = capability.svc_multi_stream_support;

  return native;
}

static webrtc::RtpHeaderExtensionCapability
to_native_rtp_header_extension_capability(RtpHeaderExtensionCapability header) {
  webrtc::RtpHeaderExtensionCapability native;
  native.uri = header.uri.c_str();

  if (header.has_preferred_id)
    native.preferred_id = header.preferred_id;

  native.preferred_encrypt = header.preferred_encrypt;
  native.direction =
      static_cast<webrtc::RtpTransceiverDirection>(header.direction);

  return native;
}

static webrtc::RtpExtension to_rtp_extension(RtpExtension ext) {
  webrtc::RtpExtension native;
  native.uri = ext.uri.c_str();
  native.id = ext.id;
  native.encrypt = ext.encrypt;
  return native;
}

static webrtc::RtpFecParameters to_rtp_fec_parameters(RtpFecParameters fec) {
  webrtc::RtpFecParameters native;

  if (fec.has_ssrc)
    native.ssrc = fec.ssrc;

  native.mechanism = static_cast<webrtc::FecMechanism>(fec.mechanism);
  return native;
}

struct webrtc::RtpRtxParameters to_rtp_rtx_parameters(RtpRtxParameters rtx) {
  webrtc::RtpRtxParameters native;

  if (rtx.has_ssrc)
    native.ssrc = rtx.ssrc;
  return native;
}

static webrtc::RtpEncodingParameters to_native_rtp_encoding_paramters(
    RtpEncodingParameters parameters) {
  webrtc::RtpEncodingParameters native;
  native.rid = parameters.rid.c_str();

  if (parameters.has_ssrc)
    native.ssrc = parameters.ssrc;

  native.active = parameters.active;
  if (parameters.has_max_framerate)
    native.max_framerate = parameters.max_framerate;

  native.adaptive_ptime = parameters.adaptive_ptime;
  if (parameters.has_max_bitrate_bps)
    native.max_bitrate_bps = parameters.max_bitrate_bps;

  if (parameters.has_min_bitrate_bps)
    native.min_bitrate_bps = parameters.min_bitrate_bps;

  native.bitrate_priority = parameters.bitrate_priority;
  native.network_priority =
      static_cast<webrtc::Priority>(parameters.network_priority);

  if (parameters.has_scalability_mode)
    native.scalability_mode = parameters.scalability_mode.c_str();

  if (parameters.has_num_temporal_layers)
    native.num_temporal_layers = parameters.num_temporal_layers;

  if (parameters.has_scale_resolution_down_by)
    native.scale_resolution_down_by = parameters.scale_resolution_down_by;
  return native;
}

static webrtc::RtpCodecParameters to_native_rtp_codec_parameters(
    RtpCodecParameters params) {
  webrtc::RtpCodecParameters native;
  native.name = params.name.c_str();
  native.kind = static_cast<cricket::MediaType>(params.kind);
  native.payload_type = params.payload_type;

  for (auto pair : params.parameters)
    native.parameters.insert(pair.key, pair.value);

  for (auto feedback : params.rtcp_feedback)
    native.rtcp_feedback.push_back(to_native_rtcp_feedback(feedback));

  if (params.has_num_channels)
    native.num_channels = params.num_channels;

  if (params.has_ptime)
    native.ptime = params.ptime;

  if (params.has_max_ptime)
    native.max_ptime = params.max_ptime;

  if (params.has_clock_rate)
    native.clock_rate = params.clock_rate;

  return native;
}

static webrtc::RtpCapabilities to_rtp_capabilities(
    RtpCapabilities capabilities) {
  webrtc::RtpCapabilities native;
  for (auto codec : capabilities.codecs)
    native.codecs.push_back(to_native_rtp_codec_capability(codec));

  for (auto header : capabilities.header_extensions)
    native.header_extensions.push_back(
        to_native_rtp_header_extension_capability(header));

  for (auto fec : capabilities.fec)
    native.fec.push_back(static_cast<webrtc::FecMechanism>(fec));

  return native;
}

static webrtc::RtcpParameters to_native_rtcp_paramaters(RtcpParameters params) {
  webrtc::RtcpParameters native;
  if (params.has_ssrc)
    native.ssrc = params.ssrc;

  native.mux = params.mux;
  native.cname = params.cname.c_str();
  native.reduced_size = params.reduced_size;
  return native;
}

}  // namespace livekit
#endif  // CLIENT_SDK_NATIVE_RTP_PARAMETERS_H
