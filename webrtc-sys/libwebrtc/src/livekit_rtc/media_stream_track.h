#ifndef LIVEKIT_RTC_MEDIA_STREAM_TRACK_H
#define LIVEKIT_RTC_MEDIA_STREAM_TRACK_H

#include "api/media_stream_interface.h"
#include "api/scoped_refptr.h"
#include "livekit_rtc/include/capi.h"

namespace livekit {

class MediaStreamTrack : public webrtc::RefCountInterface {
 public:
  explicit MediaStreamTrack(
      webrtc::scoped_refptr<webrtc::MediaStreamTrackInterface> track)
      : track_(track) {}

  std::string id() const { return track_->id(); }

  std::string kind() const { return track_->kind(); }

  bool enabled() const { return track_->enabled(); }

  void set_enabled(bool enabled) const { track_->set_enabled(enabled); }

  lkRtcTrackState state() const {
    return static_cast<lkRtcTrackState>(track_->state());
  }

  webrtc::scoped_refptr<webrtc::MediaStreamTrackInterface> rtc_track() {
    return track_;
  }

  webrtc::MediaStreamTrackInterface* track() const { return track_.get(); }

 private:
  webrtc::scoped_refptr<webrtc::MediaStreamTrackInterface> track_;
};

}  // namespace livekit

#endif  // LIVEKIT_RTC_MEDIA_STREAM_TRACK_H