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

#include "livekit/sensor_timestamp.h"

#include <cstring>
#include <algorithm>
#include <optional>

#include "api/make_ref_counted.h"
#include "livekit/peer_connection_factory.h"
#include "rtc_base/logging.h"
#include "webrtc-sys/src/sensor_timestamp.rs.h"

namespace livekit {

// SensorTimestampStore implementation

void SensorTimestampStore::store(int64_t capture_timestamp_us,
                                  int64_t sensor_timestamp_us) const {
  webrtc::MutexLock lock(&mutex_);

  // Remove old entries if we're at capacity
  while (entries_.size() >= kMaxEntries) {
    entries_.pop_front();
  }

  entries_.push_back({capture_timestamp_us, sensor_timestamp_us});
  RTC_LOG(LS_INFO) << "SensorTimestampStore::store capture_ts_us=" << capture_timestamp_us
                   << " sensor_ts_us=" << sensor_timestamp_us
                   << " size=" << entries_.size();
}

int64_t SensorTimestampStore::lookup(int64_t capture_timestamp_us) const {
  webrtc::MutexLock lock(&mutex_);
  
  // Search from the end (most recent) for better performance
  for (auto it = entries_.rbegin(); it != entries_.rend(); ++it) {
    if (it->capture_timestamp_us == capture_timestamp_us) {
      return it->sensor_timestamp_us;
    }
  }
  
  return -1;
}

int64_t SensorTimestampStore::pop() const {
  webrtc::MutexLock lock(&mutex_);

  if (entries_.empty()) {
    RTC_LOG(LS_INFO) << "SensorTimestampStore::pop empty";
    return -1;
  }

  int64_t sensor_ts = entries_.front().sensor_timestamp_us;
  entries_.pop_front();
  RTC_LOG(LS_INFO) << "SensorTimestampStore::pop sensor_ts_us=" << sensor_ts
                   << " remaining=" << entries_.size();
  return sensor_ts;
}

int64_t SensorTimestampStore::peek() const {
  webrtc::MutexLock lock(&mutex_);
  
  if (entries_.empty()) {
    return -1;
  }
  
  return entries_.front().sensor_timestamp_us;
}

void SensorTimestampStore::prune(int64_t max_age_us) const {
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

// SensorTimestampTransformer implementation

SensorTimestampTransformer::SensorTimestampTransformer(
    Direction direction,
    std::shared_ptr<SensorTimestampStore> store)
    : direction_(direction), store_(store) {
  RTC_LOG(LS_INFO) << "SensorTimestampTransformer created direction="
                   << (direction_ == Direction::kSend ? "send" : "recv");
}

void SensorTimestampTransformer::Transform(
    std::unique_ptr<webrtc::TransformableFrameInterface> frame) {
  uint32_t ssrc = frame->GetSsrc();
  uint32_t rtp_timestamp = frame->GetTimestamp();

  if (!enabled_.load()) {
    // Pass through without modification, but still log basic info so we know
    // frames are flowing through the transformer.
    RTC_LOG(LS_INFO) << "SensorTimestampTransformer::Transform (disabled)"
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
          << "SensorTimestampTransformer::Transform (disabled) has no callback"
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

void SensorTimestampTransformer::TransformSend(
    std::unique_ptr<webrtc::TransformableFrameInterface> frame) {
  // Get the RTP timestamp from the frame for logging
  uint32_t rtp_timestamp = frame->GetTimestamp();
  uint32_t ssrc = frame->GetSsrc();

  auto data = frame->GetData();

  // Pop the next sensor timestamp from the queue.
  // This assumes frames are captured and encoded in order (FIFO).
  int64_t ts_to_embed = 0;

  if (store_) {
    int64_t popped_ts = store_->pop();
    if (popped_ts >= 0) {
      ts_to_embed = popped_ts;
    } else {
      RTC_LOG(LS_INFO) << "SensorTimestampTransformer::TransformSend no sensor timestamp available"
                       << " rtp_ts=" << rtp_timestamp << " orig_size=" << data.size();
    }
  }

  // Always append trailer when enabled (even if timestamp is 0,
  // which indicates no sensor timestamp was set for this frame)
  std::vector<uint8_t> new_data;
  if (enabled_.load()) {
    new_data = AppendTimestampTrailer(data, ts_to_embed);
    frame->SetData(rtc::ArrayView<const uint8_t>(new_data));

    RTC_LOG(LS_INFO) << "SensorTimestampTransformer::TransformSend appended trailer"
                     << " ts_us=" << ts_to_embed << " rtp_ts=" << rtp_timestamp
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
        << "SensorTimestampTransformer::TransformSend has no callback"
        << " ssrc=" << ssrc << " rtp_ts=" << rtp_timestamp;
  }
}

void SensorTimestampTransformer::TransformReceive(
    std::unique_ptr<webrtc::TransformableFrameInterface> frame) {
  uint32_t ssrc = frame->GetSsrc();
  uint32_t rtp_timestamp = frame->GetTimestamp();
  auto data = frame->GetData();
  std::vector<uint8_t> stripped_data;

  RTC_LOG(LS_INFO) << "SensorTimestampTransformer::TransformReceive begin"
                   << " ssrc=" << ssrc << " rtp_ts=" << rtp_timestamp
                   << " size=" << data.size();

  auto sensor_ts = ExtractTimestampTrailer(data, stripped_data);

  if (sensor_ts.has_value()) {
    // Store the extracted timestamp for later retrieval
    last_sensor_timestamp_.store(sensor_ts.value());
    has_last_sensor_timestamp_.store(true);

    // Update frame with stripped data
    frame->SetData(rtc::ArrayView<const uint8_t>(stripped_data));

    RTC_LOG(LS_INFO) << "SensorTimestampTransformer::TransformReceive extracted trailer"
                     << " ts_us=" << sensor_ts.value()
                     << " rtp_ts=" << frame->GetTimestamp()
                     << " ssrc=" << ssrc
                     << " stripped_size=" << stripped_data.size()
                     << " orig_size=" << data.size();
  } else {
    // Log the last few bytes so we can see whether the magic marker is present.
    size_t log_len = std::min<size_t>(data.size(), 16);
    std::string tail_bytes;
    tail_bytes.reserve(log_len * 4);
    for (size_t i = data.size() - log_len; i < data.size(); ++i) {
      char buf[8];
      std::snprintf(buf, sizeof(buf), "%u", static_cast<unsigned>(data[i]));
      if (!tail_bytes.empty()) {
        tail_bytes.append(",");
      }
      tail_bytes.append(buf);
    }

    RTC_LOG(LS_INFO)
        << "SensorTimestampTransformer::TransformReceive no trailer found"
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
        << "SensorTimestampTransformer::TransformReceive has no callback"
        << " ssrc=" << ssrc << " rtp_ts=" << rtp_timestamp;
  }
}

std::vector<uint8_t> SensorTimestampTransformer::AppendTimestampTrailer(
    rtc::ArrayView<const uint8_t> data,
    int64_t sensor_timestamp_us) {
  std::vector<uint8_t> result;
  result.reserve(data.size() + kSensorTimestampTrailerSize);
  
  // Copy original data
  result.insert(result.end(), data.begin(), data.end());
  
  // Append timestamp (big-endian)
  for (int i = 7; i >= 0; --i) {
    result.push_back(static_cast<uint8_t>((sensor_timestamp_us >> (i * 8)) & 0xFF));
  }
  
  // Append magic bytes
  result.insert(result.end(), std::begin(kSensorTimestampMagic),
                std::end(kSensorTimestampMagic));
  
  return result;
}

std::optional<int64_t> SensorTimestampTransformer::ExtractTimestampTrailer(
    rtc::ArrayView<const uint8_t> data,
    std::vector<uint8_t>& out_data) {
  if (data.size() < kSensorTimestampTrailerSize) {
    RTC_LOG(LS_INFO)
        << "SensorTimestampTransformer::ExtractTimestampTrailer data too small"
        << " size=" << data.size()
        << " required=" << kSensorTimestampTrailerSize;
    out_data.assign(data.begin(), data.end());
    return std::nullopt;
  }
  
  // Check for magic bytes at the end
  const uint8_t* magic_start = data.data() + data.size() - 4;
  if (std::memcmp(magic_start, kSensorTimestampMagic, 4) != 0) {
    RTC_LOG(LS_INFO)
        << "SensorTimestampTransformer::ExtractTimestampTrailer magic mismatch"
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
  const uint8_t* ts_start = data.data() + data.size() - kSensorTimestampTrailerSize;
  int64_t timestamp = 0;
  for (int i = 0; i < 8; ++i) {
    timestamp = (timestamp << 8) | ts_start[i];
  }
  
  // Copy data without trailer
  out_data.assign(data.begin(), data.end() - kSensorTimestampTrailerSize);
  
  return timestamp;
}

void SensorTimestampTransformer::RegisterTransformedFrameCallback(
    rtc::scoped_refptr<webrtc::TransformedFrameCallback> callback) {
  webrtc::MutexLock lock(&mutex_);
  callback_ = callback;
}

void SensorTimestampTransformer::RegisterTransformedFrameSinkCallback(
    rtc::scoped_refptr<webrtc::TransformedFrameCallback> callback,
    uint32_t ssrc) {
  webrtc::MutexLock lock(&mutex_);
  sink_callbacks_[ssrc] = callback;
}

void SensorTimestampTransformer::UnregisterTransformedFrameCallback() {
  webrtc::MutexLock lock(&mutex_);
  callback_ = nullptr;
}

void SensorTimestampTransformer::UnregisterTransformedFrameSinkCallback(
    uint32_t ssrc) {
  webrtc::MutexLock lock(&mutex_);
  sink_callbacks_.erase(ssrc);
}

void SensorTimestampTransformer::set_enabled(bool enabled) {
  enabled_.store(enabled);
}

bool SensorTimestampTransformer::enabled() const {
  return enabled_.load();
}

std::optional<int64_t> SensorTimestampTransformer::last_sensor_timestamp()
    const {
  if (!has_last_sensor_timestamp_.load()) {
    return std::nullopt;
  }
  return last_sensor_timestamp_.load();
}

// SensorTimestampHandler implementation

SensorTimestampHandler::SensorTimestampHandler(
    std::shared_ptr<RtcRuntime> rtc_runtime,
    std::shared_ptr<SensorTimestampStore> store,
    rtc::scoped_refptr<webrtc::RtpSenderInterface> sender)
    : rtc_runtime_(rtc_runtime), sender_(sender) {
  transformer_ = rtc::make_ref_counted<SensorTimestampTransformer>(
      SensorTimestampTransformer::Direction::kSend, store);
  sender->SetEncoderToPacketizerFrameTransformer(transformer_);
}

SensorTimestampHandler::SensorTimestampHandler(
    std::shared_ptr<RtcRuntime> rtc_runtime,
    std::shared_ptr<SensorTimestampStore> store,
    rtc::scoped_refptr<webrtc::RtpReceiverInterface> receiver)
    : rtc_runtime_(rtc_runtime), receiver_(receiver) {
  transformer_ = rtc::make_ref_counted<SensorTimestampTransformer>(
      SensorTimestampTransformer::Direction::kReceive, store);
  receiver->SetDepacketizerToDecoderFrameTransformer(transformer_);
}

void SensorTimestampHandler::set_enabled(bool enabled) const {
  transformer_->set_enabled(enabled);
}

bool SensorTimestampHandler::enabled() const {
  return transformer_->enabled();
}

int64_t SensorTimestampHandler::last_sensor_timestamp() const {
  auto ts = transformer_->last_sensor_timestamp();
  return ts.value_or(-1);
}

bool SensorTimestampHandler::has_sensor_timestamp() const {
  return transformer_->last_sensor_timestamp().has_value();
}

// Factory functions

std::shared_ptr<SensorTimestampStore> new_sensor_timestamp_store() {
  return std::make_shared<SensorTimestampStore>();
}

std::shared_ptr<SensorTimestampHandler> new_sensor_timestamp_sender(
    std::shared_ptr<PeerConnectionFactory> peer_factory,
    std::shared_ptr<SensorTimestampStore> store,
    std::shared_ptr<RtpSender> sender) {
  return std::make_shared<SensorTimestampHandler>(
      peer_factory->rtc_runtime(), store, sender->rtc_sender());
}

std::shared_ptr<SensorTimestampHandler> new_sensor_timestamp_receiver(
    std::shared_ptr<PeerConnectionFactory> peer_factory,
    std::shared_ptr<SensorTimestampStore> store,
    std::shared_ptr<RtpReceiver> receiver) {
  return std::make_shared<SensorTimestampHandler>(
      peer_factory->rtc_runtime(), store, receiver->rtc_receiver());
}

}  // namespace livekit

