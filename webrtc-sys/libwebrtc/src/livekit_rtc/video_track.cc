#include "livekit_rtc/video_track.h"

namespace livekit {

VideoTrack::VideoTrack(webrtc::scoped_refptr<webrtc::VideoTrackInterface> track)
    : MediaStreamTrack(std::move(track)) {}

VideoTrack::~VideoTrack() {
  webrtc::MutexLock lock(&mutex_);
  for (auto& sink : sinks_) {
    track()->RemoveSink(sink.get());
  }
}

void VideoTrack::add_sink(
    const webrtc::scoped_refptr<NativeVideoSink>& sink) const {
  webrtc::MutexLock lock(&mutex_);
  track()->AddOrUpdateSink(
      sink.get(),
      webrtc::VideoSinkWants());  // TODO(theomonnom): Expose
                                  // VideoSinkWants to Rust?
  sinks_.push_back(sink);
}

void VideoTrack::remove_sink(
    const webrtc::scoped_refptr<NativeVideoSink>& sink) const {
  webrtc::MutexLock lock(&mutex_);
  track()->RemoveSink(sink.get());
  sinks_.erase(std::remove(sinks_.begin(), sinks_.end(), sink), sinks_.end());
}

void VideoTrack::set_should_receive(bool should_receive) const {
  track()->set_should_receive(should_receive);
}

bool VideoTrack::should_receive() const {
  return track()->should_receive();
}

lkContentHint VideoTrack::content_hint() const {
  return static_cast<lkContentHint>(track()->content_hint());
}

void VideoTrack::set_content_hint(lkContentHint hint) const {
  track()->set_content_hint(
      static_cast<webrtc::VideoTrackInterface::ContentHint>(hint));
}

NativeVideoSink::NativeVideoSink(const lkVideoSinkCallabacks* callbacks,
                                 void* userdata)
    : callbacks_(callbacks), userdata_(userdata) {}

void NativeVideoSink::OnFrame(const webrtc::VideoFrame& frame) {
  auto lk_frame = webrtc::make_ref_counted<VideoFrame>(frame);
  callbacks_->onFrame(reinterpret_cast<const lkVideoFrame*>(lk_frame.release()),
                      userdata_);
}

void NativeVideoSink::OnDiscardedFrame() {
  callbacks_->onDiscardedFrame(userdata_);
}

void NativeVideoSink::OnConstraintsChanged(
    const webrtc::VideoTrackSourceConstraints& constraints) {
  lkVideoTrackSourceConstraints cst;
  cst.minFps = constraints.min_fps.value_or(0.0);
  cst.maxFps = constraints.max_fps.value_or(0.0);
  callbacks_->onConstraintsChanged(&cst, userdata_);
}

VideoTrackSource::InternalSource::InternalSource(
    const lkVideoResolution& resolution)
    : webrtc::AdaptedVideoTrackSource(4), resolution_(resolution) {}

VideoTrackSource::InternalSource::~InternalSource() {}

bool VideoTrackSource::InternalSource::is_screencast() const {
  return false;
}

std::optional<bool> VideoTrackSource::InternalSource::needs_denoising() const {
  return false;
}

webrtc::MediaSourceInterface::SourceState
VideoTrackSource::InternalSource::state() const {
  return SourceState::kLive;
}

bool VideoTrackSource::InternalSource::remote() const {
  return false;
}

lkVideoResolution VideoTrackSource::InternalSource::video_resolution() const {
  webrtc::MutexLock lock(&mutex_);
  return resolution_;
}

bool VideoTrackSource::InternalSource::on_captured_frame(
    const webrtc::VideoFrame& frame) {
  webrtc::MutexLock lock(&mutex_);

  int64_t aligned_timestamp_us = timestamp_aligner_.TranslateTimestamp(
      frame.timestamp_us(), webrtc::TimeMicros());

  webrtc::scoped_refptr<webrtc::VideoFrameBuffer> buffer =
      frame.video_frame_buffer();

  if (resolution_.height == 0 || resolution_.width == 0) {
    resolution_.width = static_cast<uint32_t>(buffer->width());
    resolution_.height = static_cast<uint32_t>(buffer->height());
  }

  int adapted_width, adapted_height, crop_width, crop_height, crop_x, crop_y;
  if (!AdaptFrame(buffer->width(), buffer->height(), aligned_timestamp_us,
                  &adapted_width, &adapted_height, &crop_width, &crop_height,
                  &crop_x, &crop_y)) {
    return false;
  }

  if (adapted_width != frame.width() || adapted_height != frame.height()) {
    buffer = buffer->CropAndScale(crop_x, crop_y, crop_width, crop_height,
                                  adapted_width, adapted_height);
  }

  webrtc::VideoRotation rotation = frame.rotation();
  if (apply_rotation() && rotation != webrtc::kVideoRotation_0) {
    // If the buffer is I420, webrtc::AdaptedVideoTrackSource will handle the
    // rotation for us.
    buffer = buffer->ToI420();
  }

  OnFrame(webrtc::VideoFrame::Builder()
              .set_video_frame_buffer(buffer)
              .set_rotation(rotation)
              .set_timestamp_us(aligned_timestamp_us)
              .build());

  return true;
}

VideoTrackSource::VideoTrackSource(const lkVideoResolution& resolution) {
  source_ = webrtc::make_ref_counted<InternalSource>(resolution);
}

lkVideoResolution VideoTrackSource::video_resolution() const {
  return source_->video_resolution();
}

bool VideoTrackSource::on_captured_frame(
    const webrtc::scoped_refptr<VideoFrame> frame) const {
  auto rtc_frame = frame->rtc_frame();
  return source_->on_captured_frame(rtc_frame);
}

webrtc::scoped_refptr<VideoTrackSource::InternalSource> VideoTrackSource::get()
    const {
  return source_;
}

}  // namespace livekit