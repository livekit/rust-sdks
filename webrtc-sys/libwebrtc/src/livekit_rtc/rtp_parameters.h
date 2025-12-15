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
#include "rtc_base/logging.h"

namespace livekit {

class RtpCodecCapability : public webrtc::RefCountInterface {
 public:
  RtpCodecCapability() = default;
  virtual ~RtpCodecCapability() { RTC_LOG(LS_INFO) << "RtpCodecCapability destroyed"; }

  static webrtc::scoped_refptr<RtpCodecCapability> Create() {
    return webrtc::make_ref_counted<RtpCodecCapability>();
  }

  static webrtc::scoped_refptr<RtpCodecCapability> FromNative(
      const webrtc::RtpCodecCapability& native) {
    auto codec = webrtc::make_ref_counted<RtpCodecCapability>();
    codec->rtc_capability = native;
    return codec;
  }

  std::string mime_type() const { return rtc_capability.mime_type(); }

  void set_mime_type(const std::string& mime_type) {
    std::vector<std::string> parts = split(mime_type, "/");
    rtc_capability.name = parts[0];
    rtc_capability.kind = parts[1] == "audio" ? webrtc::MediaType::AUDIO : webrtc::MediaType::VIDEO;
  }

  uint32_t clock_rate() const { return rtc_capability.clock_rate.value_or(0); }

  void set_clock_rate(uint32_t clock_rate) { rtc_capability.clock_rate = clock_rate; }

  uint8_t num_channels() const { return rtc_capability.num_channels.value_or(1); }

  void set_num_channels(uint8_t num_channels) { rtc_capability.num_channels = num_channels; }

  lkMediaType kind() const { return static_cast<lkMediaType>(rtc_capability.kind); }

  std::string sdp_fmtp_line() const {
    std::vector<std::string> strarr;
    for (auto parameter : rtc_capability.parameters) {
      if (parameter.first == "") {
        strarr.push_back(parameter.second);
      } else {
        strarr.push_back(parameter.first + "=" + parameter.second);
      }
    }
    return join(strarr, ";");
  }

  void set_sdp_fmtp_line(const std::string& sdp_fmtp_line) {
    std::vector<std::string> parameters = split(sdp_fmtp_line, ";");
    for (auto parameter : parameters) {
      if (parameter.find("=") != std::string::npos) {
        std::vector<std::string> parameter_split = split(parameter, "=");
        rtc_capability.parameters[parameter_split[0]] = parameter_split[1];
      } else {
        rtc_capability.parameters[""] = parameter;
      }
    }
  }

  webrtc::RtpCodecCapability rtc_capability;
};

class RtpCodecParameters : public webrtc::RefCountInterface {
 public:
  RtpCodecParameters() = default;
  virtual ~RtpCodecParameters() = default;

  static webrtc::scoped_refptr<RtpCodecParameters> Create() {
    return webrtc::make_ref_counted<RtpCodecParameters>();
  }

  static webrtc::scoped_refptr<RtpCodecParameters> FromNative(
      const webrtc::RtpCodecParameters& native) {
    auto codec = webrtc::make_ref_counted<RtpCodecParameters>();
    codec->rtc_parameters = native;
    return codec;
  }

  uint8_t payload_type() const { return rtc_parameters.payload_type; }

  void set_payload_type(uint8_t payload_type) { rtc_parameters.payload_type = payload_type; }

  std::string mime_type() const { return rtc_parameters.mime_type(); }

  void set_mime_type(const std::string& mime_type) {
    std::vector<std::string> parts = split(mime_type, "/");
    rtc_parameters.name = parts[0];
    rtc_parameters.kind = parts[1] == "audio" ? webrtc::MediaType::AUDIO : webrtc::MediaType::VIDEO;
  }

  const char* name() const { return rtc_parameters.name.c_str(); }

  void set_name(const char* name) { rtc_parameters.name = std::string(name); }

  uint32_t clock_rate() const { return rtc_parameters.clock_rate.value_or(0); }

  bool has_clock_rate() const { return rtc_parameters.clock_rate.has_value(); }

  void set_clock_rate(uint32_t clock_rate) { rtc_parameters.clock_rate = clock_rate; }

  uint8_t num_channels() const { return rtc_parameters.num_channels.value_or(1); }

  bool has_num_channels() const { return rtc_parameters.num_channels.has_value(); }

  void set_num_channels(uint8_t num_channels) { rtc_parameters.num_channels = num_channels; }

  lkMediaType kind() const { return static_cast<lkMediaType>(rtc_parameters.kind); }

  webrtc::RtpCodecParameters rtc_parameters;
};

class RtcpParameters : public webrtc::RefCountInterface {
 public:
  RtcpParameters() = default;
  virtual ~RtcpParameters() = default;

  static webrtc::scoped_refptr<RtcpParameters> Create() {
    return webrtc::make_ref_counted<RtcpParameters>();
  }

  static webrtc::scoped_refptr<RtcpParameters> FromNative(const webrtc::RtcpParameters& native) {
    auto rtcp = webrtc::make_ref_counted<RtcpParameters>();
    rtcp->rtc_parameters = native;
    return rtcp;
  }

  const char* cname() const { return rtc_parameters.cname.c_str(); }

  void set_cname(const char* cname) { rtc_parameters.cname = std::string(cname); }

  bool reduced_size() const { return rtc_parameters.reduced_size; }

  void set_reduced_size(bool reduced_size) { rtc_parameters.reduced_size = reduced_size; }

  webrtc::RtcpParameters rtc_parameters;
};

class RtpEncodingParameters : public webrtc::RefCountInterface {
 public:
  RtpEncodingParameters() = default;
  virtual ~RtpEncodingParameters() = default;

  static webrtc::scoped_refptr<RtpEncodingParameters> Create() {
    return webrtc::make_ref_counted<RtpEncodingParameters>();
  }

  static webrtc::scoped_refptr<RtpEncodingParameters> FromNative(
      const webrtc::RtpEncodingParameters& native) {
    auto encoding = webrtc::make_ref_counted<RtpEncodingParameters>();
    encoding->rtc_parameters = native;
    return encoding;
  }

  bool active() const { return rtc_parameters.active; }

  void set_active(bool active) { rtc_parameters.active = active; }

  bool has_max_bitrate_bps() const { return rtc_parameters.max_bitrate_bps.has_value(); }

  uint32_t max_bitrate_bps() const { return rtc_parameters.max_bitrate_bps.value(); }

  void set_max_bitrate_bps(uint32_t bitrate) { rtc_parameters.max_bitrate_bps = bitrate; }

  bool has_min_bitrate_bps() const { return rtc_parameters.min_bitrate_bps.has_value(); }

  uint32_t min_bitrate_bps() const { return rtc_parameters.min_bitrate_bps.value(); }

  void set_min_bitrate_bps(uint32_t bitrate) { rtc_parameters.min_bitrate_bps = bitrate; }

  bool has_max_framerate() const { return rtc_parameters.max_framerate.has_value(); }

  double max_framerate() const { return rtc_parameters.max_framerate.value(); }

  void set_max_framerate(double framerate) { rtc_parameters.max_framerate = framerate; }

  bool has_scale_resolution_down_by() const {
    return rtc_parameters.scale_resolution_down_by.has_value();
  }

  double scale_resolution_down_by() const {
    return rtc_parameters.scale_resolution_down_by.value();
  }

  void set_scale_resolution_down_by(double scale) {
    rtc_parameters.scale_resolution_down_by = scale;
  }

  bool has_num_temporal_layers() const { return rtc_parameters.num_temporal_layers.has_value(); }

  uint8_t num_temporal_layers() const { return rtc_parameters.num_temporal_layers.value(); }

  void set_num_temporal_layers(uint8_t num_layers) {
    rtc_parameters.num_temporal_layers = num_layers;
  }

  bool has_ssrc() const { return rtc_parameters.ssrc.has_value(); }

  uint32_t ssrc() const { return rtc_parameters.ssrc.value(); }

  bool has_scalability_mode() const { return rtc_parameters.scalability_mode.has_value(); }

  const char* scalability_mode() const { return rtc_parameters.scalability_mode.value().c_str(); }

  void set_scalability_mode(const char* mode) {
    rtc_parameters.scalability_mode = std::string(mode);
  }

  void set_rid(const char* rid) { rtc_parameters.rid = std::string(rid); }

  std::string rid() const { return rtc_parameters.rid; }

  webrtc::RtpEncodingParameters rtc_parameters;
};

class RtpHeaderExtensionCapability : public webrtc::RefCountInterface {
 public:
  RtpHeaderExtensionCapability() = default;
  virtual ~RtpHeaderExtensionCapability() {
    RTC_LOG(LS_INFO) << "RtpHeaderExtensionCapability destroyed";
  }

  static webrtc::scoped_refptr<RtpHeaderExtensionCapability> Create() {
    return webrtc::make_ref_counted<RtpHeaderExtensionCapability>();
  }

  static webrtc::scoped_refptr<RtpHeaderExtensionCapability> FromNative(
      const webrtc::RtpHeaderExtensionCapability& native) {
    auto ext = webrtc::make_ref_counted<RtpHeaderExtensionCapability>();
    ext->rtc_capability = native;
    return ext;
  }

  std::string uri() const { return rtc_capability.uri; }

  void set_uri(const char* uri) { rtc_capability.uri = std::string(uri); }

  int preferred_id() const { return rtc_capability.preferred_id.value(); }

  bool has_preferred_id() const { return rtc_capability.preferred_id.has_value(); }

  void set_preferred_id(int id) { rtc_capability.preferred_id = id; }

  lkRtpTransceiverDirection direction() const {
    return static_cast<lkRtpTransceiverDirection>(rtc_capability.direction);
  }

  void set_direction(lkRtpTransceiverDirection direction) {
    rtc_capability.direction = static_cast<webrtc::RtpTransceiverDirection>(direction);
  }

  webrtc::RtpHeaderExtensionCapability rtc_capability;
};

class RtpCapabilities : public webrtc::RefCountInterface {
 public:
  RtpCapabilities() = default;
  virtual ~RtpCapabilities() { RTC_LOG(LS_INFO) << "RtpCapabilities destroyed"; }

  static webrtc::scoped_refptr<RtpCapabilities> Create() {
    return webrtc::make_ref_counted<RtpCapabilities>();
  }

  static webrtc::scoped_refptr<RtpCapabilities> FromNative(const webrtc::RtpCapabilities& native) {
    auto caps = webrtc::make_ref_counted<RtpCapabilities>();
    for (const auto& codec : native.codecs) {
      caps->codecs.push_back(RtpCodecCapability::FromNative(codec));
    }
    for (const auto& ext : native.header_extensions) {
      caps->header_extensions.push_back(RtpHeaderExtensionCapability::FromNative(ext));
    }
    return caps;
  }

  lkVectorGeneric* GetCodecs() {
    auto vec =
        webrtc::make_ref_counted<LKVector<webrtc::scoped_refptr<RtpCodecCapability>>>(codecs);
    return reinterpret_cast<lkVectorGeneric*>(vec.release());
  }

  lkVectorGeneric* GetHeaderExtensions() {
    auto vec =
        webrtc::make_ref_counted<LKVector<webrtc::scoped_refptr<RtpHeaderExtensionCapability>>>(
            header_extensions);
    return reinterpret_cast<lkVectorGeneric*>(vec.release());
  }

  int add_codec(webrtc::scoped_refptr<RtpCodecCapability> codec) {
    codecs.push_back(codec);
    return static_cast<int>(codecs.size());
  }

  int add_header_extension(webrtc::scoped_refptr<RtpHeaderExtensionCapability> header_extension) {
    header_extensions.push_back(header_extension);
    return static_cast<int>(header_extensions.size());
  }

  std::vector<webrtc::scoped_refptr<RtpCodecCapability>> codecs;
  std::vector<webrtc::scoped_refptr<RtpHeaderExtensionCapability>> header_extensions;
};

class RtpHeaderExtensionParameters : public webrtc::RefCountInterface {
 public:
  RtpHeaderExtensionParameters() = default;
  virtual ~RtpHeaderExtensionParameters() = default;

  static webrtc::scoped_refptr<RtpHeaderExtensionParameters> Create() {
    return webrtc::make_ref_counted<RtpHeaderExtensionParameters>();
  }

  static webrtc::scoped_refptr<RtpHeaderExtensionParameters> FromNative(
      const webrtc::RtpExtension& native) {
    auto ext = webrtc::make_ref_counted<RtpHeaderExtensionParameters>();
    ext->rtc_rtp_extension = native;
    return ext;
  }

  std::string uri() const { return rtc_rtp_extension.uri; }

  void set_uri(const char* uri) { rtc_rtp_extension.uri = std::string(uri); }

  int id() const { return rtc_rtp_extension.id; }

  void set_id(int id) { rtc_rtp_extension.id = id; }

  bool encrypted() const { return rtc_rtp_extension.encrypt; }

  void set_encrypted(bool encrypted) { rtc_rtp_extension.encrypt = encrypted; }

  webrtc::RtpExtension rtc_rtp_extension;
};

class RtpParameters : public webrtc::RefCountInterface {
 public:
  RtpParameters() = default;
  virtual ~RtpParameters() = default;

  static webrtc::scoped_refptr<RtpParameters> Create() {
    return webrtc::make_ref_counted<RtpParameters>();
  }

  static webrtc::scoped_refptr<RtpParameters> FromNative(const webrtc::RtpParameters& native) {
    auto params = webrtc::make_ref_counted<RtpParameters>();
    for (const auto& codec : native.codecs) {
      params->codecs.push_back(RtpCodecParameters::FromNative(codec));
    }

    for (const auto& ext : native.header_extensions) {
      params->header_extensions.push_back(RtpHeaderExtensionParameters::FromNative(ext));
    }
    params->rtcp = RtcpParameters::FromNative(native.rtcp);
    return params;
  }

  webrtc::RtpParameters rtc_parameters() {
    webrtc::RtpParameters params;
    for (const auto& codec : codecs) {
      params.codecs.push_back(codec->rtc_parameters);
    }
    for (const auto& ext : header_extensions) {
      params.header_extensions.push_back(ext->rtc_rtp_extension);
    }
    if (rtcp) {
      params.rtcp = rtcp->rtc_parameters;
    }
    return params;
  }

  void set_lk_codecs(lkVectorGeneric* lk_codecs) {
    codecs.clear();
    auto vec = reinterpret_cast<LKVector<webrtc::scoped_refptr<RtpCodecParameters>>*>(lk_codecs);
    for (size_t i = 0; i < vec->size(); i++) {
      codecs.push_back(vec->get_at(i));
    }
  }

  void set_rtcp(webrtc::scoped_refptr<RtcpParameters> rtcp_params) {
    rtcp->rtc_parameters = rtcp_params->rtc_parameters;
  }

  void set_lk_header_extensions(lkVectorGeneric* lk_header_extensions) {
    header_extensions.clear();
    auto vec =
        reinterpret_cast<LKVector<webrtc::scoped_refptr<RtpHeaderExtensionParameters>>*>(
            lk_header_extensions);
    for (size_t i = 0; i < vec->size(); i++) {
      header_extensions.push_back(vec->get_at(i));
    }
  }

  lkVectorGeneric* GetCodecs() {
    auto vec =
        webrtc::make_ref_counted<LKVector<webrtc::scoped_refptr<RtpCodecParameters>>>(codecs);
    return reinterpret_cast<lkVectorGeneric*>(vec.release());
  }

  lkVectorGeneric* GetHeaderExtensions() {
    auto vec =
        webrtc::make_ref_counted<LKVector<webrtc::scoped_refptr<RtpHeaderExtensionParameters>>>(
            header_extensions);
    return reinterpret_cast<lkVectorGeneric*>(vec.release());
  }

  std::vector<webrtc::scoped_refptr<RtpCodecParameters>> codecs;
  std::vector<webrtc::scoped_refptr<RtpHeaderExtensionParameters>> header_extensions;
  webrtc::scoped_refptr<RtcpParameters> rtcp;
};

class RtpTransceiverInit : public webrtc::RefCountInterface {
 public:
  RtpTransceiverInit() = default;
  virtual ~RtpTransceiverInit() = default;

  static webrtc::scoped_refptr<RtpTransceiverInit> Create() {
    return webrtc::make_ref_counted<RtpTransceiverInit>();
  }

  lkRtpTransceiverDirection direction() const {
    return static_cast<lkRtpTransceiverDirection>(rtc_init.direction);
  }

  void set_direction(lkRtpTransceiverDirection direction) {
    rtc_init.direction = static_cast<webrtc::RtpTransceiverDirection>(direction);
  }

  void set_stream_ids(const std::vector<std::string>& stream_ids) {
    rtc_init.stream_ids = stream_ids;
  }

  void set_lk_stream_ids(lkVectorGeneric* stream_ids) {
    rtc_init.stream_ids.clear();
    auto vec = reinterpret_cast<LKVector<webrtc::scoped_refptr<LKString>>*>(stream_ids);
    for (size_t i = 0; i < vec->size(); i++) {
      rtc_init.stream_ids.push_back(vec->get_at(i)->get());
    }
  }

  void set_send_encodings(const std::vector<RtpEncodingParameters>& send_encodings) {
    for (const auto& encoding : send_encodings) {
      rtc_init.send_encodings.push_back(encoding.rtc_parameters);
    }
  }

  void set_lk_send_encodings(lkVectorGeneric* send_encodings) {
    rtc_init.send_encodings.clear();
    auto vec =
        reinterpret_cast<LKVector<webrtc::scoped_refptr<RtpEncodingParameters>>*>(send_encodings);
    for (size_t i = 0; i < vec->size(); i++) {
      rtc_init.send_encodings.push_back(vec->get_at(i)->rtc_parameters);
    }
  }

  webrtc::RtpTransceiverInit rtc_init;
};

}  // namespace livekit
