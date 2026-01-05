#ifndef LIVEKIT_RTC_MEDIA_STREAM_H
#define LIVEKIT_RTC_MEDIA_STREAM_H

#include "api/media_stream_interface.h"
#include "api/scoped_refptr.h"
#include "rtc_base/logging.h"

namespace livekit {

class MediaStream : public webrtc::RefCountInterface {
 public:
  explicit MediaStream(
      webrtc::scoped_refptr<webrtc::MediaStreamInterface> stream)
      : stream_(stream) {}

  virtual ~MediaStream() {
    RTC_LOG(LS_INFO) << "MediaStream destroyed";
  }

  std::string id() const { return stream_->id(); }

  webrtc::MediaStreamInterface* media_stream() const { return stream_.get(); }

 private:
  webrtc::scoped_refptr<webrtc::MediaStreamInterface> stream_;
};

}

#endif  // LIVEKIT_RTC_MEDIA_STREAM_H