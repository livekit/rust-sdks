//
// Created by Théo Monnom on 01/09/2022.
//

#ifndef CLIENT_SDK_NATIVE_RTP_RECEIVER_H
#define CLIENT_SDK_NATIVE_RTP_RECEIVER_H

#include <memory>

#include "api/rtp_receiver_interface.h"
#include "livekit/media_stream.h"
#include "livekit/rtp_parameters.h"
#include "rust/cxx.h"
#include "webrtc-sys/src/rtp_parameters.rs.h"

namespace livekit {

struct MediaStreamPtr;

// TODO(theomonnom): Implement RtpReceiverObserverInterface?
// TODO(theomonnom): RtpSource
// TODO(theomonnom): FrameTransformer & FrameDecryptor interface
class RtpReceiver {
 public:
  explicit RtpReceiver(
      rtc::scoped_refptr<webrtc::RtpReceiverInterface> receiver);

  std::shared_ptr<MediaStreamTrack> track() const;

  rust::Vec<rust::String> stream_ids() const;
  rust::Vec<MediaStreamPtr> streams() const;

  MediaType media_type() const;
  rust::String id() const;

  RtpParameters get_parameters() const;

  // bool set_parameters(RtpParameters parameters) const; // Seems unsupported

  void set_jitter_buffer_minimum_delay(bool is_some,
                                       double delay_seconds) const;

 private:
  rtc::scoped_refptr<webrtc::RtpReceiverInterface> receiver_;
};

static std::shared_ptr<RtpReceiver> _shared_rtp_receiver() {
  return nullptr;
}

}  // namespace livekit

#endif  // CLIENT_SDK_NATIVE_RTP_RECEIVER_H
