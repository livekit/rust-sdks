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

#pragma once

#include <stdint.h>

#include <memory>
#include <string>
#include <unordered_map>
#include <vector>

#include "api/crypto/frame_crypto_transformer.h"
#include "api/scoped_refptr.h"
#include "livekit/peer_connection.h"
#include "livekit/peer_connection_factory.h"
#include "livekit/rtp_receiver.h"
#include "livekit/rtp_sender.h"
#include "livekit/webrtc.h"
#include "rtc_base/synchronization/mutex.h"
#include "rust/cxx.h"

namespace livekit_ffi {

struct KeyProviderOptions;
struct EncryptedPacket;
enum class Algorithm : ::std::int32_t;
class RtcFrameCryptorObserverWrapper;
class NativeFrameCryptorObserver;
class PacketTrailerHandler;

/// WebRTC's key provider with receivers: shared-key handlers held here rather than in the
/// default provider's map, which can't remove one, each of which can be pointed at a
/// participant's handler. A cryptor looks its handler up once per frame, so a frame already
/// being decrypted keeps using, and writing its ratchets to, the handler it started with.
///
/// Lock order: `receivers_mutex_`, then the default provider's lock, then a handler's. Every
/// change to the "shared" handler and every copy of it holds `receivers_mutex_`, as copying a
/// handler takes no lock, and a shared-key update reaches the default provider's handlers and
/// the receivers' handlers in one step.
class BindingKeyProvider : public webrtc::DefaultKeyProviderImpl {
 public:
  /// Receiver ids start with this (the Rust E2EE manager makes them). The control character
  /// keeps them apart from real participant identities.
  static constexpr const char* kReceiverPrefix = "\x1freceiver:";

  explicit BindingKeyProvider(webrtc::KeyProviderOptions options)
      : webrtc::DefaultKeyProviderImpl(options) {}

  const webrtc::scoped_refptr<webrtc::ParticipantKeyHandler> GetSharedKey(
      const std::string participant_id) override {
    webrtc::MutexLock lock(&receivers_mutex_);
    auto it = receivers_.find(participant_id);
    if (it == receivers_.end()) {
      // A frame can still look up a removed receiver: it is dropped as missing its key
      // rather than given a handler in the default provider's map, which would never be freed.
      if (participant_id.rfind(kReceiverPrefix, 0) == 0) {
        return nullptr;
      }
      return webrtc::DefaultKeyProviderImpl::GetSharedKey(participant_id);
    }
    if (!it->second.participant_id.empty()) {
      return webrtc::DefaultKeyProviderImpl::GetSharedKey(
          it->second.participant_id);
    }
    if (!it->second.keys) {
      auto shared = GetKey("shared");
      if (!shared) {
        return nullptr;
      }
      it->second.keys = shared->Clone();
    }
    return it->second.keys;
  }

  bool SetSharedKey(int key_index, std::vector<uint8_t> key) override {
    webrtc::MutexLock lock(&receivers_mutex_);
    if (!webrtc::DefaultKeyProviderImpl::SetSharedKey(key_index, key)) {
      return false;
    }
    for (auto& receiver : receivers_) {
      if (receiver.second.keys) {
        receiver.second.keys->SetKey(key, key_index);
      }
    }
    return true;
  }

  const std::vector<uint8_t> RatchetSharedKey(int key_index) override {
    webrtc::MutexLock lock(&receivers_mutex_);
    auto key = webrtc::DefaultKeyProviderImpl::RatchetSharedKey(key_index);
    if (!key.empty() && options().shared_key) {
      for (auto& receiver : receivers_) {
        if (receiver.second.keys) {
          receiver.second.keys->SetKey(key, key_index);
        }
      }
    }
    return key;
  }

  bool SetKey(const std::string participant_id,
              int index,
              std::vector<uint8_t> key) override {
    if (participant_id != "shared") {
      return webrtc::DefaultKeyProviderImpl::SetKey(participant_id, index, key);
    }
    webrtc::MutexLock lock(&receivers_mutex_);
    return webrtc::DefaultKeyProviderImpl::SetKey(participant_id, index, key);
  }

  const std::vector<uint8_t> RatchetKey(const std::string participant_id,
                                        int key_index) override {
    if (participant_id != "shared") {
      return webrtc::DefaultKeyProviderImpl::RatchetKey(participant_id,
                                                        key_index);
    }
    webrtc::MutexLock lock(&receivers_mutex_);
    return webrtc::DefaultKeyProviderImpl::RatchetKey(participant_id, key_index);
  }

  /// For a receiver id, the key its frames are decrypted with, found as a frame finds it.
  const std::vector<uint8_t> ExportKey(const std::string participant_id,
                                       int key_index) const override {
    {
      webrtc::MutexLock lock(&receivers_mutex_);
      if (receivers_.find(participant_id) == receivers_.end()) {
        return webrtc::DefaultKeyProviderImpl::ExportKey(participant_id,
                                                         key_index);
      }
    }
    auto keys =
        const_cast<BindingKeyProvider*>(this)->GetSharedKey(participant_id);
    auto key_set = keys ? keys->GetKeySet(key_index) : nullptr;
    return key_set ? key_set->material : std::vector<uint8_t>();
  }

  void AddReceiver(const std::string& id) {
    webrtc::MutexLock lock(&receivers_mutex_);
    auto shared = GetKey("shared");
    receivers_[id].keys = shared ? shared->Clone() : nullptr;
  }

  /// An empty `participant_id` puts the receiver back on its own handler.
  void BindReceiver(const std::string& id, const std::string& participant_id) {
    webrtc::MutexLock lock(&receivers_mutex_);
    auto it = receivers_.find(id);
    if (it != receivers_.end()) {
      it->second.participant_id = participant_id;
    }
  }

  void RemoveReceiver(const std::string& id) {
    webrtc::MutexLock lock(&receivers_mutex_);
    receivers_.erase(id);
  }

 private:
  struct Receiver {
    /// Its own handler, a copy of the shared one (made on first use if there was none).
    webrtc::scoped_refptr<webrtc::ParticipantKeyHandler> keys;
    /// The participant whose handler it uses instead, if any.
    std::string participant_id;
  };
  mutable webrtc::Mutex receivers_mutex_;
  std::unordered_map<std::string, Receiver> receivers_;
};

/// Shared secret key for frame encryption.
class KeyProvider {
 public:
  KeyProvider(KeyProviderOptions options);
  ~KeyProvider() {}

  bool set_shared_key(int32_t index, rust::Vec<::std::uint8_t> key) const {
    if (index < 0 || index >= key_ring_size_) {
      return false;
    }
    std::vector<uint8_t> key_vec;
    std::copy(key.begin(), key.end(), std::back_inserter(key_vec));
    return impl_->SetSharedKey(index, key_vec);
  }

  rust::Vec<::std::uint8_t> ratchet_shared_key(int32_t key_index) const {
    if (key_index < 0 || key_index >= key_ring_size_) {
      throw std::runtime_error("ratchet_shared_key failed: index out of bounds");
    }
    rust::Vec<uint8_t> vec;
    auto data = impl_->RatchetSharedKey(key_index);
    if (data.empty()) {
      throw std::runtime_error("ratchet_shared_key failed");
    }

    std::move(data.begin(), data.end(), std::back_inserter(vec));
    return vec;
  }

  rust::Vec<::std::uint8_t> get_shared_key(int32_t key_index) const {
    if (key_index < 0 || key_index >= key_ring_size_) {
      throw std::runtime_error("get_shared_key failed: index out of bounds");
    }
    rust::Vec<uint8_t> vec;
    auto data = impl_->ExportSharedKey(key_index);
    if (data.empty()) {
      throw std::runtime_error("get_shared_key failed");
    }

    std::move(data.begin(), data.end(), std::back_inserter(vec));
    return vec;
  }

  /// Set the key at the given index.
  bool set_key(const ::rust::String participant_id,
               int32_t index,
               rust::Vec<::std::uint8_t> key) const {
    if (index < 0 || index >= key_ring_size_) {
      return false;
    }
    std::vector<uint8_t> key_vec;
    std::copy(key.begin(), key.end(), std::back_inserter(key_vec));
    return impl_->SetKey(
        std::string(participant_id.data(), participant_id.size()), index,
        key_vec);
  }

  rust::Vec<::std::uint8_t> ratchet_key(const ::rust::String participant_id,
                                        int32_t key_index) const {
    if (key_index < 0 || key_index >= key_ring_size_) {
      throw std::runtime_error("ratchet_key failed: index out of bounds");
    }
    rust::Vec<uint8_t> vec;
    auto data = impl_->RatchetKey(
        std::string(participant_id.data(), participant_id.size()), key_index);
    if (data.empty()) {
      throw std::runtime_error("ratchet_key failed");
    }

    std::move(data.begin(), data.end(), std::back_inserter(vec));
    return vec;
  }

  rust::Vec<::std::uint8_t> get_key(const ::rust::String participant_id,
                                    int32_t key_index) const {
    if (key_index < 0 || key_index >= key_ring_size_) {
      throw std::runtime_error("get_key failed: index out of bounds");
    }
    rust::Vec<uint8_t> vec;
    auto data = impl_->ExportKey(
        std::string(participant_id.data(), participant_id.size()), key_index);
    if (data.empty()) {
      throw std::runtime_error("get_key failed");
    }

    std::move(data.begin(), data.end(), std::back_inserter(vec));
    return vec;
  }

  void add_receiver(const ::rust::String id) const {
    impl_->AddReceiver(std::string(id.data(), id.size()));
  }

  void bind_receiver(const ::rust::String id,
                     const ::rust::String participant_id) const {
    impl_->BindReceiver(std::string(id.data(), id.size()),
                        std::string(participant_id.data(), participant_id.size()));
  }

  void remove_receiver(const ::rust::String id) const {
    impl_->RemoveReceiver(std::string(id.data(), id.size()));
  }

  void set_sif_trailer(rust::Vec<::std::uint8_t> trailer) const {
    std::vector<uint8_t> trailer_vec;
    std::copy(trailer.begin(), trailer.end(), std::back_inserter(trailer_vec));
    impl_->SetSifTrailer(trailer_vec);
  }

  int32_t key_ring_size() const { return key_ring_size_; }

  webrtc::scoped_refptr<webrtc::KeyProvider> rtc_key_provider() { return impl_; }

 private:
  webrtc::scoped_refptr<BindingKeyProvider> impl_;
  int32_t key_ring_size_ = 16;
};

class FrameCryptor {
 public:
  FrameCryptor(std::shared_ptr<RtcRuntime> rtc_runtime,
               const std::string participant_id,
               webrtc::FrameCryptorTransformer::Algorithm algorithm,
               webrtc::scoped_refptr<webrtc::KeyProvider> key_provider,
               webrtc::scoped_refptr<webrtc::RtpSenderInterface> sender);

  FrameCryptor(std::shared_ptr<RtcRuntime> rtc_runtime,
               const std::string participant_id,
               webrtc::FrameCryptorTransformer::Algorithm algorithm,
               webrtc::scoped_refptr<webrtc::KeyProvider> key_provider,
               webrtc::scoped_refptr<webrtc::RtpReceiverInterface> receiver);
  ~FrameCryptor();

  /// Enable/Disable frame crypto for the sender or receiver.
  void set_enabled(bool enabled) const;

  /// Get the enabled state for the sender or receiver.
  bool enabled() const;

  /// Set the key index for the sender or receiver.
  /// If the key index is not set, the key index will be set to 0.
  void set_key_index(int32_t index) const;

  /// Get the key index for the sender or receiver.
  int32_t key_index() const;

  rust::String participant_id() const { return participant_id_; }

  void register_observer(
      rust::Box<RtcFrameCryptorObserverWrapper> observer) const;

  void unregister_observer() const;

  /// Attach a packet trailer transformer for chained processing.
  void set_packet_trailer_handler(
      std::shared_ptr<PacketTrailerHandler> handler) const;

 private:
  std::shared_ptr<RtcRuntime> rtc_runtime_;
  const rust::String participant_id_;
  mutable webrtc::Mutex mutex_;
  webrtc::scoped_refptr<webrtc::FrameCryptorTransformer> e2ee_transformer_;
  webrtc::scoped_refptr<webrtc::KeyProvider> key_provider_;
  webrtc::scoped_refptr<webrtc::RtpSenderInterface> sender_;
  webrtc::scoped_refptr<webrtc::RtpReceiverInterface> receiver_;
  mutable webrtc::scoped_refptr<NativeFrameCryptorObserver> observer_;
  mutable webrtc::scoped_refptr<webrtc::FrameTransformerInterface>
      chained_transformer_;
};

class NativeFrameCryptorObserver
    : public webrtc::FrameCryptorTransformerObserver {
 public:
  NativeFrameCryptorObserver(rust::Box<RtcFrameCryptorObserverWrapper> observer,
                             const FrameCryptor* fc);
  ~NativeFrameCryptorObserver();

  void OnFrameCryptionStateChanged(const std::string participant_id,
                                   webrtc::FrameCryptionState error) override;

 private:
  rust::Box<RtcFrameCryptorObserverWrapper> observer_;
  const FrameCryptor* fc_;
};

class DataPacketCryptor {
 public:
  DataPacketCryptor(webrtc::FrameCryptorTransformer::Algorithm algorithm,
                   webrtc::scoped_refptr<webrtc::KeyProvider> key_provider);

  EncryptedPacket encrypt_data_packet(
      const ::rust::String participant_id,
      uint32_t key_index,
      rust::Vec<::std::uint8_t> data) const;

  rust::Vec<::std::uint8_t> decrypt_data_packet(
      const ::rust::String participant_id,
      const EncryptedPacket& encrypted_packet) const;

 private:
  webrtc::scoped_refptr<webrtc::DataPacketCryptor> data_packet_cryptor_;
};

std::shared_ptr<FrameCryptor> new_frame_cryptor_for_rtp_sender(
    std::shared_ptr<PeerConnectionFactory> peer_factory,
    const ::rust::String participant_id,
    Algorithm algorithm,
    std::shared_ptr<KeyProvider> key_provider,
    std::shared_ptr<RtpSender> sender);

std::shared_ptr<FrameCryptor> new_frame_cryptor_for_rtp_receiver(
    std::shared_ptr<PeerConnectionFactory> peer_factory,
    const ::rust::String participant_id,
    Algorithm algorithm,
    std::shared_ptr<KeyProvider> key_provider,
    std::shared_ptr<RtpReceiver> receiver);

std::shared_ptr<KeyProvider> new_key_provider(KeyProviderOptions options);

std::shared_ptr<DataPacketCryptor> new_data_packet_cryptor(
    Algorithm algorithm,
    std::shared_ptr<KeyProvider> key_provider);

}  // namespace livekit_ffi
