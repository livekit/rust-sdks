#include "livekit/encoded_audio_frame.h"

namespace livekit {

EncodedAudioFrame::EncodedAudioFrame(
    std::unique_ptr<webrtc::TransformableAudioFrameInterface> frame) 
    : frame_(std::move(frame)) {
    
    rtc::ArrayView<const uint8_t> arr_view = frame_->GetData();

    // fprintf(stderr, "Original:\n");
    // for (auto e : arr_view) {
    //     fprintf(stderr, "%02x", e);
    // }
    // fprintf(stderr, "\n");

    // TODO: fix this - we don't know for how long the
    // result of data() is valid
    data = arr_view.data();
    size = arr_view.size();
}

uint8_t EncodedAudioFrame::payload_type() const {
    return frame_->GetPayloadType();
}

const uint8_t* EncodedAudioFrame::payload_data() const {
    return data;
}

size_t EncodedAudioFrame::payload_size() const {
    return size;
}

uint32_t EncodedAudioFrame::timestamp() const {
    return frame_->GetTimestamp();
}

}