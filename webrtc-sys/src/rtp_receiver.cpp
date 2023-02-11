//
// Created by Théo Monnom on 01/09/2022.
//

#include "livekit/rtp_receiver.h"

#include "absl/types/optional.h"
#include "api/rtp_transceiver_interface.h"
#include "livekit/media_stream.h"
#include "livekit/rtp_parameters.h"
#include "livekit/rtp_transceiver.h"

namespace livekit {

webrtc::RtpTransceiverInit to_native_rtp_transceiver_init(
    RtpTransceiverInit init) {
  {
    webrtc::RtpTransceiverInit native;
    native.direction =
        static_cast<webrtc::RtpTransceiverDirection>(init.direction);
    native.stream_ids = std::vector<std::string>(init.stream_ids.begin(),
                                                 init.stream_ids.end());
    for (auto encoding : init.send_encodings)
      native.send_encodings.push_back(
          to_native_rtp_encoding_paramters(encoding));
    return native;
  }
}

RtpReceiver::RtpReceiver(
    rtc::scoped_refptr<webrtc::RtpReceiverInterface> receiver)
    : receiver_(std::move(receiver)) {}

std::shared_ptr<MediaStreamTrack> RtpReceiver::track() const {
  return MediaStreamTrack::from(receiver_->track());
}

rust::Vec<rust::String> RtpReceiver::stream_ids() const {
  rust::Vec<rust::String> rust;
  for (auto id : receiver_->stream_ids())
    rust.push_back(id);
  return rust;
}

rust::Vec<MediaStreamPtr> RtpReceiver::streams() const {
  rust::Vec<MediaStreamPtr> rust;
  for (auto stream : receiver_->streams())
    rust.push_back(MediaStreamPtr{std::make_shared<MediaStream>(stream)});
  return rust;
}

MediaType RtpReceiver::media_type() const {
  return static_cast<MediaType>(receiver_->media_type());
}

rust::String RtpReceiver::id() const {
  return receiver_->id();
}

RtpParameters RtpReceiver::get_parameters() const {
  return to_rust_rtp_parameters(receiver_->GetParameters());
}

void RtpReceiver::set_jitter_buffer_minimum_delay(bool is_some,
                                                  double delay_seconds) const {
  receiver_->SetJitterBufferMinimumDelay(
      is_some ? absl::make_optional(delay_seconds) : absl::nullopt);
}

}  // namespace livekit
