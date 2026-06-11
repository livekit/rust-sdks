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

#include "livekit/frame_cryptor.h"

#include <deque>
#include <memory>
#include <optional>
#include <unordered_map>

#include "absl/types/optional.h"
#include "api/make_ref_counted.h"
#include "livekit/peer_connection.h"
#include "livekit/peer_connection_factory.h"
#include "livekit/packet_trailer.h"
#include "livekit/packet_trailer_av1.h"
#include "livekit/webrtc.h"
#include "rtc_base/logging.h"
#include "rtc_base/thread.h"
#include "webrtc-sys/src/frame_cryptor.rs.h"

namespace livekit_ffi {

class ChainedFrameTransformer : public webrtc::FrameTransformerInterface,
                                public webrtc::TransformedFrameCallback {
 public:
  ChainedFrameTransformer(
      webrtc::scoped_refptr<webrtc::FrameTransformerInterface> first,
      webrtc::scoped_refptr<webrtc::FrameTransformerInterface> second)
      : first_(std::move(first)), second_(std::move(second)) {}

  void Transform(
      std::unique_ptr<webrtc::TransformableFrameInterface> frame) override {
    first_->Transform(std::move(frame));
  }

  void RegisterTransformedFrameCallback(
      webrtc::scoped_refptr<webrtc::TransformedFrameCallback> callback) override {
    second_->RegisterTransformedFrameCallback(callback);
    first_->RegisterTransformedFrameCallback(
        webrtc::scoped_refptr<webrtc::TransformedFrameCallback>(this));
  }

  void RegisterTransformedFrameSinkCallback(
      webrtc::scoped_refptr<webrtc::TransformedFrameCallback> callback,
      uint32_t ssrc) override {
    second_->RegisterTransformedFrameSinkCallback(callback, ssrc);
    first_->RegisterTransformedFrameSinkCallback(
        webrtc::scoped_refptr<webrtc::TransformedFrameCallback>(this), ssrc);
  }

  void UnregisterTransformedFrameCallback() override {
    first_->UnregisterTransformedFrameCallback();
    second_->UnregisterTransformedFrameCallback();
  }

  void UnregisterTransformedFrameSinkCallback(uint32_t ssrc) override {
    first_->UnregisterTransformedFrameSinkCallback(ssrc);
    second_->UnregisterTransformedFrameSinkCallback(ssrc);
  }

  void OnTransformedFrame(
      std::unique_ptr<webrtc::TransformableFrameInterface> frame) override {
    second_->Transform(std::move(frame));
  }

 private:
  webrtc::scoped_refptr<webrtc::FrameTransformerInterface> first_;
  webrtc::scoped_refptr<webrtc::FrameTransformerInterface> second_;
};

/// Sequence headers captured from plaintext AV1 keyframes before
/// encryption, keyed by (ssrc, rtp timestamp) so the post-encryption wrap
/// stage can attach the frame's own header. Bounded FIFO eviction guards
/// against entries that are never consumed (e.g. frames the cryptor
/// discards while keys are missing).
class Av1SequenceHeaderCache {
 public:
  void Store(uint32_t ssrc, uint32_t rtp_timestamp, std::vector<uint8_t> obu) {
    const uint64_t key = MakeKey(ssrc, rtp_timestamp);
    webrtc::MutexLock lock(&mutex_);
    while (entries_.size() >= kMaxEntries && !order_.empty()) {
      entries_.erase(order_.front());
      order_.pop_front();
    }
    if (entries_.find(key) == entries_.end()) {
      order_.push_back(key);
    }
    entries_[key] = std::move(obu);
  }

  std::optional<std::vector<uint8_t>> Take(uint32_t ssrc,
                                           uint32_t rtp_timestamp) {
    const uint64_t key = MakeKey(ssrc, rtp_timestamp);
    webrtc::MutexLock lock(&mutex_);
    auto it = entries_.find(key);
    if (it == entries_.end()) {
      return std::nullopt;
    }
    std::vector<uint8_t> obu = std::move(it->second);
    entries_.erase(it);
    for (auto oit = order_.begin(); oit != order_.end(); ++oit) {
      if (*oit == key) {
        order_.erase(oit);
        break;
      }
    }
    return obu;
  }

 private:
  static uint64_t MakeKey(uint32_t ssrc, uint32_t rtp_timestamp) {
    return (static_cast<uint64_t>(ssrc) << 32) | rtp_timestamp;
  }

  static constexpr size_t kMaxEntries = 16;
  mutable webrtc::Mutex mutex_;
  std::unordered_map<uint64_t, std::vector<uint8_t>> entries_;
  std::deque<uint64_t> order_;
};

/// Captures the sequence header OBU of plaintext AV1 keyframes on the send
/// side, ahead of the e2ee transform, so the wrap stage downstream can
/// prepend the frame's real header (SFUs parsing it then observe the
/// stream's true parameters). Frames are forwarded untouched.
class Av1SequenceHeaderSniffer : public webrtc::FrameTransformerInterface {
 public:
  Av1SequenceHeaderSniffer(
      std::shared_ptr<Av1SequenceHeaderCache> cache,
      webrtc::scoped_refptr<webrtc::FrameCryptorTransformer> e2ee_transformer)
      : cache_(std::move(cache)),
        e2ee_transformer_(std::move(e2ee_transformer)) {}

  void Transform(
      std::unique_ptr<webrtc::TransformableFrameInterface> frame) override {
    if (av1::IsAv1Frame(*frame) && !frame->GetData().empty() &&
        e2ee_transformer_->enabled()) {
      auto* video_frame =
          static_cast<webrtc::TransformableVideoFrameInterface*>(frame.get());
      if (video_frame->IsKeyFrame()) {
        if (auto obu = av1::ExtractSequenceHeaderObu(frame->GetData())) {
          cache_->Store(frame->GetSsrc(), frame->GetTimestamp(),
                        std::move(*obu));
        }
      }
    }

    webrtc::scoped_refptr<webrtc::TransformedFrameCallback> callback;
    {
      webrtc::MutexLock lock(&mutex_);
      auto it = sink_callbacks_.find(frame->GetSsrc());
      callback = it != sink_callbacks_.end() ? it->second : callback_;
    }
    if (callback) {
      callback->OnTransformedFrame(std::move(frame));
    }
  }

  void RegisterTransformedFrameCallback(
      webrtc::scoped_refptr<webrtc::TransformedFrameCallback> callback)
      override {
    webrtc::MutexLock lock(&mutex_);
    callback_ = std::move(callback);
  }

  void RegisterTransformedFrameSinkCallback(
      webrtc::scoped_refptr<webrtc::TransformedFrameCallback> callback,
      uint32_t ssrc) override {
    webrtc::MutexLock lock(&mutex_);
    sink_callbacks_[ssrc] = std::move(callback);
  }

  void UnregisterTransformedFrameCallback() override {
    webrtc::MutexLock lock(&mutex_);
    callback_ = nullptr;
  }

  void UnregisterTransformedFrameSinkCallback(uint32_t ssrc) override {
    webrtc::MutexLock lock(&mutex_);
    sink_callbacks_.erase(ssrc);
  }

 private:
  std::shared_ptr<Av1SequenceHeaderCache> cache_;
  webrtc::scoped_refptr<webrtc::FrameCryptorTransformer> e2ee_transformer_;
  mutable webrtc::Mutex mutex_;
  webrtc::scoped_refptr<webrtc::TransformedFrameCallback> callback_;
  std::unordered_map<uint32_t,
                     webrtc::scoped_refptr<webrtc::TransformedFrameCallback>>
      sink_callbacks_;
};

/// Adapts fully-encrypted AV1 payloads for RTP transport.
///
/// The AV1 RTP packetizer parses its input as a sequence of OBUs, so the
/// opaque payload produced by the e2ee transform cannot be packetized as-is:
/// frames are dropped or corrupted in transit, and SFUs lose keyframe
/// detection. On send this transformer runs after encryption and wraps the
/// encrypted payload into a synthetic AV1 temporal unit (see
/// av1::WrapEncryptedPayload) carrying the keyframe's real sequence header
/// captured by [`Av1SequenceHeaderSniffer`]; on receive it runs before
/// decryption and restores the encrypted payload. Frames of other codecs,
/// and AV1 frames passed through while encryption is disabled, are
/// forwarded untouched.
class Av1EncryptedPayloadAdapter : public webrtc::FrameTransformerInterface {
 public:
  enum class Direction { kSend, kReceive };

  Av1EncryptedPayloadAdapter(
      Direction direction,
      webrtc::scoped_refptr<webrtc::FrameCryptorTransformer> e2ee_transformer,
      std::shared_ptr<Av1SequenceHeaderCache> sequence_header_cache)
      : direction_(direction),
        e2ee_transformer_(std::move(e2ee_transformer)),
        sequence_header_cache_(std::move(sequence_header_cache)) {}

  void Transform(
      std::unique_ptr<webrtc::TransformableFrameInterface> frame) override {
    if (av1::IsAv1Frame(*frame) && !frame->GetData().empty()) {
      if (direction_ == Direction::kSend) {
        // Frames forwarded while encryption is disabled are already valid
        // AV1; wrapping them would break subscribers without an unwrap
        // stage.
        if (e2ee_transformer_->enabled()) {
          auto* video_frame =
              static_cast<webrtc::TransformableVideoFrameInterface*>(
                  frame.get());
          const bool is_keyframe = video_frame->IsKeyFrame();
          std::optional<std::vector<uint8_t>> sequence_header;
          if (is_keyframe && sequence_header_cache_) {
            sequence_header = sequence_header_cache_->Take(
                frame->GetSsrc(), frame->GetTimestamp());
            if (sequence_header) {
              RTC_LOG(LS_INFO)
                  << "Av1EncryptedPayloadAdapter: wrapping keyframe with real"
                     " sequence header ("
                  << sequence_header->size() << " bytes) ssrc="
                  << frame->GetSsrc() << " rtp_ts=" << frame->GetTimestamp();
            } else {
              RTC_LOG(LS_WARNING)
                  << "Av1EncryptedPayloadAdapter: no sniffed sequence header"
                     " for keyframe, using synthetic fallback ssrc="
                  << frame->GetSsrc() << " rtp_ts=" << frame->GetTimestamp();
            }
          }
          auto wrapped = av1::WrapEncryptedPayload(
              frame->GetData(), is_keyframe,
              sequence_header
                  ? webrtc::ArrayView<const uint8_t>(*sequence_header)
                  : webrtc::ArrayView<const uint8_t>());
          frame->SetData(webrtc::ArrayView<const uint8_t>(wrapped));
        }
      } else if (auto unwrapped =
                     av1::UnwrapEncryptedPayload(frame->GetData())) {
        frame->SetData(webrtc::ArrayView<const uint8_t>(*unwrapped));
      }
    }

    webrtc::scoped_refptr<webrtc::TransformedFrameCallback> callback;
    {
      webrtc::MutexLock lock(&mutex_);
      auto it = sink_callbacks_.find(frame->GetSsrc());
      callback = it != sink_callbacks_.end() ? it->second : callback_;
    }
    if (callback) {
      callback->OnTransformedFrame(std::move(frame));
    }
  }

  void RegisterTransformedFrameCallback(
      webrtc::scoped_refptr<webrtc::TransformedFrameCallback> callback)
      override {
    webrtc::MutexLock lock(&mutex_);
    callback_ = std::move(callback);
  }

  void RegisterTransformedFrameSinkCallback(
      webrtc::scoped_refptr<webrtc::TransformedFrameCallback> callback,
      uint32_t ssrc) override {
    webrtc::MutexLock lock(&mutex_);
    sink_callbacks_[ssrc] = std::move(callback);
  }

  void UnregisterTransformedFrameCallback() override {
    webrtc::MutexLock lock(&mutex_);
    callback_ = nullptr;
  }

  void UnregisterTransformedFrameSinkCallback(uint32_t ssrc) override {
    webrtc::MutexLock lock(&mutex_);
    sink_callbacks_.erase(ssrc);
  }

 private:
  const Direction direction_;
  webrtc::scoped_refptr<webrtc::FrameCryptorTransformer> e2ee_transformer_;
  std::shared_ptr<Av1SequenceHeaderCache> sequence_header_cache_;
  mutable webrtc::Mutex mutex_;
  webrtc::scoped_refptr<webrtc::TransformedFrameCallback> callback_;
  std::unordered_map<uint32_t,
                     webrtc::scoped_refptr<webrtc::TransformedFrameCallback>>
      sink_callbacks_;
};

webrtc::FrameCryptorTransformer::Algorithm AlgorithmToFrameCryptorAlgorithm(
    Algorithm algorithm) {
  switch (algorithm) {
    case Algorithm::AesGcm:
      return webrtc::FrameCryptorTransformer::Algorithm::kAesGcm;
    case Algorithm::AesCbc:
      return webrtc::FrameCryptorTransformer::Algorithm::kAesCbc;
    default:
      return webrtc::FrameCryptorTransformer::Algorithm::kAesGcm;
  }
}

webrtc::KeyDerivationAlgorithm
KeyDerivationAlgorithmToFrameCryptorKeyDerivationAlgorithm(
    KeyDerivationAlgorithm algorithm) {
  switch (algorithm) {
    case KeyDerivationAlgorithm::PBKDF2:
      return webrtc::KeyDerivationAlgorithm::kPBKDF2;
    case KeyDerivationAlgorithm::HKDF:
      return webrtc::KeyDerivationAlgorithm::kHKDF;
    default:
      return webrtc::KeyDerivationAlgorithm::kPBKDF2;
  }
}

KeyProvider::KeyProvider(KeyProviderOptions options) {
  webrtc::KeyProviderOptions rtc_options;
  rtc_options.shared_key = options.shared_key;

  std::vector<uint8_t> ratchet_salt;
  std::copy(options.ratchet_salt.begin(), options.ratchet_salt.end(),
            std::back_inserter(ratchet_salt));

  rtc_options.ratchet_salt = ratchet_salt;
  rtc_options.ratchet_window_size = options.ratchet_window_size;
  rtc_options.failure_tolerance = options.failure_tolerance;
  rtc_options.key_ring_size = options.key_ring_size;
  rtc_options.key_derivation_algorithm =
      KeyDerivationAlgorithmToFrameCryptorKeyDerivationAlgorithm(
          options.key_derivation_algorithm);
  impl_ =
      new webrtc::RefCountedObject<webrtc::DefaultKeyProviderImpl>(rtc_options);
}

FrameCryptor::FrameCryptor(
    std::shared_ptr<RtcRuntime> rtc_runtime,
    const std::string participant_id,
    webrtc::FrameCryptorTransformer::Algorithm algorithm,
    webrtc::scoped_refptr<webrtc::KeyProvider> key_provider,
    webrtc::scoped_refptr<webrtc::RtpSenderInterface> sender)
    : rtc_runtime_(rtc_runtime),
      participant_id_(participant_id),
      key_provider_(key_provider),
      sender_(sender) {
  auto mediaType =
      sender->track()->kind() == "audio"
          ? webrtc::FrameCryptorTransformer::MediaType::kAudioFrame
          : webrtc::FrameCryptorTransformer::MediaType::kVideoFrame;
  e2ee_transformer_ = webrtc::scoped_refptr<webrtc::FrameCryptorTransformer>(
      new webrtc::FrameCryptorTransformer(rtc_runtime->signaling_thread(),
                                          participant_id, mediaType, algorithm,
                                          key_provider_));
  if (mediaType == webrtc::FrameCryptorTransformer::MediaType::kVideoFrame) {
    auto sequence_header_cache = std::make_shared<Av1SequenceHeaderCache>();
    av1_sniffer_ = webrtc::make_ref_counted<Av1SequenceHeaderSniffer>(
        sequence_header_cache, e2ee_transformer_);
    av1_adapter_ = webrtc::make_ref_counted<Av1EncryptedPayloadAdapter>(
        Av1EncryptedPayloadAdapter::Direction::kSend, e2ee_transformer_,
        std::move(sequence_header_cache));
  }
  sender->SetEncoderToPacketizerFrameTransformer(encryption_transformer());
  e2ee_transformer_->SetEnabled(false);
}

FrameCryptor::FrameCryptor(
    std::shared_ptr<RtcRuntime> rtc_runtime,
    const std::string participant_id,
    webrtc::FrameCryptorTransformer::Algorithm algorithm,
    webrtc::scoped_refptr<webrtc::KeyProvider> key_provider,
    webrtc::scoped_refptr<webrtc::RtpReceiverInterface> receiver)
    : rtc_runtime_(rtc_runtime),
      participant_id_(participant_id),
      key_provider_(key_provider),
      receiver_(receiver) {
  auto mediaType =
      receiver->track()->kind() == "audio"
          ? webrtc::FrameCryptorTransformer::MediaType::kAudioFrame
          : webrtc::FrameCryptorTransformer::MediaType::kVideoFrame;
  e2ee_transformer_ = webrtc::scoped_refptr<webrtc::FrameCryptorTransformer>(
      new webrtc::FrameCryptorTransformer(rtc_runtime->signaling_thread(),
                                          participant_id, mediaType, algorithm,
                                          key_provider_));
  if (mediaType == webrtc::FrameCryptorTransformer::MediaType::kVideoFrame) {
    av1_adapter_ = webrtc::make_ref_counted<Av1EncryptedPayloadAdapter>(
        Av1EncryptedPayloadAdapter::Direction::kReceive, e2ee_transformer_,
        nullptr);
  }
  receiver->SetDepacketizerToDecoderFrameTransformer(encryption_transformer());
  e2ee_transformer_->SetEnabled(false);
}

FrameCryptor::~FrameCryptor() {
  if (observer_) {
    unregister_observer();
  }
}

void FrameCryptor::register_observer(
    rust::Box<RtcFrameCryptorObserverWrapper> observer) const {
  webrtc::MutexLock lock(&mutex_);
  observer_ = webrtc::make_ref_counted<NativeFrameCryptorObserver>(
      std::move(observer), this);
  e2ee_transformer_->RegisterFrameCryptorTransformerObserver(observer_);
}

void FrameCryptor::unregister_observer() const {
  webrtc::MutexLock lock(&mutex_);
  observer_ = nullptr;
  e2ee_transformer_->UnRegisterFrameCryptorTransformerObserver();
}

webrtc::scoped_refptr<webrtc::FrameTransformerInterface>
FrameCryptor::encryption_transformer() const {
  if (!av1_adapter_) {
    return e2ee_transformer_;
  }
  if (sender_) {
    // Sniff the plaintext sequence header, encrypt, then wrap the encrypted
    // payload (attaching the sniffed header on keyframes).
    return webrtc::make_ref_counted<ChainedFrameTransformer>(
        av1_sniffer_, webrtc::make_ref_counted<ChainedFrameTransformer>(
                          e2ee_transformer_, av1_adapter_));
  }
  return webrtc::make_ref_counted<ChainedFrameTransformer>(av1_adapter_,
                                                           e2ee_transformer_);
}

void FrameCryptor::set_packet_trailer_handler(
    std::shared_ptr<PacketTrailerHandler> handler) const {
  if (!handler) {
    return;
  }

  auto timestamp_transformer = handler->transformer();
  if (!timestamp_transformer) {
    return;
  }

  // The trailer transform stays outside the encryption boundary for every
  // codec: it runs against the encrypted (and, for AV1, wrapped) payload on
  // send and strips the trailer before decryption on receive.
  webrtc::scoped_refptr<webrtc::FrameTransformerInterface> first;
  webrtc::scoped_refptr<webrtc::FrameTransformerInterface> second;
  if (sender_) {
    first = encryption_transformer();
    second = timestamp_transformer;
  } else if (receiver_) {
    first = timestamp_transformer;
    second = encryption_transformer();
  } else {
    return;
  }

  chained_transformer_ =
      webrtc::make_ref_counted<ChainedFrameTransformer>(first, second);

  if (sender_) {
    sender_->SetEncoderToPacketizerFrameTransformer(chained_transformer_);
  }
  if (receiver_) {
    receiver_->SetDepacketizerToDecoderFrameTransformer(chained_transformer_);
  }
}

NativeFrameCryptorObserver::NativeFrameCryptorObserver(
    rust::Box<RtcFrameCryptorObserverWrapper> observer,
    const FrameCryptor* fc)
    : observer_(std::move(observer)), fc_(fc) {}

NativeFrameCryptorObserver::~NativeFrameCryptorObserver() {}

void NativeFrameCryptorObserver::OnFrameCryptionStateChanged(
    const std::string participant_id,
    webrtc::FrameCryptionState state) {
  observer_->on_frame_cryption_state_change(
      participant_id, static_cast<FrameCryptionState>(state));
}

void FrameCryptor::set_enabled(bool enabled) const {
  webrtc::MutexLock lock(&mutex_);
  e2ee_transformer_->SetEnabled(enabled);
}

bool FrameCryptor::enabled() const {
  webrtc::MutexLock lock(&mutex_);
  return e2ee_transformer_->enabled();
}

void FrameCryptor::set_key_index(int32_t index) const {
  webrtc::MutexLock lock(&mutex_);
  e2ee_transformer_->SetKeyIndex(index);
}

int32_t FrameCryptor::key_index() const {
  webrtc::MutexLock lock(&mutex_);
  return e2ee_transformer_->key_index();
}

DataPacketCryptor::DataPacketCryptor(
    webrtc::FrameCryptorTransformer::Algorithm algorithm,
    webrtc::scoped_refptr<webrtc::KeyProvider> key_provider)
    : data_packet_cryptor_(
          webrtc::make_ref_counted<webrtc::DataPacketCryptor>(algorithm,
                                                              key_provider)) {}

EncryptedPacket DataPacketCryptor::encrypt_data_packet(
    const ::rust::String participant_id,
    uint32_t key_index,
    rust::Vec<::std::uint8_t> data) const {
  std::vector<uint8_t> data_vec;
  std::copy(data.begin(), data.end(), std::back_inserter(data_vec));

  auto result = data_packet_cryptor_->Encrypt(
      std::string(participant_id.data(), participant_id.size()), key_index,
      data_vec);

  if (!result.ok()) {
    throw std::runtime_error(std::string("Failed to encrypt data packet: ") +
                             result.error().message());
  }

  auto& packet = result.value();

  EncryptedPacket encrypted_packet;
  encrypted_packet.data = rust::Vec<uint8_t>();
  std::copy(packet->data.begin(), packet->data.end(),
            std::back_inserter(encrypted_packet.data));

  encrypted_packet.iv = rust::Vec<uint8_t>();
  std::copy(packet->iv.begin(), packet->iv.end(),
            std::back_inserter(encrypted_packet.iv));

  encrypted_packet.key_index = packet->key_index;

  return encrypted_packet;
}

rust::Vec<::std::uint8_t> DataPacketCryptor::decrypt_data_packet(
    const ::rust::String participant_id,
    const EncryptedPacket& encrypted_packet) const {
  std::vector<uint8_t> data_vec;
  std::copy(encrypted_packet.data.begin(), encrypted_packet.data.end(),
            std::back_inserter(data_vec));

  std::vector<uint8_t> iv_vec;
  std::copy(encrypted_packet.iv.begin(), encrypted_packet.iv.end(),
            std::back_inserter(iv_vec));

  auto native_encrypted_packet =
      webrtc::make_ref_counted<webrtc::EncryptedPacket>(
          std::move(data_vec), std::move(iv_vec), encrypted_packet.key_index);

  auto result = data_packet_cryptor_->Decrypt(
      std::string(participant_id.data(), participant_id.size()),
      native_encrypted_packet);

  if (!result.ok()) {
    throw std::runtime_error(std::string("Failed to decrypt data packet: ") +
                             result.error().message());
  }

  rust::Vec<uint8_t> decrypted_data;
  auto& decrypted = result.value();
  std::copy(decrypted.begin(), decrypted.end(),
            std::back_inserter(decrypted_data));
  return decrypted_data;
}

std::shared_ptr<KeyProvider> new_key_provider(KeyProviderOptions options) {
  return std::make_shared<KeyProvider>(options);
}

std::shared_ptr<FrameCryptor> new_frame_cryptor_for_rtp_sender(
    std::shared_ptr<PeerConnectionFactory> peer_factory,
    const ::rust::String participant_id,
    Algorithm algorithm,
    std::shared_ptr<KeyProvider> key_provider,
    std::shared_ptr<RtpSender> sender) {
  return std::make_shared<FrameCryptor>(
      peer_factory->rtc_runtime(),
      std::string(participant_id.data(), participant_id.size()),
      AlgorithmToFrameCryptorAlgorithm(algorithm),
      key_provider->rtc_key_provider(), sender->rtc_sender());
}

std::shared_ptr<FrameCryptor> new_frame_cryptor_for_rtp_receiver(
    std::shared_ptr<PeerConnectionFactory> peer_factory,
    const ::rust::String participant_id,
    Algorithm algorithm,
    std::shared_ptr<KeyProvider> key_provider,
    std::shared_ptr<RtpReceiver> receiver) {
  return std::make_shared<FrameCryptor>(
      peer_factory->rtc_runtime(),
      std::string(participant_id.data(), participant_id.size()),
      AlgorithmToFrameCryptorAlgorithm(algorithm),
      key_provider->rtc_key_provider(), receiver->rtc_receiver());
}

std::shared_ptr<DataPacketCryptor> new_data_packet_cryptor(
    Algorithm algorithm,
    std::shared_ptr<KeyProvider> key_provider) {
  return std::make_shared<DataPacketCryptor>(
      AlgorithmToFrameCryptorAlgorithm(algorithm),
      key_provider->rtc_key_provider());
}

}  // namespace livekit_ffi
