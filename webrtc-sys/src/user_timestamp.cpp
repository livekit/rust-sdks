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
#include "rtc_base/logging.h"
#include "webrtc-sys/src/user_timestamp.rs.h"

namespace livekit_ffi {

// UserTimestampStore implementation

void UserTimestampStore::store(int64_t capture_timestamp_us,
                               int64_t user_timestamp_us) const {
  webrtc::MutexLock lock(&mutex_);

  // Remove old entries if we're at capacity
  while (entries_.size() >= kMaxEntries) {
    entries_.pop_front();
  }

  entries_.push_back({capture_timestamp_us, user_timestamp_us});
  RTC_LOG(LS_INFO) << "UserTimestampStore::store capture_ts_us="
                   << capture_timestamp_us
                   << " user_ts_us=" << user_timestamp_us
                   << " size=" << entries_.size();
}

int64_t UserTimestampStore::lookup(int64_t capture_timestamp_us) const {
  webrtc::MutexLock lock(&mutex_);

  // Search from the end (most recent) for better performance
  for (auto it = entries_.rbegin(); it != entries_.rend(); ++it) {
    if (it->capture_timestamp_us == capture_timestamp_us) {
      return it->user_timestamp_us;
    }
  }

  return -1;
}

int64_t UserTimestampStore::pop() const {
  webrtc::MutexLock lock(&mutex_);

  if (entries_.empty()) {
    RTC_LOG(LS_INFO) << "UserTimestampStore::pop empty";
    return -1;
  }

  int64_t user_ts = entries_.front().user_timestamp_us;
  entries_.pop_front();
  RTC_LOG(LS_INFO) << "UserTimestampStore::pop user_ts_us=" << user_ts
                   << " remaining=" << entries_.size();
  return user_ts;
}

int64_t UserTimestampStore::peek() const {
  webrtc::MutexLock lock(&mutex_);

  if (entries_.empty()) {
    return -1;
  }

  return entries_.front().user_timestamp_us;
}

void UserTimestampStore::prune(int64_t max_age_us) const {
  webrtc::MutexLock lock(&mutex_);

  if (entries_.empty()) {
    return;
  }

  int64_t newest_timestamp = entries_.back().capture_timestamp_us;
  int64_t threshold = newest_timestamp - max_age_us;

  while (!entries_.empty() &&
         entries_.front().capture_timestamp_us < threshold) {
    entries_.pop_front();
  }
}

// UserTimestampTransformer implementation

UserTimestampTransformer::UserTimestampTransformer(
    Direction direction,
    std::shared_ptr<UserTimestampStore> store)
    : direction_(direction), store_(store) {
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
  // Get the RTP timestamp from the frame for logging
  uint32_t rtp_timestamp = frame->GetTimestamp();
  uint32_t ssrc = frame->GetSsrc();

  auto data = frame->GetData();

  // Drain all queued user timestamps and use the most recent one.
  // The encoder may skip captured frames (rate control, CPU), so the
  // store can accumulate faster than TransformSend is called.  Draining
  // ensures we always embed the timestamp closest to the frame actually
  // being encoded.  With simulcast, multiple layers encode the same
  // captured frame — subsequent layers will find the queue empty and
  // fall back to the cached value.
  int64_t ts_to_embed = 0;

  if (store_) {
    int64_t newest_ts = -1;
    // Drain: pop all available entries, keep the last one
    for (;;) {
      int64_t popped_ts = store_->pop();
      if (popped_ts < 0) break;
      newest_ts = popped_ts;
    }

    if (newest_ts >= 0) {
      ts_to_embed = newest_ts;
      // Cache for simulcast layers that encode the same frame
      webrtc::MutexLock lock(&send_cache_mutex_);
      last_sent_user_timestamp_ = newest_ts;
    } else {
      // Queue was empty — use cached value (simulcast or encoder
      // encoding the same frame as a previous layer)
      webrtc::MutexLock lock(&send_cache_mutex_);
      ts_to_embed = last_sent_user_timestamp_;
    }
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

    // Also push to the receive queue so decoded frames can pop 1:1
    {
      webrtc::MutexLock lock(&recv_queue_mutex_);
      if (recv_queue_.size() >= kMaxRecvQueueEntries) {
        recv_queue_.pop_front();
      }
      recv_queue_.push_back(user_ts.value());
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

std::optional<int64_t> UserTimestampTransformer::pop_user_timestamp() {
  webrtc::MutexLock lock(&recv_queue_mutex_);
  if (recv_queue_.empty()) {
    return std::nullopt;
  }
  int64_t ts = recv_queue_.front();
  recv_queue_.pop_front();
  return ts;
}

// UserTimestampHandler implementation

UserTimestampHandler::UserTimestampHandler(
    std::shared_ptr<RtcRuntime> rtc_runtime,
    std::shared_ptr<UserTimestampStore> store,
    rtc::scoped_refptr<webrtc::RtpSenderInterface> sender)
    : rtc_runtime_(rtc_runtime), sender_(sender) {
  transformer_ = rtc::make_ref_counted<UserTimestampTransformer>(
      UserTimestampTransformer::Direction::kSend, store);
  sender->SetEncoderToPacketizerFrameTransformer(transformer_);
}

UserTimestampHandler::UserTimestampHandler(
    std::shared_ptr<RtcRuntime> rtc_runtime,
    std::shared_ptr<UserTimestampStore> store,
    rtc::scoped_refptr<webrtc::RtpReceiverInterface> receiver)
    : rtc_runtime_(rtc_runtime), receiver_(receiver) {
  transformer_ = rtc::make_ref_counted<UserTimestampTransformer>(
      UserTimestampTransformer::Direction::kReceive, store);
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

int64_t UserTimestampHandler::pop_user_timestamp() const {
  auto ts = transformer_->pop_user_timestamp();
  return ts.value_or(-1);
}

bool UserTimestampHandler::has_user_timestamp() const {
  return transformer_->last_user_timestamp().has_value();
}

rtc::scoped_refptr<UserTimestampTransformer> UserTimestampHandler::transformer() const {
  return transformer_;
}

// Factory functions

std::shared_ptr<UserTimestampStore> new_user_timestamp_store() {
  return std::make_shared<UserTimestampStore>();
}

std::shared_ptr<UserTimestampHandler> new_user_timestamp_sender(
    std::shared_ptr<PeerConnectionFactory> peer_factory,
    std::shared_ptr<UserTimestampStore> store,
    std::shared_ptr<RtpSender> sender) {
  return std::make_shared<UserTimestampHandler>(
      peer_factory->rtc_runtime(), store, sender->rtc_sender());
}

std::shared_ptr<UserTimestampHandler> new_user_timestamp_receiver(
    std::shared_ptr<PeerConnectionFactory> peer_factory,
    std::shared_ptr<UserTimestampStore> store,
    std::shared_ptr<RtpReceiver> receiver) {
  return std::make_shared<UserTimestampHandler>(
      peer_factory->rtc_runtime(), store, receiver->rtc_receiver());
}

}  // namespace livekit_ffi
