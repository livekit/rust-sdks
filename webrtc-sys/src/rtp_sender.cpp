#include "livekit/rtp_sender.h"

namespace livekit {

RtpSender::RtpSender(rtc::scoped_refptr<webrtc::RtpSenderInterface> sender)
    : sender_(std::move(sender)) {}

bool RtpSender::set_track(std::shared_ptr<MediaStreamTrack> track) const {
  return sender_->SetTrack(track->get().get());
}

std::shared_ptr<MediaStreamTrack> RtpSender::track() const {
  return MediaStreamTrack::from(sender_->track());
}

uint32_t RtpSender::ssrc() const {
  return sender_->ssrc();
}

MediaType RtpSender::media_type() const {
  return static_cast<MediaType>(sender_->media_type());
}

rust::String RtpSender::id() const {
  return sender_->id();
}

rust::Vec<rust::String> RtpSender::stream_ids() const {
  rust::Vec<rust::String> vec;
  for (auto str : sender_->stream_ids())
    vec.push_back(str);

  return vec;
}

void RtpSender::set_streams(const rust::Vec<rust::String>& stream_ids) const {
  std::vector<std::string> std_stream_ids(stream_ids.begin(), stream_ids.end());
  sender_->SetStreams(std_stream_ids);
}

rust::Vec<RtpEncodingParameters> RtpSender::init_send_encodings() const {
  rust::Vec<RtpEncodingParameters> encodings;
  for (auto encoding : sender_->init_send_encodings())
    encodings.push_back(to_rust_rtp_encoding_parameters(encoding));
  return encodings;
}

RtpParameters RtpSender::get_parameters() const {
  return to_rust_rtp_parameters(sender_->GetParameters());
}

void RtpSender::set_parameters(RtpParameters params) const {
  auto error = sender_->SetParameters(to_native_rtp_parameters(params));
  if (!error.ok())
    throw std::runtime_error(serialize_error(to_error(error)));
}

}  // namespace livekit
