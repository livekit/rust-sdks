/*
 * Copyright 2023 LiveKit
 *
 * Licensed under the Apache License, Version 2.0 (the “License”);
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an “AS IS” BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

#include "livekit/webrtc.h"

#include <memory>

#include "livekit/audio_track.h"
#include "livekit/media_stream_track.h"
#include "livekit/rtp_receiver.h"
#include "livekit/rtp_sender.h"
#include "livekit/video_track.h"
#include "rtc_base/helpers.h"
#include "rtc_base/logging.h"
#include "rtc_base/synchronization/mutex.h"

namespace livekit {
RtcRuntime::RtcRuntime() {
  RTC_LOG(LS_INFO) << "RtcRuntime()";
  RTC_CHECK(rtc::InitializeSSL()) << "Failed to InitializeSSL()";

  network_thread_ = rtc::Thread::CreateWithSocketServer();
  network_thread_->SetName("network_thread", &network_thread_);
  network_thread_->Start();
  worker_thread_ = rtc::Thread::Create();
  worker_thread_->SetName("worker_thread", &worker_thread_);
  worker_thread_->Start();
  signaling_thread_ = rtc::Thread::Create();
  signaling_thread_->SetName("signaling_thread", &signaling_thread_);
  signaling_thread_->Start();
}

RtcRuntime::~RtcRuntime() {
  RTC_LOG(LS_INFO) << "~RtcRuntime()";

  rtc::ThreadManager::Instance()->SetCurrentThread(nullptr);
  RTC_CHECK(rtc::CleanupSSL()) << "Failed to CleanupSSL()";

  worker_thread_->Stop();
  signaling_thread_->Stop();
  network_thread_->Stop();
}

rtc::Thread* RtcRuntime::network_thread() const {
  return network_thread_.get();
}

rtc::Thread* RtcRuntime::worker_thread() const {
  return worker_thread_.get();
}

rtc::Thread* RtcRuntime::signaling_thread() const {
  return signaling_thread_.get();
}

std::shared_ptr<MediaStreamTrack> RtcRuntime::get_or_create_media_stream_track(
    rtc::scoped_refptr<webrtc::MediaStreamTrackInterface> rtc_track) {
  webrtc::MutexLock lock(&mutex_);
  for (std::weak_ptr<MediaStreamTrack> weak_existing_track :
       media_stream_tracks_) {
    if (std::shared_ptr<MediaStreamTrack> existing_track =
            weak_existing_track.lock()) {
      if (existing_track->rtc_track() == rtc_track) {
        return existing_track;
      }
    }
  }

  if (rtc_track->kind() == webrtc::MediaStreamTrackInterface::kVideoKind) {
    std::shared_ptr<VideoTrack> video_track =
        std::shared_ptr<VideoTrack>(new VideoTrack(
            shared_from_this(),
            rtc::scoped_refptr<webrtc::VideoTrackInterface>(
                static_cast<webrtc::VideoTrackInterface*>(rtc_track.get()))));

    media_stream_tracks_.push_back(
        std::static_pointer_cast<MediaStreamTrack>(video_track));
    return video_track;
  } else {
    std::shared_ptr<AudioTrack> audio_track =
        std::shared_ptr<AudioTrack>(new AudioTrack(
            shared_from_this(),
            rtc::scoped_refptr<webrtc::AudioTrackInterface>(
                static_cast<webrtc::AudioTrackInterface*>(rtc_track.get()))));

    media_stream_tracks_.push_back(
        std::static_pointer_cast<MediaStreamTrack>(audio_track));
    return audio_track;
  }
}

std::shared_ptr<AudioTrack> RtcRuntime::get_or_create_audio_track(
    rtc::scoped_refptr<webrtc::AudioTrackInterface> track) {
  return std::static_pointer_cast<AudioTrack>(
      get_or_create_media_stream_track(track));
}

std::shared_ptr<VideoTrack> RtcRuntime::get_or_create_video_track(
    rtc::scoped_refptr<webrtc::VideoTrackInterface> track) {
  return std::static_pointer_cast<VideoTrack>(
      get_or_create_media_stream_track(track));
}

rust::String create_random_uuid() {
  return rtc::CreateRandomUuid();
}

}  // namespace livekit
