//
// Created by Théo Monnom on 01/09/2022.
//

#ifndef CLIENT_SDK_NATIVE_DATA_CHANNEL_H
#define CLIENT_SDK_NATIVE_DATA_CHANNEL_H

#include <memory>

#include "api/data_channel_interface.h"
#include "rust/cxx.h"
#include "rust_types.h"
#include "webrtc.h"

namespace livekit {
using NativeDataChannelInit = webrtc::DataChannelInit;
class NativeDataChannelObserver;

class DataChannel {
 public:
  explicit DataChannel(
      std::shared_ptr<RTCRuntime> rtc_runtime,
      rtc::scoped_refptr<webrtc::DataChannelInterface> data_channel);

  void register_observer(NativeDataChannelObserver& observer) const;
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

static std::unique_ptr<DataChannel> _unique_data_channel() {
  return nullptr;  // Ignore
}

class NativeDataChannelObserver : public webrtc::DataChannelObserver {
 public:
  explicit NativeDataChannelObserver(
      rust::Box<DataChannelObserverWrapper> observer);

  void OnStateChange() override;
  void OnMessage(const webrtc::DataBuffer& buffer) override;
  void OnBufferedAmountChange(uint64_t sent_data_size) override;

 private:
  rust::Box<DataChannelObserverWrapper> observer_;
};

std::unique_ptr<NativeDataChannelObserver> create_native_data_channel_observer(
    rust::Box<DataChannelObserverWrapper> observer);
}  // namespace livekit

#endif  // CLIENT_SDK_NATIVE_DATA_CHANNEL_H
