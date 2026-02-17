#ifndef LIVEKIT_JETSON_MMAPI_ENCODER_H_
#define LIVEKIT_JETSON_MMAPI_ENCODER_H_

#include <linux/videodev2.h>

#include <cstdint>
#include <optional>
#include <string>
#include <vector>

class NvVideoEncoder;

namespace livekit {

enum class JetsonCodec { kH264, kH265 };

class JetsonMmapiEncoder {
 public:
  explicit JetsonMmapiEncoder(JetsonCodec codec);
  ~JetsonMmapiEncoder();

  static bool IsSupported();
  static bool IsCodecSupported(JetsonCodec codec);

  bool Initialize(int width,
                  int height,
                  int framerate,
                  int bitrate_bps,
                  int keyframe_interval);
  void Destroy();
  bool IsInitialized() const;

  bool Encode(const uint8_t* src_y,
              int stride_y,
              const uint8_t* src_u,
              int stride_u,
              const uint8_t* src_v,
              int stride_v,
              bool force_keyframe,
              std::vector<uint8_t>* encoded,
              bool* is_keyframe);

  // NV12 input: full-resolution luma plane (Y) + interleaved chroma plane (UV).
  // This matches Jetson MMAPI encoder output-plane expectations on most Jetsons.
  bool EncodeNV12(const uint8_t* src_y,
                  int stride_y,
                  const uint8_t* src_uv,
                  int stride_uv,
                  bool force_keyframe,
                  std::vector<uint8_t>* encoded,
                  bool* is_keyframe);

  void SetRates(int framerate, int bitrate_bps);
  void SetKeyframeInterval(int keyframe_interval);

 private:
  bool CreateEncoder();
  bool ConfigureEncoder();
  bool SetupPlanes();
  bool QueueCaptureBuffers();
  bool StartStreaming();
  void StopStreaming();
  bool QueueOutputBuffer(const uint8_t* src_y,
                         int stride_y,
                         const uint8_t* src_u,
                         int stride_u,
                         const uint8_t* src_v,
                         int stride_v);
  bool QueueOutputBufferNV12(const uint8_t* src_y,
                             int stride_y,
                             const uint8_t* src_uv,
                             int stride_uv);
  bool DequeueCaptureBuffer(std::vector<uint8_t>* encoded, bool* is_keyframe);
  bool DequeueOutputBuffer();
  bool ForceKeyframe();

  static std::optional<std::string> FindEncoderDevice();
  static uint32_t CodecToV4L2PixFmt(JetsonCodec codec);
  static uint32_t CodecToV4L2FallbackPixFmt(JetsonCodec codec);

  JetsonCodec codec_;
  NvVideoEncoder* encoder_ = nullptr;
  bool initialized_ = false;
  bool streaming_ = false;

  int width_ = 0;
  int height_ = 0;
  int framerate_ = 0;
  int bitrate_bps_ = 0;
  int keyframe_interval_ = 0;

  int output_buffer_count_ = 0;
  int capture_buffer_count_ = 0;
  int next_output_index_ = 0;
  int output_y_stride_ = 0;
  int output_u_stride_ = 0;
  int output_v_stride_ = 0;
  bool output_is_nv12_ = false;
};

}  // namespace livekit

#endif  // LIVEKIT_JETSON_MMAPI_ENCODER_H_
