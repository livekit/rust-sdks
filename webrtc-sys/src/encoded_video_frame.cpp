#include "livekit/encoded_video_frame.h"
#include "api/video/video_frame_metadata.h"

namespace livekit {

EncodedVideoFrame::EncodedVideoFrame(
    std::unique_ptr<webrtc::TransformableVideoFrameInterface> frame) 
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

bool EncodedVideoFrame::is_key_frame() const {
    return frame_->IsKeyFrame();
}

uint16_t EncodedVideoFrame::width() const {
    return frame_->GetMetadata().GetWidth();
}

uint16_t EncodedVideoFrame::height() const {
    return frame_->GetMetadata().GetHeight();
}

uint8_t EncodedVideoFrame::payload_type() const {
    return frame_->GetPayloadType();
}

const uint8_t* EncodedVideoFrame::payload_data() const {
    return data;
}

size_t EncodedVideoFrame::payload_size() const {
    return size;
}

uint32_t EncodedVideoFrame::timestamp() const {
    return frame_->GetTimestamp();
}

}