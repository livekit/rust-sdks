/*
 * Copyright 2025 LiveKit, Inc.
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

#pragma once

#include <string>
#include <vector>

#include "api/rtp_parameters.h"
#include "livekit_rtc/utils.h"

namespace livekit {

typedef struct {
  uint8_t feedback_type;
  bool has_message_type;
  uint8_t message_type;
} RtcpFeedback;

typedef struct {
  std::string key;
  std::string value;
} StringKeyValue;

typedef struct {
  std::string name;
  std::string mime_type;
  uint8_t kind;
  bool has_clock_rate;
  uint32_t clock_rate;
  bool has_preferred_payload_type;
  uint8_t preferred_payload_type;
  bool has_num_channels;
  uint16_t num_channels;
  std::vector<RtcpFeedback> rtcp_feedback;
  int rtcp_feedback_count;
  std::vector<StringKeyValue> parameters;
  int parameters_count;
} RtpCodecCapability;

typedef struct {
  std::string uri;
  bool has_preferred_id;
  uint8_t preferred_id;
  bool preferred_encrypt;
  uint8_t direction;
} RtpHeaderExtensionCapability;

typedef struct {
  std::string uri;
  uint8_t id;
  bool encrypt;
} RtpExtension;

typedef enum {
  FEC_MECHANISM_ULPFEC = 1,
  FEC_MECHANISM_RED = 2,
} FecMechanism;

typedef struct {
  bool has_ssrc;
  uint32_t ssrc;
  FecMechanism mechanism;
} RtpFecParameters;

typedef struct {
  bool has_ssrc;
  uint32_t ssrc;
} RtpRtxParameters;

typedef enum {
  kVeryLow,
  kLow,
  kMedium,
  kHigh,
} NetworkPriority;

typedef struct {
  bool has_ssrc;
  uint32_t ssrc;
  bool has_payload_type;
  uint8_t payload_type;
  bool has_max_bitrate_bps;
  uint32_t max_bitrate_bps;
  bool has_min_bitrate_bps;
  uint32_t min_bitrate_bps;
  bool has_max_framerate;
  double max_framerate;
  bool has_scale_resolution_down_by;
  double scale_resolution_down_by;
  bool has_num_temporal_layers;
  uint8_t num_temporal_layers;
  double bitrate_priority;
  NetworkPriority network_priority;
  std::string rid;
  bool active;
  bool adaptive_ptime;
  bool has_scalability_mode;
  std::string scalability_mode;
  RtpFecParameters fec;
  RtpRtxParameters rtx;
} RtpEncodingParameters;

typedef struct {
  lkRtpTransceiverDirection direction;
  std::vector<std::string> stream_ids;
  std::vector<RtpEncodingParameters> send_encodings;
} RtpTransceiverInit;

typedef enum {
  RTCP_FEEDBACK_MESSAGE_TYPE_NONE = 0,
  RTCP_FEEDBACK_MESSAGE_TYPE_ACK = 1,
  RTCP_FEEDBACK_MESSAGE_TYPE_NACK = 2,
  RTCP_FEEDBACK_MESSAGE_TYPE_CCM = 3,
} RtcpFeedbackMessageType;

typedef enum {
  RTCP_FEEDBACK_TYPE_UNDEFINED = 0,
  RTCP_FEEDBACK_TYPE_GOOG_ACK = 1,
  RTCP_FEEDBACK_TYPE_RTP_FB = 2,
  RTCP_FEEDBACK_TYPE_PS_FB = 3,
} RtcpFeedbackType;

typedef struct {
  std::string name;
  std::string mime_type;
  uint8_t kind;
  uint8_t payload_type;
  bool has_clock_rate;
  uint32_t clock_rate;
  bool has_num_channels;
  uint16_t num_channels;
  std::vector<RtcpFeedback> rtcp_feedback;
  std::vector<StringKeyValue> parameters;
} RtpCodecParameters;

typedef struct {
  std::vector<RtpCodecCapability> codecs;
  std::vector<RtpHeaderExtensionCapability> header_extensions;
  std::vector<FecMechanism> fec;
} RtpCapabilities;

typedef enum {
  DEGRADATION_PREFERENCE_MAINTAIN_FRAMERATE = 0,
  DEGRADATION_PREFERENCE_MAINTAIN_RESOLUTION = 1,
  DEGRADATION_PREFERENCE_BALANCED = 2,
} DegradationPreference;

typedef struct {
  bool has_ssrc;
  uint32_t ssrc;
  bool cname_is_set;
  std::string cname;
  bool mux;
  uint32_t reduced_size;
} RtcpParameters;

typedef struct {
  std::vector<RtpEncodingParameters> encodings;
  std::vector<RtpCodecParameters> codecs;
  std::vector<RtpExtension> header_extensions;
  bool has_degradation_preference;
  DegradationPreference degradation_preference;
  RtcpParameters rtcp;
  std::string transaction_id;
  std::string mid;
} RtpParameters;

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

RtcpFeedback to_capi_rtcp_feedback(webrtc::RtcpFeedback feedback);

RtpCodecCapability to_capi_rtp_codec_capability(
    webrtc::RtpCodecCapability capability);

RtpHeaderExtensionCapability to_capi_rtp_header_extension_capability(
    webrtc::RtpHeaderExtensionCapability header);

RtpExtension to_capi_rtp_extension(webrtc::RtpExtension ext);

RtpFecParameters to_capi_rtp_fec_parameters(webrtc::RtpFecParameters fec);

RtpRtxParameters to_capi_rtp_rtx_parameters(webrtc::RtpRtxParameters param);

RtpEncodingParameters to_capi_rtp_encoding_parameters(
    webrtc::RtpEncodingParameters params);

RtpCodecParameters to_capi_rtp_codec_parameters(
    webrtc::RtpCodecParameters params);

RtpCapabilities to_capi_rtp_capabilities(webrtc::RtpCapabilities capabilities);

RtcpParameters to_capi_rtcp_parameters(webrtc::RtcpParameters params);

RtpParameters to_capi_rtp_parameters(webrtc::RtpParameters params);

}  // namespace livekit
