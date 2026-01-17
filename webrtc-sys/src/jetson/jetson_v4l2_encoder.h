#ifndef LIVEKIT_JETSON_V4L2_ENCODER_H_
#define LIVEKIT_JETSON_V4L2_ENCODER_H_

#include <linux/videodev2.h>

#include <optional>
#include <string>
#include <vector>

namespace livekit {

enum class JetsonCodec { kH264, kH265 };

class JetsonV4L2Encoder {
 public:
  explicit JetsonV4L2Encoder(JetsonCodec codec);
  ~JetsonV4L2Encoder();

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
              const uint8_t* src_uv,
              int stride_uv,
              bool force_keyframe,
              std::vector<uint8_t>* encoded,
              bool* is_keyframe);

  void SetRates(int framerate, int bitrate_bps);
  void SetKeyframeInterval(int keyframe_interval);

 private:
  struct PlaneBuffer {
    void* start = nullptr;
    size_t length = 0;
  };

  struct MMapBuffer {
    std::vector<PlaneBuffer> planes;
  };

  bool OpenDevice();
  bool ConfigureFormats();
  bool ConfigureControls();
  bool SetupBuffers();
  bool QueueCaptureBuffers();
  bool StartStreaming();
  void StopStreaming();
  bool QueueOutputBuffer(int index,
                         const uint8_t* src_y,
                         int stride_y,
                         const uint8_t* src_uv,
                         int stride_uv);
  bool DequeueCaptureBuffer(std::vector<uint8_t>* encoded,
                            bool* is_keyframe);
  void DequeueOutputBuffer();

  bool SetControl(uint32_t id, int32_t value);
  bool SetStreamParam(int framerate);

  static std::optional<std::string> FindEncoderDevice(JetsonCodec codec);
  static bool DeviceSupportsCodec(int fd, JetsonCodec codec);

  JetsonCodec codec_;
  std::string device_path_;
  int fd_ = -1;
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

  std::vector<MMapBuffer> output_buffers_;
  std::vector<MMapBuffer> capture_buffers_;
};

}  // namespace livekit

#endif  // LIVEKIT_JETSON_V4L2_ENCODER_H_
