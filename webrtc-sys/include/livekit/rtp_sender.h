#ifndef CLIENT_SDK_NATIVE_RTP_SENDER_H
#define CLIENT_SDK_NATIVE_RTP_SENDER_H

#include <memory>

#include "api/rtp_sender_interface.h"
#include "livekit/media_stream.h"
#include "livekit/rtc_error.h"
#include "livekit/rtp_parameters.h"
#include "rust/cxx.h"
#include "webrtc-sys/src/rtp_parameters.rs.h"

namespace livekit {

class RtpSender {
 public:
  explicit RtpSender(rtc::scoped_refptr<webrtc::RtpSenderInterface> sender);

  bool set_track(std::shared_ptr<MediaStreamTrack> track) const {
    return sender_->SetTrack(track->get().get());
  }

  std::unique_ptr<MediaStreamTrack> track() const {
    return MediaStreamTrack::from(sender_->track());
  }

  uint32_t ssrc() const { return sender_->ssrc(); }

  MediaType media_type() const {
    return static_cast<MediaType>(sender_->media_type());
  }

  rust::String id() const { return sender_->id(); }

  rust::Vec<rust::String> stream_ids() const {
    rust::Vec<rust::String> vec;
    for (auto str : sender_->stream_ids())
      vec.push_back(str);

    return vec;
  }

  void set_streams(const rust::Vec<rust::String>& stream_ids) const {
    std::vector<std::string> std_stream_ids(stream_ids.begin(),
                                            stream_ids.end());
    sender_->SetStreams(std_stream_ids);
  }

  rust::Vec<RtpEncodingParameters> init_send_encodings() const {
    rust::Vec<RtpEncodingParameters> encodings;
    for (auto encoding : sender_->init_send_encodings())
      encodings.push_back(to_rust_rtp_encoding_parameters(encoding));
    return encodings;
  }

  RtpParameters get_parameters() const {
    return to_rust_rtp_parameters(sender_->GetParameters());
  }

  void set_parameters(RtpParameters params) const {
    auto error = sender_->SetParameters(to_native_rtp_parameters(params));
    if (!error.ok())
      throw std::runtime_error(serialize_error(to_error(error)));
  }

  rtc::scoped_refptr<webrtc::RtpSenderInterface> get() const { return sender_; }

 private:
  rtc::scoped_refptr<webrtc::RtpSenderInterface> sender_;
};

static std::shared_ptr<RtpSender> _shared_rtp_sender() {
  return nullptr;  // Ignore
}
}  // namespace livekit

#endif  // CLIENT_SDK_NATIVE_RTP_SENDER_H
