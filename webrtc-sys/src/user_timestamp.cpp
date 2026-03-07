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

#include "livekit/user_timestamp.h"

#include <cstring>
#include <optional>

#include "api/make_ref_counted.h"
#include "livekit/peer_connection_factory.h"
#include "livekit/rtp_receiver.h"
#include "livekit/rtp_sender.h"
#include "rtc_base/logging.h"
#include "webrtc-sys/src/user_timestamp.rs.h"

namespace livekit_ffi {

// UserTimestampTransformer implementation

UserTimestampTransformer::UserTimestampTransformer(Direction direction)
    : direction_(direction) {}

void UserTimestampTransformer::Transform(
    std::unique_ptr<webrtc::TransformableFrameInterface> frame) {
  uint32_t ssrc = frame->GetSsrc();
  uint32_t rtp_timestamp = frame->GetTimestamp();

  if (!enabled_.load()) {
    rtc::scoped_refptr<webrtc::TransformedFrameCallback> cb;
    {
      webrtc::MutexLock lock(&mutex_);
      auto it = sink_callbacks_.find(ssrc);
      if (it != sink_callbacks_.end()) {
        cb = it->second;
      } else {
        cb = callback_;
      }
    }

    if (cb) {
      cb->OnTransformedFrame(std::move(frame));
    } else {
      RTC_LOG(LS_WARNING)
          << "UserTimestampTransformer::Transform (disabled) has no callback"
          << " direction="
          << (direction_ == Direction::kSend ? "send" : "recv")
          << " ssrc=" << ssrc << " rtp_ts=" << rtp_timestamp;
    }
    return;
  }

  if (direction_ == Direction::kSend) {
    TransformSend(std::move(frame));
  } else {
    TransformReceive(std::move(frame));
  }
}

void UserTimestampTransformer::TransformSend(
    std::unique_ptr<webrtc::TransformableFrameInterface> frame) {
  uint32_t rtp_timestamp = frame->GetTimestamp();
  uint32_t ssrc = frame->GetSsrc();

  auto data = frame->GetData();

  // Look up the frame metadata by the frame's capture time.
  // CaptureTime() returns Timestamp::Millis(capture_time_ms_) where
  // capture_time_ms_ = timestamp_us / 1000.  So capture_time->us()
  // has millisecond precision (bottom 3 digits always zero).
  // store_frame_metadata() truncates its key the same way.
  FrameMetadata meta_to_embed{0, 0};
  auto capture_time = frame->CaptureTime();
  if (capture_time.has_value()) {
    int64_t capture_us = capture_time->us();

    webrtc::MutexLock lock(&send_map_mutex_);
    auto it = send_map_.find(capture_us);
    if (it != send_map_.end()) {
      meta_to_embed = it->second;
      // Don't erase — simulcast layers share the same capture time.
      // Entries are pruned by capacity in store_frame_metadata().
    }
  } else {
    RTC_LOG(LS_WARNING)
        << "UserTimestampTransformer::TransformSend CaptureTime() not available"
        << " ssrc=" << ssrc << " rtp_ts=" << rtp_timestamp;
  }

  // Always append trailer when enabled (even if timestamp is 0,
  // which indicates no metadata was set for this frame)
  std::vector<uint8_t> new_data;
  if (enabled_.load()) {
    new_data = AppendTrailer(data, meta_to_embed.user_timestamp_us,
                             meta_to_embed.frame_id);
    frame->SetData(rtc::ArrayView<const uint8_t>(new_data));
  }

  // Forward to the appropriate callback (either global or per-SSRC sink).
  rtc::scoped_refptr<webrtc::TransformedFrameCallback> cb;
  {
    webrtc::MutexLock lock(&mutex_);
    auto it = sink_callbacks_.find(ssrc);
    if (it != sink_callbacks_.end()) {
      cb = it->second;
    } else {
      cb = callback_;
    }
  }

  if (cb) {
    cb->OnTransformedFrame(std::move(frame));
  } else {
    RTC_LOG(LS_WARNING)
        << "UserTimestampTransformer::TransformSend has no callback"
        << " ssrc=" << ssrc << " rtp_ts=" << rtp_timestamp;
  }
}

void UserTimestampTransformer::TransformReceive(
    std::unique_ptr<webrtc::TransformableFrameInterface> frame) {
  uint32_t ssrc = frame->GetSsrc();
  uint32_t rtp_timestamp = frame->GetTimestamp();
  auto data = frame->GetData();
  std::vector<uint8_t> stripped_data;

  auto meta = ExtractTrailer(data, stripped_data);

  if (meta.has_value()) {
    // Store in the receive map keyed by RTP timestamp so decoded frames
    // can look up their metadata regardless of frame drops.
    {
      webrtc::MutexLock lock(&recv_map_mutex_);
      // Evict oldest entry if at capacity
      while (recv_map_.size() >= kMaxRecvMapEntries && !recv_map_order_.empty()) {
        recv_map_.erase(recv_map_order_.front());
        recv_map_order_.pop_front();
      }
      if (recv_map_.find(rtp_timestamp) == recv_map_.end()) {
        recv_map_order_.push_back(rtp_timestamp);
      }
      recv_map_[rtp_timestamp] = meta.value();
    }

    // Update frame with stripped data
    frame->SetData(rtc::ArrayView<const uint8_t>(stripped_data));
  }

  // Forward to the appropriate callback (either global or per-SSRC sink).
  rtc::scoped_refptr<webrtc::TransformedFrameCallback> cb;
  {
    webrtc::MutexLock lock(&mutex_);
    auto it = sink_callbacks_.find(ssrc);
    if (it != sink_callbacks_.end()) {
      cb = it->second;
    } else {
      cb = callback_;
    }
  }

  if (cb) {
    cb->OnTransformedFrame(std::move(frame));
  } else {
    RTC_LOG(LS_WARNING)
        << "UserTimestampTransformer::TransformReceive has no callback"
        << " ssrc=" << ssrc << " rtp_ts=" << rtp_timestamp;
  }
}

std::vector<uint8_t> UserTimestampTransformer::AppendTrailer(
    rtc::ArrayView<const uint8_t> data,
    int64_t user_timestamp_us,
    uint32_t frame_id) {
  std::vector<uint8_t> result;
  result.reserve(data.size() + kUserTimestampTrailerSize);

  // Copy original data
  result.insert(result.end(), data.begin(), data.end());

  // Append user_timestamp_us (big-endian, 8 bytes)
  for (int i = 7; i >= 0; --i) {
    result.push_back(
        static_cast<uint8_t>((user_timestamp_us >> (i * 8)) & 0xFF));
  }

  // Append frame_id (big-endian, 4 bytes)
  for (int i = 3; i >= 0; --i) {
    result.push_back(
        static_cast<uint8_t>((frame_id >> (i * 8)) & 0xFF));
  }

  // Append magic bytes
  result.insert(result.end(), std::begin(kUserTimestampMagic),
                std::end(kUserTimestampMagic));

  return result;
}

std::optional<FrameMetadata> UserTimestampTransformer::ExtractTrailer(
    rtc::ArrayView<const uint8_t> data,
    std::vector<uint8_t>& out_data) {
  if (data.size() < kUserTimestampTrailerSize) {
    out_data.assign(data.begin(), data.end());
    return std::nullopt;
  }

  // Check for magic bytes at the end
  const uint8_t* magic_start = data.data() + data.size() - 4;
  if (std::memcmp(magic_start, kUserTimestampMagic, 4) != 0) {
    out_data.assign(data.begin(), data.end());
    return std::nullopt;
  }

  const uint8_t* trailer_start =
      data.data() + data.size() - kUserTimestampTrailerSize;

  // Extract user_timestamp_us (big-endian, 8 bytes)
  int64_t timestamp = 0;
  for (int i = 0; i < 8; ++i) {
    timestamp = (timestamp << 8) | trailer_start[i];
  }

  // Extract frame_id (big-endian, 4 bytes)
  uint32_t frame_id = 0;
  for (int i = 0; i < 4; ++i) {
    frame_id = (frame_id << 8) | trailer_start[8 + i];
  }

  // Copy data without trailer
  out_data.assign(data.begin(),
                  data.end() - kUserTimestampTrailerSize);

  return FrameMetadata{timestamp, frame_id};
}

void UserTimestampTransformer::RegisterTransformedFrameCallback(
    rtc::scoped_refptr<webrtc::TransformedFrameCallback> callback) {
  webrtc::MutexLock lock(&mutex_);
  callback_ = callback;
}

void UserTimestampTransformer::RegisterTransformedFrameSinkCallback(
    rtc::scoped_refptr<webrtc::TransformedFrameCallback> callback,
    uint32_t ssrc) {
  webrtc::MutexLock lock(&mutex_);
  sink_callbacks_[ssrc] = callback;
}

void UserTimestampTransformer::UnregisterTransformedFrameCallback() {
  webrtc::MutexLock lock(&mutex_);
  callback_ = nullptr;
}

void UserTimestampTransformer::UnregisterTransformedFrameSinkCallback(
    uint32_t ssrc) {
  webrtc::MutexLock lock(&mutex_);
  sink_callbacks_.erase(ssrc);
}

void UserTimestampTransformer::set_enabled(bool enabled) {
  enabled_.store(enabled);
}

bool UserTimestampTransformer::enabled() const {
  return enabled_.load();
}

std::optional<FrameMetadata> UserTimestampTransformer::lookup_frame_metadata(
    uint32_t rtp_timestamp) {
  webrtc::MutexLock lock(&recv_map_mutex_);
  auto it = recv_map_.find(rtp_timestamp);
  if (it == recv_map_.end()) {
    return std::nullopt;
  }
  FrameMetadata meta = it->second;
  recv_map_.erase(it);
  // Remove from insertion-order tracker (linear scan is fine for bounded size)
  for (auto oit = recv_map_order_.begin(); oit != recv_map_order_.end(); ++oit) {
    if (*oit == rtp_timestamp) {
      recv_map_order_.erase(oit);
      break;
    }
  }
  return meta;
}

void UserTimestampTransformer::store_frame_metadata(
    int64_t capture_timestamp_us,
    int64_t user_timestamp_us,
    uint32_t frame_id) {
  // Truncate to millisecond precision to match what WebRTC stores
  // internally.  The encoder pipeline converts the VideoFrame's
  // timestamp_us to capture_time_ms_ = timestamp_us / 1000, and
  // CaptureTime() returns Timestamp::Millis(capture_time_ms_).
  // When we call capture_time->us() in TransformSend we get a value
  // with the bottom 3 digits zeroed, so we must store with the same
  // truncation to ensure the lookup succeeds.
  //
  // The caller (VideoTrackSource::on_captured_frame) passes the
  // TimestampAligner-adjusted timestamp here, which is the same
  // value that becomes CaptureTime() in the encoder pipeline.
  int64_t key = (capture_timestamp_us / 1000) * 1000;

  webrtc::MutexLock lock(&send_map_mutex_);

  // Evict oldest entries if at capacity
  while (send_map_.size() >= kMaxSendMapEntries && !send_map_order_.empty()) {
    send_map_.erase(send_map_order_.front());
    send_map_order_.pop_front();
  }

  if (send_map_.find(key) == send_map_.end()) {
    send_map_order_.push_back(key);
  }
  send_map_[key] = FrameMetadata{user_timestamp_us, frame_id};
}

// UserTimestampHandler implementation

UserTimestampHandler::UserTimestampHandler(
    std::shared_ptr<RtcRuntime> rtc_runtime,
    rtc::scoped_refptr<webrtc::RtpSenderInterface> sender)
    : rtc_runtime_(rtc_runtime), sender_(sender) {
  transformer_ = rtc::make_ref_counted<UserTimestampTransformer>(
      UserTimestampTransformer::Direction::kSend);
  sender->SetEncoderToPacketizerFrameTransformer(transformer_);
}

UserTimestampHandler::UserTimestampHandler(
    std::shared_ptr<RtcRuntime> rtc_runtime,
    rtc::scoped_refptr<webrtc::RtpReceiverInterface> receiver)
    : rtc_runtime_(rtc_runtime), receiver_(receiver) {
  transformer_ = rtc::make_ref_counted<UserTimestampTransformer>(
      UserTimestampTransformer::Direction::kReceive);
  receiver->SetDepacketizerToDecoderFrameTransformer(transformer_);
}

void UserTimestampHandler::set_enabled(bool enabled) const {
  transformer_->set_enabled(enabled);
}

bool UserTimestampHandler::enabled() const {
  return transformer_->enabled();
}

int64_t UserTimestampHandler::lookup_user_timestamp(uint32_t rtp_timestamp) const {
  auto meta = transformer_->lookup_frame_metadata(rtp_timestamp);
  if (meta.has_value()) {
    last_frame_id_ = meta->frame_id;
    return meta->user_timestamp_us;
  }
  return -1;
}

uint32_t UserTimestampHandler::last_lookup_frame_id() const {
  return last_frame_id_;
}

void UserTimestampHandler::store_frame_metadata(
    int64_t capture_timestamp_us,
    int64_t user_timestamp_us,
    uint32_t frame_id) const {
  transformer_->store_frame_metadata(capture_timestamp_us, user_timestamp_us, frame_id);
}

rtc::scoped_refptr<UserTimestampTransformer> UserTimestampHandler::transformer() const {
  return transformer_;
}

// Factory functions

std::shared_ptr<UserTimestampHandler> new_user_timestamp_sender(
    std::shared_ptr<PeerConnectionFactory> peer_factory,
    std::shared_ptr<RtpSender> sender) {
  return std::make_shared<UserTimestampHandler>(
      peer_factory->rtc_runtime(), sender->rtc_sender());
}

std::shared_ptr<UserTimestampHandler> new_user_timestamp_receiver(
    std::shared_ptr<PeerConnectionFactory> peer_factory,
    std::shared_ptr<RtpReceiver> receiver) {
  return std::make_shared<UserTimestampHandler>(
      peer_factory->rtc_runtime(), receiver->rtc_receiver());
}

}  // namespace livekit_ffi
