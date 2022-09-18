//
// Created by Théo Monnom on 01/09/2022.
//

#include <utility>

#include "livekit/data_channel.h"
#include "libwebrtc-sys/src/data_channel.rs.h"

namespace livekit {

    DataChannel::DataChannel(rtc::scoped_refptr<webrtc::DataChannelInterface> data_channel) : data_channel_(std::move(data_channel)) {

    }

    void DataChannel::register_observer(std::unique_ptr<NativeDataChannelObserver> observer) {
        data_channel_->RegisterObserver(observer.get());
    }

    void DataChannel::unregister_observer() {
        data_channel_->UnregisterObserver();
    }

    void DataChannel::close() {
        return data_channel_->Close();
    }

    std::unique_ptr<NativeDataChannelInit> create_data_channel_init(DataChannelInit init) {
        auto rtc_init = std::make_unique<webrtc::DataChannelInit>();
        rtc_init->id = init.id;
        rtc_init->negotiated = init.negotiated;
        rtc_init->ordered = init.ordered;
        rtc_init->protocol = init.protocol.c_str();
        rtc_init->reliable = init.reliable;

        if(init.has_max_retransmit_time)
            rtc_init->maxRetransmitTime = init.max_retransmit_time;

        if(init.has_max_retransmits)
            rtc_init->maxRetransmits = init.max_retransmits;

        if(init.has_priority)
            rtc_init->priority = static_cast<webrtc::Priority>(init.priority);

        return rtc_init;
    }

    NativeDataChannelObserver::NativeDataChannelObserver(rust::Box<DataChannelObserverWrapper> observer) : observer_(std::move(observer)){

    }

    void NativeDataChannelObserver::OnStateChange() {
        observer_->on_state_change();
    }

    void NativeDataChannelObserver::OnMessage(const webrtc::DataBuffer &buffer) {
        DataBuffer data{};
        data.binary = buffer.data.data();
        data.len = buffer.data.size();
        data.binary = buffer.binary;
        observer_->on_message(data);
    }

    void NativeDataChannelObserver::OnBufferedAmountChange(uint64_t sent_data_size) {
        observer_->on_buffered_amount_change(sent_data_size);
    }

    std::unique_ptr<NativeDataChannelObserver> create_native_peer_connection_observer(rust::Box<DataChannelObserverWrapper> observer){
        return std::make_unique<NativeDataChannelObserver>(std::move(observer));
    }
} // livekit