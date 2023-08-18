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
    //return frame_->GetMetadata().GetWidth();
    return 1280;
}

uint16_t EncodedVideoFrame::height() const {
    //return frame_->GetMetadata().GetHeight();
    return 720;
}

uint16_t EncodedVideoFrame::first_seq_num() const {
    return frame_->first_seq_num();
}

uint16_t EncodedVideoFrame::last_seq_num() const {
    return frame_->last_seq_num();
}

uint8_t EncodedVideoFrame::payload_type() const {
    return frame_->GetPayloadType();
}

int64_t EncodedVideoFrame::get_ntp_time_ms() const {
    return frame_->GetNtpTimeMs();
}

std::shared_ptr<int64_t> EncodedVideoFrame::frame_id() const {
    if (frame_->header().generic.has_value()) {
        webrtc::RTPVideoHeader::GenericDescriptorInfo generic = frame_->header().generic.value();
        std::shared_ptr<int64_t> p = std::make_shared<int64_t>(generic.frame_id);
        return p;
    }
    else {
        fprintf(stderr, "frame_id empty\n");
    }
    return nullptr;
};

int EncodedVideoFrame::temporal_index() const {
    //return frame_->GetMetadata().GetTemporalIndex();
    return 0;
};

const uint8_t* EncodedVideoFrame::payload_data() const {
    return data;
}

size_t EncodedVideoFrame::payload_size() const {
    return size;
}

uint32_t EncodedVideoFrame::timestamp() const {
    return frame_->GetTimestamp();
}


std::shared_ptr<uint64_t> EncodedVideoFrame::absolute_capture_timestamp() const {
    webrtc::RTPVideoHeader header = frame_->header();
    if (header.absolute_capture_time.has_value()) {
        webrtc::AbsoluteCaptureTime absolute_capture_time = 
            header.absolute_capture_time.value();

        std::shared_ptr<uint64_t> p = std::make_shared<uint64_t>(absolute_capture_time.absolute_capture_timestamp);
        return p;
    }
    return nullptr;
}

std::shared_ptr<int64_t> EncodedVideoFrame::estimated_capture_clock_offset() const {
    webrtc::RTPVideoHeader header = frame_->header();
    if (header.absolute_capture_time.has_value()) {
        webrtc::AbsoluteCaptureTime absolute_capture_time = 
            header.absolute_capture_time.value();
        if (absolute_capture_time.estimated_capture_clock_offset.has_value()) {
            std::shared_ptr<int64_t> p = std::make_shared<int64_t>(absolute_capture_time.estimated_capture_clock_offset.value());
            return p;
        }
    }
    return nullptr;
}

}