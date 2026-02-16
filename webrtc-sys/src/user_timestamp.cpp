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
#include <algorithm>
#include <optional>
#include <chrono>

#include "api/make_ref_counted.h"
#include "livekit/peer_connection_factory.h"
#include "livekit/rtp_receiver.h"
#include "livekit/rtp_sender.h"
#include "rtc_base/logging.h"
#include "webrtc-sys/src/user_timestamp.rs.h"

namespace livekit_ffi {

// UserTimestampTransformer implementation

UserTimestampTransformer::UserTimestampTransformer(Direction direction)
    : direction_(direction) {
  RTC_LOG(LS_INFO) << "UserTimestampTransformer created direction="
                   << (direction_ == Direction::kSend ? "send" : "recv");
}

void UserTimestampTransformer::Transform(
    std::unique_ptr<webrtc::TransformableFrameInterface> frame) {
  uint32_t ssrc = frame->GetSsrc();
  uint32_t rtp_timestamp = frame->GetTimestamp();

  if (!enabled_.load()) {
    // Pass through without modification, but still log basic info so we know
    // frames are flowing through the transformer.
    RTC_LOG(LS_INFO) << "UserTimestampTransformer::Transform (disabled)"
                     << " direction="
                     << (direction_ == Direction::kSend ? "send" : "recv")
                     << " ssrc=" << ssrc << " rtp_ts=" << rtp_timestamp;

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

  // Look up the user timestamp by the frame's capture time.
  // CaptureTime() returns Timestamp::Millis(capture_time_ms_) where
  // capture_time_ms_ = timestamp_us / 1000.  So capture_time->us()
  // has millisecond precision (bottom 3 digits always zero).
  // store_user_timestamp() truncates its key the same way.
  int64_t ts_to_embed = 0;
  auto capture_time = frame->CaptureTime();
  if (capture_time.has_value()) {
    int64_t capture_us = capture_time->us();

    webrtc::MutexLock lock(&send_map_mutex_);
    auto it = send_map_.find(capture_us);
    if (it != send_map_.end()) {
      ts_to_embed = it->second;
      // Don't erase — simulcast layers share the same capture time.
      // Entries are pruned by capacity in store_user_timestamp().
    }
  } else {
    RTC_LOG(LS_WARNING)
        << "UserTimestampTransformer::TransformSend CaptureTime() not available"
        << " ssrc=" << ssrc << " rtp_ts=" << rtp_timestamp;
  }

  // Always append trailer when enabled (even if timestamp is 0,
  // which indicates no user timestamp was set for this frame)
  std::vector<uint8_t> new_data;
  if (enabled_.load()) {
    new_data = AppendTimestampTrailer(data, ts_to_embed);
    frame->SetData(rtc::ArrayView<const uint8_t>(new_data));

    RTC_LOG(LS_INFO) << "UserTimestampTransformer::TransformSend appended "
                        "trailer"
                     << " ts_us=" << ts_to_embed
                     << " rtp_ts=" << rtp_timestamp
                     << " ssrc=" << ssrc
                     << " capture_us="
                     << (capture_time.has_value() ? capture_time->us() : -1)
                     << " orig_size=" << data.size()
                     << " new_size=" << new_data.size();
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

  auto user_ts = ExtractTimestampTrailer(data, stripped_data);

  if (user_ts.has_value()) {
    // Compute latency from embedded user timestamp to RTP receive
    // time (both in microseconds since Unix epoch), so we can compare
    // this with the latency logged after decode on the subscriber side.
    int64_t now_us =
        std::chrono::duration_cast<std::chrono::microseconds>(
            std::chrono::system_clock::now().time_since_epoch())
            .count();
    double recv_latency_ms =
        static_cast<double>(now_us - user_ts.value()) / 1000.0;

    // Store the extracted timestamp for later retrieval (legacy atomic)
    last_user_timestamp_.store(user_ts.value());
    has_last_user_timestamp_.store(true);

    // Store in the receive map keyed by RTP timestamp so decoded frames
    // can look up their user timestamp regardless of frame drops.
    {
      webrtc::MutexLock lock(&recv_map_mutex_);
      // Evict oldest entry if at capacity
      while (recv_map_.size() >= kMaxRecvMapEntries && !recv_map_order_.empty()) {
        recv_map_.erase(recv_map_order_.front());
        recv_map_order_.pop_front();
      }
      recv_map_[rtp_timestamp] = user_ts.value();
      recv_map_order_.push_back(rtp_timestamp);
    }

    // Update frame with stripped data
    frame->SetData(rtc::ArrayView<const uint8_t>(stripped_data));

    RTC_LOG(LS_INFO) << "UserTimestampTransformer"
                     << " user_ts=" << user_ts.value()
                     << " rtp_ts=" << frame->GetTimestamp()
                     << " recv_latency=" << recv_latency_ms << " ms";
  } else {
    // Log the last few bytes so we can see whether the magic marker is present.
    size_t log_len = std::min<size_t>(data.size(), 16);
    std::string tail_bytes;
    tail_bytes.reserve(log_len * 4);
    for (size_t i = data.size() - log_len; i < data.size(); ++i) {
      char buf[8];
      std::snprintf(buf, sizeof(buf), "%u",
                    static_cast<unsigned>(data[i]));
      if (!tail_bytes.empty()) {
        tail_bytes.append(",");
      }
      tail_bytes.append(buf);
    }

    RTC_LOG(LS_INFO)
        << "UserTimestampTransformer::TransformReceive no trailer found"
        << " ssrc=" << ssrc << " rtp_ts=" << rtp_timestamp
        << " size=" << data.size()
        << " tail_bytes_dec=[" << tail_bytes << "]";
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

std::vector<uint8_t> UserTimestampTransformer::AppendTimestampTrailer(
    rtc::ArrayView<const uint8_t> data,
    int64_t user_timestamp_us) {
  std::vector<uint8_t> result;
  result.reserve(data.size() + kUserTimestampTrailerSize);

  // Copy original data
  result.insert(result.end(), data.begin(), data.end());

  // Append timestamp (big-endian)
  for (int i = 7; i >= 0; --i) {
    result.push_back(
        static_cast<uint8_t>((user_timestamp_us >> (i * 8)) & 0xFF));
  }

  // Append magic bytes
  result.insert(result.end(), std::begin(kUserTimestampMagic),
                std::end(kUserTimestampMagic));

  return result;
}

std::optional<int64_t> UserTimestampTransformer::ExtractTimestampTrailer(
    rtc::ArrayView<const uint8_t> data,
    std::vector<uint8_t>& out_data) {
  if (data.size() < kUserTimestampTrailerSize) {
    RTC_LOG(LS_INFO)
        << "UserTimestampTransformer::ExtractTimestampTrailer data too small"
        << " size=" << data.size()
        << " required=" << kUserTimestampTrailerSize;
    out_data.assign(data.begin(), data.end());
    return std::nullopt;
  }

  // Check for magic bytes at the end
  const uint8_t* magic_start = data.data() + data.size() - 4;
  if (std::memcmp(magic_start, kUserTimestampMagic, 4) != 0) {
    RTC_LOG(LS_INFO)
        << "UserTimestampTransformer::ExtractTimestampTrailer magic mismatch"
        << " size=" << data.size()
        << " magic_bytes_dec=["
        << static_cast<unsigned>(magic_start[0]) << ","
        << static_cast<unsigned>(magic_start[1]) << ","
        << static_cast<unsigned>(magic_start[2]) << ","
        << static_cast<unsigned>(magic_start[3]) << "]";
    out_data.assign(data.begin(), data.end());
    return std::nullopt;
  }

  // Extract timestamp (big-endian)
  const uint8_t* ts_start =
      data.data() + data.size() - kUserTimestampTrailerSize;
  int64_t timestamp = 0;
  for (int i = 0; i < 8; ++i) {
    timestamp = (timestamp << 8) | ts_start[i];
  }

  // Copy data without trailer
  out_data.assign(data.begin(),
                  data.end() - kUserTimestampTrailerSize);

  return timestamp;
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

std::optional<int64_t> UserTimestampTransformer::last_user_timestamp()
    const {
  if (!has_last_user_timestamp_.load()) {
    return std::nullopt;
  }
  return last_user_timestamp_.load();
}

std::optional<int64_t> UserTimestampTransformer::lookup_user_timestamp(
    uint32_t rtp_timestamp) {
  webrtc::MutexLock lock(&recv_map_mutex_);
  auto it = recv_map_.find(rtp_timestamp);
  if (it == recv_map_.end()) {
    return std::nullopt;
  }
  int64_t ts = it->second;
  recv_map_.erase(it);
  // Remove from insertion-order tracker (linear scan is fine for bounded size)
  for (auto oit = recv_map_order_.begin(); oit != recv_map_order_.end(); ++oit) {
    if (*oit == rtp_timestamp) {
      recv_map_order_.erase(oit);
      break;
    }
  }
  return ts;
}

void UserTimestampTransformer::store_user_timestamp(
    int64_t capture_timestamp_us,
    int64_t user_timestamp_us) {
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

  send_map_[key] = user_timestamp_us;
  send_map_order_.push_back(key);

  RTC_LOG(LS_INFO) << "UserTimestampTransformer::store_user_timestamp"
                   << " capture_ts_us=" << capture_timestamp_us
                   << " key_us=" << key
                   << " user_ts_us=" << user_timestamp_us
                   << " size=" << send_map_.size();
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

int64_t UserTimestampHandler::last_user_timestamp() const {
  auto ts = transformer_->last_user_timestamp();
  return ts.value_or(-1);
}

int64_t UserTimestampHandler::lookup_user_timestamp(uint32_t rtp_timestamp) const {
  auto ts = transformer_->lookup_user_timestamp(rtp_timestamp);
  return ts.value_or(-1);
}

bool UserTimestampHandler::has_user_timestamp() const {
  return transformer_->last_user_timestamp().has_value();
}

void UserTimestampHandler::store_user_timestamp(
    int64_t capture_timestamp_us,
    int64_t user_timestamp_us) const {
  transformer_->store_user_timestamp(capture_timestamp_us, user_timestamp_us);
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
