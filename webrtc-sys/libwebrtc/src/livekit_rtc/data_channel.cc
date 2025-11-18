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
  observer_->onStateChange(userdata_,
                           static_cast<lkDcState>(data_channel_->state()));
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
  observer_ =
      std::make_unique<DataChannelObserver>(observer, data_channel_, userdata);
  data_channel_->RegisterObserver(observer_.get());
}

void DataChannel::UnregisterObserver() {
  webrtc::MutexLock lock(&mutex_);
  data_channel_->UnregisterObserver();
  observer_ = nullptr;
}

class onCompleteHandler {
 public:
  onCompleteHandler(void (*onComplete)(lkRtcError* error, void* userdata),
                    void* userdata)
      : onComplete_(onComplete), userdata_(userdata) {}

  void onComplete(webrtc::RTCError err) {
    if (err.ok()) {
      //onComplete_(nullptr, userdata_);
    } else {
      lkRtcError lkErr = toRtcError(err);
      //onComplete_(&lkErr, userdata_);
    }
  }

 private:
  void (*onComplete_)(lkRtcError* error, void* userdata);
  void* userdata_;
};

void DataChannel::SendAsync(const uint8_t* data,
                            uint64_t size,
                            bool binary,
                            void (*onComplete)(lkRtcError* error,
                                               void* userdata),
                            void* userdata) {
  auto handler = onCompleteHandler(onComplete, userdata);
  data_channel_->SendAsync(
      webrtc::DataBuffer{webrtc::CopyOnWriteBuffer(data, size), binary},
      [&](webrtc::RTCError err) { handler.onComplete(err); });
}

}  // namespace livekit
