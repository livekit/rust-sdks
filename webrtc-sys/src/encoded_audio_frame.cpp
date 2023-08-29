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

uint16_t EncodedAudioFrame::sequence_number() const {
    webrtc::RTPHeader header = frame_->GetHeader();
    return header.sequenceNumber;
};

const uint8_t* EncodedAudioFrame::payload_data() const {
    return data;
}

size_t EncodedAudioFrame::payload_size() const {
    return size;
}

uint32_t EncodedAudioFrame::ssrc() const {
    return frame_->GetHeader().ssrc;
}

uint32_t EncodedAudioFrame::timestamp() const {
    return frame_->GetTimestamp();
}

std::shared_ptr<uint64_t> EncodedAudioFrame::absolute_capture_timestamp() const {
    webrtc::RTPHeader header = frame_->GetHeader();
    if (header.extension.absolute_capture_time.has_value()) {
        webrtc::AbsoluteCaptureTime absolute_capture_time = 
            header.extension.absolute_capture_time.value();
        std::shared_ptr<uint64_t> p = std::make_shared<uint64_t>(absolute_capture_time.absolute_capture_timestamp);
        return p;
    }
    else {
        return nullptr;
    }
}

std::shared_ptr<int64_t> EncodedAudioFrame::estimated_capture_clock_offset() const {
    webrtc::RTPHeader header = frame_->GetHeader();

    if (header.extension.absolute_capture_time.has_value()) {
        webrtc::AbsoluteCaptureTime absolute_capture_time = 
            header.extension.absolute_capture_time.value();
        if (absolute_capture_time.estimated_capture_clock_offset.has_value()) {
            std::shared_ptr<int64_t> p = std::make_shared<int64_t>(absolute_capture_time.estimated_capture_clock_offset.value());
            return p;
        }

    }
    return nullptr;
}

std::unique_ptr<webrtc::TransformableAudioFrameInterface> EncodedAudioFrame::get_raw_frame() {
    std::unique_ptr<webrtc::TransformableAudioFrameInterface> tmp = std::move(frame_);
    
    frame_ = nullptr;
    
    return tmp;
}

}