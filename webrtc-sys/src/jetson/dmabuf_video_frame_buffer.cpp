#include "dmabuf_video_frame_buffer.h"

#include <fcntl.h>
#include <sys/mman.h>
#include <unistd.h>

#include <algorithm>
#include <cstring>
#include <utility>

#include "api/make_ref_counted.h"
#include "rtc_base/logging.h"
#include "third_party/libyuv/include/libyuv/convert.h"

namespace livekit_ffi {

namespace {

constexpr uint32_t FourCc(char a, char b, char c, char d) {
  return static_cast<uint32_t>(a) | (static_cast<uint32_t>(b) << 8) |
         (static_cast<uint32_t>(c) << 16) | (static_cast<uint32_t>(d) << 24);
}

constexpr uint32_t kDrmFormatNv12 = FourCc('N', 'V', '1', '2');

class MappedPlane {
 public:
  explicit MappedPlane(const DmaBufVideoFramePlane& plane) {
    if (plane.fd < 0 || plane.size == 0) {
      return;
    }

    const long page_size = sysconf(_SC_PAGESIZE);
    if (page_size <= 0) {
      return;
    }

    const size_t page_mask = static_cast<size_t>(page_size - 1);
    map_offset_ = plane.offset & ~page_mask;
    const size_t plane_delta = plane.offset - map_offset_;
    map_size_ = plane_delta + plane.size;

    void* mapped =
        mmap(nullptr, map_size_, PROT_READ, MAP_SHARED, plane.fd, map_offset_);
    if (mapped == MAP_FAILED) {
      map_size_ = 0;
      map_offset_ = 0;
      return;
    }

    mapped_ = mapped;
    data_ = static_cast<const uint8_t*>(mapped_) + plane_delta;
  }

  MappedPlane(const MappedPlane&) = delete;
  MappedPlane& operator=(const MappedPlane&) = delete;

  ~MappedPlane() {
    if (mapped_) {
      munmap(mapped_, map_size_);
    }
  }

  const uint8_t* data() const { return data_; }

 private:
  void* mapped_ = nullptr;
  const uint8_t* data_ = nullptr;
  size_t map_offset_ = 0;
  size_t map_size_ = 0;
};

bool DuplicatePlaneFd(DmaBufVideoFramePlane* plane) {
  if (plane->fd < 0) {
    return false;
  }

  const int duplicated = fcntl(plane->fd, F_DUPFD_CLOEXEC, 0);
  if (duplicated < 0) {
    RTC_LOG(LS_WARNING) << "Failed to duplicate DMA-BUF FD";
    return false;
  }

  plane->fd = duplicated;
  return true;
}

void ClosePlaneFd(const DmaBufVideoFramePlane& plane) {
  if (plane.fd >= 0) {
    close(plane.fd);
  }
}

}  // namespace

DmaBufVideoFrameBuffer::DmaBufVideoFrameBuffer(
    DmaBufVideoFrameDescriptor descriptor,
    ReleaseCallback release_callback)
    : descriptor_(descriptor), release_callback_(std::move(release_callback)) {}

DmaBufVideoFrameBuffer::~DmaBufVideoFrameBuffer() {
  if (release_callback_) {
    release_callback_();
  }
}

webrtc::VideoFrameBuffer::Type DmaBufVideoFrameBuffer::type() const {
  return Type::kNative;
}

int DmaBufVideoFrameBuffer::width() const {
  return static_cast<int>(descriptor_.width);
}

int DmaBufVideoFrameBuffer::height() const {
  return static_cast<int>(descriptor_.height);
}

webrtc::scoped_refptr<webrtc::I420BufferInterface>
DmaBufVideoFrameBuffer::ToI420() {
  if (descriptor_.fourcc != kDrmFormatNv12 || descriptor_.num_planes < 2 ||
      descriptor_.y.fd < 0 || descriptor_.uv.fd < 0) {
    RTC_LOG(LS_WARNING) << "Cannot convert DMA-BUF frame to I420";
    return nullptr;
  }

  MappedPlane y(descriptor_.y);
  MappedPlane uv(descriptor_.uv);
  if (!y.data() || !uv.data()) {
    RTC_LOG(LS_WARNING) << "Failed to map DMA-BUF frame for CPU conversion";
    return nullptr;
  }

  auto i420 = webrtc::I420Buffer::Create(width(), height());
  const int result = libyuv::NV12ToI420(
      y.data(), static_cast<int>(descriptor_.y.stride), uv.data(),
      static_cast<int>(descriptor_.uv.stride), i420->MutableDataY(),
      i420->StrideY(), i420->MutableDataU(), i420->StrideU(),
      i420->MutableDataV(), i420->StrideV(), width(), height());
  if (result != 0) {
    RTC_LOG(LS_WARNING) << "libyuv::NV12ToI420 failed for DMA-BUF frame: "
                        << result;
    return nullptr;
  }

  return i420;
}

const DmaBufVideoFrameDescriptor& DmaBufVideoFrameBuffer::descriptor() const {
  return descriptor_;
}

bool GetDmaBufVideoFrameDescriptor(const webrtc::VideoFrameBuffer* buffer,
                                   DmaBufVideoFrameDescriptor* descriptor) {
  if (!buffer || !descriptor) {
    return false;
  }

  const auto* dmabuf = dynamic_cast<const DmaBufVideoFrameBuffer*>(buffer);
  if (!dmabuf) {
    return false;
  }

  *descriptor = dmabuf->descriptor();
  if (!DuplicatePlaneFd(&descriptor->y)) {
    return false;
  }
  if (!DuplicatePlaneFd(&descriptor->uv)) {
    ClosePlaneFd(descriptor->y);
    return false;
  }

  return true;
}

}  // namespace livekit_ffi
