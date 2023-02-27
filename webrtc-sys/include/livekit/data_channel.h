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

#pragma once

#include <memory>

#include "api/data_channel_interface.h"
#include "livekit/webrtc.h"
#include "rust/cxx.h"

namespace livekit {
class DataChannel;
using NativeDataChannelInit = webrtc::DataChannelInit;
class NativeDataChannelObserver;
}  // namespace livekit
#include "webrtc-sys/src/data_channel.rs.h"

namespace livekit {

class DataChannel {
 public:
  explicit DataChannel(
      std::shared_ptr<RTCRuntime> rtc_runtime,
      rtc::scoped_refptr<webrtc::DataChannelInterface> data_channel);

  void register_observer(NativeDataChannelObserver* observer) const;
  void unregister_observer() const;
  bool send(const DataBuffer& buffer) const;
  rust::String label() const;
  DataState state() const;
  void close() const;

 private:
  std::shared_ptr<RTCRuntime> rtc_runtime_;
  rtc::scoped_refptr<webrtc::DataChannelInterface> data_channel_;
};

std::unique_ptr<NativeDataChannelInit> create_data_channel_init(
    DataChannelInit init);

static std::shared_ptr<DataChannel> _shared_data_channel() {
  return nullptr;  // Ignore
}

class NativeDataChannelObserver : public webrtc::DataChannelObserver {
 public:
  explicit NativeDataChannelObserver(
      rust::Box<DataChannelObserverWrapper> observer,
      DataChannel* dc);

  ~NativeDataChannelObserver();

  void OnStateChange() override;
  void OnMessage(const webrtc::DataBuffer& buffer) override;
  void OnBufferedAmountChange(uint64_t sent_data_size) override;

 private:
  rust::Box<DataChannelObserverWrapper> observer_;
  const DataChannel* dc_;
};

std::shared_ptr<NativeDataChannelObserver> create_native_data_channel_observer(
    rust::Box<DataChannelObserverWrapper> observer,
    const DataChannel* dc);
}  // namespace livekit
