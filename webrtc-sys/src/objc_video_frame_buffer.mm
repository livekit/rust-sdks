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

#include "livekit/video_frame_buffer.h"

#import <CoreVideo/CoreVideo.h>
#import <sdk/objc/components/video_frame_buffer/RTCCVPixelBuffer.h>
#include "sdk/objc/native/api/video_frame_buffer.h"

namespace livekit {

std::unique_ptr<VideoFrameBuffer> new_native_buffer(
    CVPixelBufferRef pixelBuffer
) {
    RTCCVPixelBuffer *buffer = [[RTCCVPixelBuffer alloc] initWithPixelBuffer:pixelBuffer];
    rtc::scoped_refptr<webrtc::VideoFrameBuffer> frame_buffer = webrtc::ObjCToNativeVideoFrameBuffer(buffer);
    [buffer release];
    [pixelBuffer release];
    return std::make_unique<VideoFrameBuffer>(frame_buffer);
}

}  // namespace livekit
