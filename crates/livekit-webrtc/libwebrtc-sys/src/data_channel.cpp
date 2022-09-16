//
// Created by Théo Monnom on 01/09/2022.
//

#include <utility>

#include "livekit/data_channel.h"
#include "libwebrtc-sys/src/data_channel.rs.h"

namespace livekit {

    DataChannel::DataChannel(rtc::scoped_refptr<webrtc::DataChannelInterface> data_channel) : data_channel_(std::move(data_channel)) {

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

} // livekit