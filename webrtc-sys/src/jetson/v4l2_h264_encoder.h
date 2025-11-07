#ifndef LIVEKIT_JETSON_V4L2_H264_ENCODER_H_
#define LIVEKIT_JETSON_V4L2_H264_ENCODER_H_

#include <linux/videodev2.h>
#include <stdint.h>
#include <sys/mman.h>

#include <optional>
#include <string>
#include <utility>
#include <vector>

namespace livekit {

struct DmabufPlanesNV12 {
  int fd_y;
  int fd_uv;
  int width;
  int height;
  int stride_y;
  int stride_uv;
};

// Minimal V4L2 M2M H264 encoder for Jetson devices.
// - OUTPUT: NV12 (DMABUF, 2-plane)
// - CAPTURE: H264 (MMAP)
class V4L2H264Encoder {
 public:
  V4L2H264Encoder();
  ~V4L2H264Encoder();

  // Open an encoder device and configure formats/buffers.
  bool Initialize(int width, int height, int fps, int bitrate_bps);

  // Enqueue one input frame via DMABUF NV12 planes. If keyframe is true, request IDR.
  bool EnqueueDmabufFrame(const DmabufPlanesNV12& planes, bool keyframe);

  // Try to dequeue an encoded frame. Returns empty if none available.
  std::optional<std::vector<uint8_t>> DequeueEncoded();

  // Update bitrate/framerate.
  void UpdateRates(int fps, int bitrate_bps);

  // Tear down queues/close fd.
  void Shutdown();

 private:
  bool OpenDevice();
  bool SetupOutputFormat(int width, int height);
  bool SetupCaptureFormat();
  bool SetControls(int fps, int bitrate_bps);
  bool RequestBuffers();
  bool StartStreaming();
  void StopStreaming();

  bool QueueOutput(const DmabufPlanesNV12& planes);
  bool DequeueOutput();  // drain if driver requires
  std::optional<std::pair<int, size_t>> DequeueCaptureIndexAndSize();
  bool QueueCapture(int index);

 private:
  int fd_ = -1;
  int width_ = 0;
  int height_ = 0;
  int fps_ = 0;
  int bitrate_bps_ = 0;

  // Capture MMAP buffers
  struct MappedBuffer {
    void* addr = MAP_FAILED;
    size_t length = 0;
  };
  std::vector<MappedBuffer> capture_buffers_;
  bool streaming_ = false;
};

}  // namespace livekit

#endif  // LIVEKIT_JETSON_V4L2_H264_ENCODER_H_


