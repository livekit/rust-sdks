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

#include <chrono>
#include <cstdio>
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
  FrameMetadata meta_to_embed{0, 0, 0};
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

  // Track per-SSRC encoding delay for simulcast diagnostics.
  {
    auto now_us = std::chrono::duration_cast<std::chrono::microseconds>(
                      std::chrono::system_clock::now().time_since_epoch())
                      .count();
    webrtc::MutexLock lock(&send_map_mutex_);
    auto& stats = send_ssrc_stats_[ssrc];
    stats.frame_count++;
    if (meta_to_embed.user_timestamp_us > 0) {
      int64_t delay_us = now_us - meta_to_embed.user_timestamp_us;
      stats.sum_encode_delay_us += delay_us;
      stats.encode_delay_samples++;
      if ((stats.frame_count % 60) == 1) {
        double avg_ms = stats.encode_delay_samples > 0
                            ? (stats.sum_encode_delay_us /
                               (double)stats.encode_delay_samples / 1000.0)
                            : 0.0;
        fprintf(stderr,
                "[UserTS-Send] ssrc=%u frames=%llu fid=%u "
                "encode_delay=%.1fms (cur=%.1fms) user_ts=%lld\n",
                ssrc, (unsigned long long)stats.frame_count,
                meta_to_embed.frame_id, avg_ms, delay_us / 1000.0,
                (long long)meta_to_embed.user_timestamp_us);
        stats.sum_encode_delay_us = 0;
        stats.encode_delay_samples = 0;
      }
    }
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
    meta->ssrc = ssrc;

    {
      webrtc::MutexLock lock(&recv_map_mutex_);

      recv_frame_count_++;

      // Detect simulcast layer switch (SSRC change).
      // When the SFU switches us to a different layer, the old layer's
      // entries are stale and can cause RTP timestamp collisions or
      // return wrong user timestamps on lookup.  Flush them.
      if (recv_active_ssrc_ != 0 && recv_active_ssrc_ != ssrc) {
        size_t flushed = 0;
        auto oit = recv_map_order_.begin();
        while (oit != recv_map_order_.end()) {
          auto mit = recv_map_.find(*oit);
          if (mit != recv_map_.end() && mit->second.ssrc != ssrc) {
            recv_map_.erase(mit);
            oit = recv_map_order_.erase(oit);
            flushed++;
          } else {
            ++oit;
          }
        }
        fprintf(stderr,
                "[UserTS-Recv] SSRC_SWITCH old=%u new=%u flushed=%zu "
                "remaining=%zu frame_count=%llu\n",
                recv_active_ssrc_, ssrc, flushed, recv_map_.size(),
                (unsigned long long)recv_frame_count_);
      }
      recv_active_ssrc_ = ssrc;

      bool collision = recv_map_.find(rtp_timestamp) != recv_map_.end();
      if (collision) {
        auto& existing = recv_map_[rtp_timestamp];
        fprintf(stderr,
                "[UserTS-Recv] COLLISION rtp_ts=%u ssrc=%u "
                "existing: ts=%lld fid=%u ssrc=%u  "
                "new: ts=%lld fid=%u ssrc=%u\n",
                rtp_timestamp, ssrc,
                (long long)existing.user_timestamp_us, existing.frame_id,
                existing.ssrc,
                (long long)meta->user_timestamp_us, meta->frame_id,
                meta->ssrc);
      }

      // Check for timestamp regression (non-monotonic user timestamps
      // indicate stale data or clock issues).
      if (recv_last_user_ts_ > 0 &&
          meta->user_timestamp_us < recv_last_user_ts_ &&
          meta->user_timestamp_us > 0) {
        int64_t regression_ms =
            (recv_last_user_ts_ - meta->user_timestamp_us) / 1000;
        fprintf(stderr,
                "[UserTS-Recv] TS_REGRESSION ssrc=%u rtp_ts=%u "
                "prev_ts=%lld new_ts=%lld regression=%lldms fid=%u\n",
                ssrc, rtp_timestamp,
                (long long)recv_last_user_ts_,
                (long long)meta->user_timestamp_us,
                (long long)regression_ms, meta->frame_id);
      }
      if (meta->user_timestamp_us > 0) {
        recv_last_user_ts_ = meta->user_timestamp_us;

        // Measure end-to-end latency per SSRC.
        auto now_us = std::chrono::duration_cast<std::chrono::microseconds>(
                          std::chrono::system_clock::now().time_since_epoch())
                          .count();
        int64_t latency_us = now_us - meta->user_timestamp_us;
        auto& rstats = recv_ssrc_stats_[ssrc];
        rstats.frame_count++;
        rstats.sum_latency_us += latency_us;
        rstats.latency_samples++;
        if (latency_us > rstats.max_latency_us) {
          rstats.max_latency_us = latency_us;
        }
        if ((rstats.frame_count % 60) == 1) {
          double avg_ms = rstats.latency_samples > 0
                              ? (rstats.sum_latency_us /
                                 (double)rstats.latency_samples / 1000.0)
                              : 0.0;
          double max_ms = rstats.max_latency_us / 1000.0;
          fprintf(stderr,
                  "[UserTS-Recv] LATENCY ssrc=%u frames=%llu "
                  "avg=%.1fms max=%.1fms cur=%.1fms fid=%u\n",
                  ssrc, (unsigned long long)rstats.frame_count,
                  avg_ms, max_ms, latency_us / 1000.0, meta->frame_id);
          rstats.sum_latency_us = 0;
          rstats.latency_samples = 0;
          rstats.max_latency_us = 0;
        }
      }

      // Evict oldest entry if at capacity
      while (recv_map_.size() >= kMaxRecvMapEntries &&
             !recv_map_order_.empty()) {
        auto evicted_rtp = recv_map_order_.front();
        recv_map_.erase(evicted_rtp);
        recv_map_order_.pop_front();
      }
      if (!collision) {
        recv_map_order_.push_back(rtp_timestamp);
      }
      recv_map_[rtp_timestamp] = meta.value();
    }

    // Update frame with stripped data
    frame->SetData(rtc::ArrayView<const uint8_t>(stripped_data));
  } else {
    fprintf(stderr,
            "[UserTS-Recv] NO_TRAILER rtp_ts=%u ssrc=%u data_size=%zu\n",
            rtp_timestamp, ssrc, data.size());
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

  // Append user_timestamp_us (big-endian, 8 bytes) XORed with 0xFF to
  // prevent H.264 NAL start code sequences (0x000001 / 0x00000001) from
  // appearing inside the trailer.  The H.264 packetizer scans the full
  // frame payload for start codes, and the trailer's raw bytes can
  // contain 0x000001 (e.g. frame_id 256 = 0x00000100).
  for (int i = 7; i >= 0; --i) {
    result.push_back(
        static_cast<uint8_t>(((user_timestamp_us >> (i * 8)) & 0xFF) ^ 0xFF));
  }

  // Append frame_id (big-endian, 4 bytes), also XORed
  for (int i = 3; i >= 0; --i) {
    result.push_back(
        static_cast<uint8_t>(((frame_id >> (i * 8)) & 0xFF) ^ 0xFF));
  }

  // Append magic bytes (NOT XORed — they must remain recognizable and
  // already contain no 0x00/0x01 bytes)
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

  // Extract user_timestamp_us (big-endian, 8 bytes, XORed with 0xFF)
  int64_t timestamp = 0;
  for (int i = 0; i < 8; ++i) {
    timestamp = (timestamp << 8) | (trailer_start[i] ^ 0xFF);
  }

  // Extract frame_id (big-endian, 4 bytes, XORed with 0xFF)
  uint32_t frame_id = 0;
  for (int i = 0; i < 4; ++i) {
    frame_id = (frame_id << 8) | (trailer_start[8 + i] ^ 0xFF);
  }

  if (timestamp < 946684800000000LL || timestamp > 4102444800000000LL) {
    std::string hex;
    for (size_t i = 0; i < kUserTimestampTrailerSize; ++i) {
      char buf[4];
      snprintf(buf, sizeof(buf), "%02x", trailer_start[i]);
      hex += buf;
      if (i == 7 || i == 11) hex += "|";
    }
    size_t context_bytes = std::min<size_t>(data.size(), kUserTimestampTrailerSize + 8);
    std::string context_hex;
    const uint8_t* ctx_start = data.data() + data.size() - context_bytes;
    for (size_t i = 0; i < context_bytes; ++i) {
      char buf[4];
      snprintf(buf, sizeof(buf), "%02x", ctx_start[i]);
      context_hex += buf;
      if (&ctx_start[i] == trailer_start - 1) context_hex += "|";
      else if (&ctx_start[i] == trailer_start + 7) context_hex += "|";
      else if (&ctx_start[i] == trailer_start + 11) context_hex += "|";
    }
    fprintf(stderr,
            "[UserTS-Extract] BAD ts=%lld fid=%u data_size=%zu "
            "trailer=%s context=%s\n",
            (long long)timestamp, frame_id, data.size(),
            hex.c_str(), context_hex.c_str());
  }

  // Copy data without trailer
  out_data.assign(data.begin(),
                  data.end() - kUserTimestampTrailerSize);

  return FrameMetadata{timestamp, frame_id, 0};
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
    recv_lookup_misses_++;
    if ((recv_lookup_misses_ % 30) == 1) {
      fprintf(stderr,
              "[UserTS-Lookup] MISS rtp_ts=%u map=%zu "
              "hits=%llu misses=%llu active_ssrc=%u\n",
              rtp_timestamp, recv_map_.size(),
              (unsigned long long)recv_lookup_hits_,
              (unsigned long long)recv_lookup_misses_,
              recv_active_ssrc_);
    }
    return std::nullopt;
  }
  recv_lookup_hits_++;
  FrameMetadata meta = it->second;
  recv_map_.erase(it);
  for (auto oit = recv_map_order_.begin(); oit != recv_map_order_.end();
       ++oit) {
    if (*oit == rtp_timestamp) {
      recv_map_order_.erase(oit);
      break;
    }
  }
  if (meta.user_timestamp_us < 946684800000000LL ||
      meta.user_timestamp_us > 4102444800000000LL) {
    fprintf(stderr,
            "[UserTS-Lookup] BAD rtp_ts=%u ts=%lld fid=%u ssrc=%u map=%zu\n",
            rtp_timestamp, (long long)meta.user_timestamp_us,
            meta.frame_id, meta.ssrc, recv_map_.size());
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
  send_map_[key] = FrameMetadata{user_timestamp_us, frame_id, 0};
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
