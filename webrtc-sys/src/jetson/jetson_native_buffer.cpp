/*
 * Copyright 2025 LiveKit, Inc.
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

#include "jetson_native_buffer.h"

namespace livekit {

int JetsonDmabufVideoFrameBuffer::width() const { return width_; }
int JetsonDmabufVideoFrameBuffer::height() const { return height_; }

rtc::scoped_refptr<webrtc::I420BufferInterface> JetsonDmabufVideoFrameBuffer::ToI420() {
  return nullptr;
}

bool JetsonDmabufVideoFrameBuffer::is_nv12() const { return layout_ == PixelLayout::kNV12M; }
int JetsonDmabufVideoFrameBuffer::fd_y() const { return fd_y_; }
int JetsonDmabufVideoFrameBuffer::fd_u() const { return fd_u_; }
int JetsonDmabufVideoFrameBuffer::fd_v() const { return fd_v_; }
int JetsonDmabufVideoFrameBuffer::stride_y() const { return stride_y_; }
int JetsonDmabufVideoFrameBuffer::stride_u() const { return stride_u_; }
int JetsonDmabufVideoFrameBuffer::stride_v() const { return stride_v_; }

}  // namespace livekit


