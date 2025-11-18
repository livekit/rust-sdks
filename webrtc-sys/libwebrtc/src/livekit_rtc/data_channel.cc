#include "livekit_rtc/data_channel.h"

#include "api/data_channel_interface.h"
#include "api/make_ref_counted.h"
#include "livekit_rtc/capi.h"

namespace livekit {

webrtc::DataChannelInit toNativeDataChannelInit(const lkDataChannelInit& init) {
  webrtc::DataChannelInit nativeInit{};
  nativeInit.reliable = init.reliable;
  nativeInit.ordered = init.ordered;
  nativeInit.maxRetransmits = init.maxRetransmits;

  return nativeInit;
}

void DataChannelObserver::OnStateChange() {
  observer_->onStateChange(userdata_, data_channel_->State());
}

void DataChannelObserver::OnMessage(const webrtc::DataBuffer& buffer) {
  observer_->onMessage(buffer.data.data(), buffer.data.size(), buffer.binary,
                       userdata_);
}

void DataChannelObserver::OnBufferedAmountChange(uint64_t sentDataSize) {
  observer_->onBufferedAmountChange(sentDataSize, userdata_);
}

void DataChannel::RegisterObserver(const lkDataChannelObserver* observer,
                                   void* userdata) {
  webrtc::MutexLock lock(&mutex_);
  observer_ = std::make_unique<DataChannelObserver>(observer, this, userdata);
  data_channel_->RegisterObserver(observer_.get());
}

void DataChannel::UnregisterObserver() {
  webrtc::MutexLock lock(&mutex_);
  data_channel_->UnregisterObserver();
  observer_ = nullptr;
}

void DataChannel::SendAsync(const uint8_t* data,
                            uint64_t size,
                            bool binary,
                            void (*onComplete)(lkRtcError* error,
                                               void* userdata),
                            void* userdata) {
  rtc::CopyOnWriteBuffer cow{data, size};
  webrtc::DataBuffer buffer{cow, binary};
  data_channel_->SendAsync(buffer, [&](webrtc::RTCError err) {
    if (err.ok()) {
      onComplete(nullptr, userdata);
    } else {
      lkRtcError lkErr = toRtcError(err);
      onComplete(&lkErr, userdata);
    }
  });
}

}  // namespace livekit
