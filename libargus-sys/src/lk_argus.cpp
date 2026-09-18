// Copyright 2026 LiveKit, Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// C shim around NVIDIA libargus for MIPI CSI camera capture on Jetson.
// See lk_argus.h for the ABI and the thread-safety contract.

#include "lk_argus.h"

#include <atomic>
#include <chrono>
#include <cstdarg>
#include <cstdio>
#include <cstring>
#include <mutex>
#include <thread>
#include <vector>

#include <Argus/Argus.h>
#include <Argus/CaptureMetadata.h>
#include <Argus/Event.h>
#include <Argus/EventProvider.h>
#include <Argus/EventQueue.h>
#include <EGLStream/EGLStream.h>
#include <EGLStream/MetadataContainer.h>
#include <EGLStream/NV/ImageNativeBuffer.h>
#include "NvBufSurface.h"

namespace {

constexpr int kDefaultNumDmaBufs = 4;
constexpr int kMinNumDmaBufs = 2;

// ---------------------------------------------------------------------------
// Logging

std::mutex g_log_mutex;
LkArgusLogFn g_log_fn = nullptr;
void* g_log_user_data = nullptr;

#if defined(__GNUC__)
__attribute__((format(printf, 2, 3)))
#endif
void lk_log(int32_t level, const char* fmt, ...) {
  char buf[512];
  va_list args;
  va_start(args, fmt);
  vsnprintf(buf, sizeof(buf), fmt, args);
  va_end(args);

  std::lock_guard<std::mutex> lock(g_log_mutex);
  if (g_log_fn) {
    g_log_fn(level, buf, g_log_user_data);
  } else {
    fprintf(stderr, "[lk_argus] %s\n", buf);
  }
}

// ---------------------------------------------------------------------------
// Process-wide CameraProvider
//
// Argus::CameraProvider is a process singleton in libargus, and repeatedly
// creating/destroying it across sessions is flaky on some JetPack releases.
// Create it lazily on first use and keep it for the life of the process
// (intentional leak); all open sessions and enumeration calls share it.

std::mutex g_provider_mutex;
Argus::CameraProvider* g_provider = nullptr;

// Requires g_provider_mutex to be held.
Argus::ICameraProvider* provider_locked() {
  if (!g_provider) {
    g_provider = Argus::CameraProvider::create();
    if (!g_provider) {
      lk_log(LK_ARGUS_LOG_ERROR,
             "failed to create CameraProvider (is nvargus-daemon running?)");
      return nullptr;
    }
    auto* i_provider = Argus::interface_cast<Argus::ICameraProvider>(g_provider);
    if (i_provider) {
      lk_log(LK_ARGUS_LOG_INFO, "Argus version: %s",
             i_provider->getVersion().c_str());
    }
  }
  return Argus::interface_cast<Argus::ICameraProvider>(g_provider);
}

// Requires g_provider_mutex to be held.
int32_t devices_locked(std::vector<Argus::CameraDevice*>* devices) {
  Argus::ICameraProvider* i_provider = provider_locked();
  if (!i_provider) {
    return LK_ARGUS_ERR_NO_PROVIDER;
  }
  Argus::Status status = i_provider->getCameraDevices(devices);
  if (status != Argus::STATUS_OK) {
    lk_log(LK_ARGUS_LOG_ERROR, "getCameraDevices failed: %d",
           static_cast<int>(status));
    return LK_ARGUS_ERR_ARGUS;
  }
  return LK_ARGUS_OK;
}

// ---------------------------------------------------------------------------
// Sensor timestamp helpers

enum class SensorTimestampStatus {
  Available,
  InvalidArgs,
  NoEventQueue,
  EventWaitFailed,
  NoCaptureCompleteEvent,
  CaptureCompleteFailed,
  NoEventMetadata,
  NoOutputStream,
  MetadataCreateFailed,
  NoCaptureMetadata,
  ZeroTimestamp,
};

const char* sensor_timestamp_status_name(SensorTimestampStatus status) {
  switch (status) {
    case SensorTimestampStatus::Available:
      return "available";
    case SensorTimestampStatus::InvalidArgs:
      return "invalid args";
    case SensorTimestampStatus::NoEventQueue:
      return "no capture-complete event queue";
    case SensorTimestampStatus::EventWaitFailed:
      return "capture-complete event wait failed";
    case SensorTimestampStatus::NoCaptureCompleteEvent:
      return "no capture-complete event";
    case SensorTimestampStatus::CaptureCompleteFailed:
      return "capture-complete event failed";
    case SensorTimestampStatus::NoEventMetadata:
      return "no capture-complete metadata";
    case SensorTimestampStatus::NoOutputStream:
      return "no EGL output stream";
    case SensorTimestampStatus::MetadataCreateFailed:
      return "metadata container create failed";
    case SensorTimestampStatus::NoCaptureMetadata:
      return "no capture metadata interface";
    case SensorTimestampStatus::ZeroTimestamp:
      return "zero sensor timestamp";
  }
  return "unknown";
}

}  // namespace

struct LkArgusSession {
  Argus::UniqueObj<Argus::CaptureSession> session;
  Argus::UniqueObj<Argus::OutputStreamSettings> stream_settings;
  Argus::UniqueObj<Argus::OutputStream> stream;
  Argus::UniqueObj<Argus::Request> request;
  Argus::UniqueObj<Argus::EventQueue> event_queue;
  Argus::UniqueObj<EGLStream::FrameConsumer> consumer;

  // DMA buffer ring. A slot is "leased" from lk_argus_frame_acquire until
  // lk_argus_frame_release; the blit never targets a leased slot, so a
  // frame's fd stays valid however long the consumer holds it.
  int num_dma_bufs = 0;
  int dmabuf_fds[LK_ARGUS_MAX_DMA_BUFS];
  NvBufSurface* dmabuf_surfaces[LK_ARGUS_MAX_DMA_BUFS];
  bool leased[LK_ARGUS_MAX_DMA_BUFS];
  int next_slot = 0;  // ring scan start, only touched by the acquire thread
  std::mutex lease_mutex;

  std::atomic<bool> interrupted{false};
  std::atomic<int32_t> last_argus_status{0};

  int width = 0;
  int height = 0;
  bool metadata_enabled = false;
  bool event_metadata_enabled = false;

  // Log rate-limiting state for sensor timestamp availability (per session,
  // only touched by the acquire thread).
  SensorTimestampStatus last_logged_ts_status = SensorTimestampStatus::Available;
};

namespace {

SensorTimestampStatus read_sensor_timestamp_ns_from_event(
    LkArgusSession* s,
    uint64_t* sensor_timestamp_ns,
    Argus::Status* metadata_status) {
  if (metadata_status) *metadata_status = Argus::STATUS_OK;
  if (!s || !sensor_timestamp_ns) return SensorTimestampStatus::InvalidArgs;
  *sensor_timestamp_ns = 0;

  auto* i_event_provider =
      Argus::interface_cast<Argus::IEventProvider>(s->session);
  auto* i_event_queue = Argus::interface_cast<Argus::IEventQueue>(s->event_queue);
  if (!i_event_provider || !i_event_queue) {
    return SensorTimestampStatus::NoEventQueue;
  }

  Argus::Status status =
      i_event_provider->waitForEvents(s->event_queue.get(), 1000000);
  if (metadata_status) *metadata_status = status;
  if (status != Argus::STATUS_OK) {
    return SensorTimestampStatus::EventWaitFailed;
  }

  const Argus::Event* newest_capture_complete = nullptr;
  for (uint32_t i = 0; i < i_event_queue->getSize(); i++) {
    const Argus::Event* event = i_event_queue->getEvent(i);
    auto* i_event = Argus::interface_cast<const Argus::IEvent>(event);
    if (i_event && i_event->getEventType() == Argus::EVENT_TYPE_CAPTURE_COMPLETE) {
      newest_capture_complete = event;
    }
  }
  if (!newest_capture_complete) {
    return SensorTimestampStatus::NoCaptureCompleteEvent;
  }

  auto* i_capture_complete =
      Argus::interface_cast<const Argus::IEventCaptureComplete>(
          newest_capture_complete);
  if (!i_capture_complete) {
    return SensorTimestampStatus::NoCaptureCompleteEvent;
  }
  status = i_capture_complete->getStatus();
  if (metadata_status) *metadata_status = status;
  if (status != Argus::STATUS_OK) {
    return SensorTimestampStatus::CaptureCompleteFailed;
  }

  const Argus::CaptureMetadata* metadata = i_capture_complete->getMetadata();
  if (!metadata) {
    return SensorTimestampStatus::NoEventMetadata;
  }

  auto* i_metadata =
      Argus::interface_cast<const Argus::ICaptureMetadata>(metadata);
  if (!i_metadata) {
    return SensorTimestampStatus::NoCaptureMetadata;
  }

  *sensor_timestamp_ns = i_metadata->getSensorTimestamp();
  if (*sensor_timestamp_ns == 0) {
    return SensorTimestampStatus::ZeroTimestamp;
  }
  return SensorTimestampStatus::Available;
}

SensorTimestampStatus read_sensor_timestamp_ns_from_egl_metadata(
    LkArgusSession* s,
    uint64_t* sensor_timestamp_ns,
    Argus::Status* metadata_status) {
  if (metadata_status) *metadata_status = Argus::STATUS_OK;
  if (!s || !sensor_timestamp_ns) return SensorTimestampStatus::InvalidArgs;
  *sensor_timestamp_ns = 0;

  auto* i_stream = Argus::interface_cast<Argus::IEGLOutputStream>(s->stream);
  if (!i_stream) return SensorTimestampStatus::NoOutputStream;

  Argus::Status status;
  EGLStream::MetadataContainer* metadata = EGLStream::MetadataContainer::create(
      i_stream->getEGLDisplay(), i_stream->getEGLStream(),
      EGLStream::MetadataContainer::CONSUMER, &status);
  if (metadata_status) *metadata_status = status;
  if (status != Argus::STATUS_OK || !metadata) {
    return SensorTimestampStatus::MetadataCreateFailed;
  }

  auto* i_metadata = Argus::interface_cast<Argus::ICaptureMetadata>(metadata);
  if (!i_metadata) {
    metadata->destroy();
    return SensorTimestampStatus::NoCaptureMetadata;
  }

  *sensor_timestamp_ns = i_metadata->getSensorTimestamp();
  metadata->destroy();
  if (*sensor_timestamp_ns == 0) {
    return SensorTimestampStatus::ZeroTimestamp;
  }
  return SensorTimestampStatus::Available;
}

SensorTimestampStatus read_sensor_timestamp_ns(LkArgusSession* s,
                                               uint64_t* sensor_timestamp_ns,
                                               Argus::Status* metadata_status) {
  SensorTimestampStatus status = read_sensor_timestamp_ns_from_egl_metadata(
      s, sensor_timestamp_ns, metadata_status);
  if (status == SensorTimestampStatus::Available) {
    return status;
  }

  // Fall back to capture-complete events only when embedded EGLStream
  // metadata is unavailable. Event queues are session-scoped, so they can lag
  // or lead the exact frame returned by FrameConsumer::acquireFrame().
  SensorTimestampStatus egl_status = status;
  Argus::Status egl_metadata_status =
      metadata_status ? *metadata_status : Argus::STATUS_OK;

  SensorTimestampStatus event_status =
      read_sensor_timestamp_ns_from_event(s, sensor_timestamp_ns, metadata_status);
  if (event_status == SensorTimestampStatus::Available) {
    return event_status;
  }

  if (metadata_status) *metadata_status = egl_metadata_status;
  return egl_status;
}

// Destroys the persistent NvBufSurface ring entries [0, count), releasing
// their DMA-BUF fds. Entries that were never created (nullptr) are skipped,
// so this is safe on a partially-initialized session.
void destroy_dmabuf_surfaces(LkArgusSession* s, int count) {
  for (int i = 0; i < count; i++) {
    if (s->dmabuf_surfaces[i]) {
      NvBufSurfaceDestroy(s->dmabuf_surfaces[i]);
      s->dmabuf_surfaces[i] = nullptr;
    }
    s->dmabuf_fds[i] = -1;
  }
}

}  // namespace

extern "C" {

int32_t lk_argus_set_logger(LkArgusLogFn log_fn, void* user_data) {
  std::lock_guard<std::mutex> lock(g_log_mutex);
  g_log_fn = log_fn;
  g_log_user_data = user_data;
  return LK_ARGUS_OK;
}

int32_t lk_argus_version(char* buf, size_t buf_len) {
  if (!buf || buf_len == 0) return LK_ARGUS_ERR_INVALID_ARG;
  std::lock_guard<std::mutex> lock(g_provider_mutex);
  Argus::ICameraProvider* i_provider = provider_locked();
  if (!i_provider) return LK_ARGUS_ERR_NO_PROVIDER;
  snprintf(buf, buf_len, "%s", i_provider->getVersion().c_str());
  return LK_ARGUS_OK;
}

int32_t lk_argus_device_count(void) {
  std::lock_guard<std::mutex> lock(g_provider_mutex);
  std::vector<Argus::CameraDevice*> devices;
  int32_t status = devices_locked(&devices);
  if (status != LK_ARGUS_OK) return status;
  return static_cast<int32_t>(devices.size());
}

int32_t lk_argus_device_info(int32_t device_index, LkArgusDeviceInfo* out) {
  if (!out || device_index < 0) return LK_ARGUS_ERR_INVALID_ARG;
  memset(out, 0, sizeof(*out));

  std::lock_guard<std::mutex> lock(g_provider_mutex);
  std::vector<Argus::CameraDevice*> devices;
  int32_t status = devices_locked(&devices);
  if (status != LK_ARGUS_OK) return status;
  if (device_index >= static_cast<int32_t>(devices.size())) {
    return LK_ARGUS_ERR_NO_DEVICE;
  }

  auto* i_props =
      Argus::interface_cast<Argus::ICameraProperties>(devices[device_index]);
  if (!i_props) return LK_ARGUS_ERR_ARGUS;

  const Argus::UUID uuid = i_props->getUUID();
  snprintf(out->uuid, sizeof(out->uuid),
           "%08x-%04x-%04x-%04x-%02x%02x%02x%02x%02x%02x",
           uuid.time_low, uuid.time_mid, uuid.time_hi_and_version,
           uuid.clock_seq, uuid.node[0], uuid.node[1], uuid.node[2],
           uuid.node[3], uuid.node[4], uuid.node[5]);

  // A human-readable module name is only exposed through version-specific
  // extension interfaces; leave `name` empty and let callers synthesize one.

  std::vector<Argus::SensorMode*> modes;
  if (i_props->getAllSensorModes(&modes) == Argus::STATUS_OK) {
    out->sensor_mode_count = static_cast<int32_t>(modes.size());
  }
  return LK_ARGUS_OK;
}

int32_t lk_argus_sensor_mode_info(int32_t device_index,
                                  int32_t mode_index,
                                  LkArgusSensorModeInfo* out) {
  if (!out || device_index < 0 || mode_index < 0) {
    return LK_ARGUS_ERR_INVALID_ARG;
  }
  memset(out, 0, sizeof(*out));

  std::lock_guard<std::mutex> lock(g_provider_mutex);
  std::vector<Argus::CameraDevice*> devices;
  int32_t status = devices_locked(&devices);
  if (status != LK_ARGUS_OK) return status;
  if (device_index >= static_cast<int32_t>(devices.size())) {
    return LK_ARGUS_ERR_NO_DEVICE;
  }

  auto* i_props =
      Argus::interface_cast<Argus::ICameraProperties>(devices[device_index]);
  if (!i_props) return LK_ARGUS_ERR_ARGUS;

  std::vector<Argus::SensorMode*> modes;
  if (i_props->getAllSensorModes(&modes) != Argus::STATUS_OK ||
      mode_index >= static_cast<int32_t>(modes.size())) {
    return LK_ARGUS_ERR_INVALID_ARG;
  }

  auto* i_mode = Argus::interface_cast<Argus::ISensorMode>(modes[mode_index]);
  if (!i_mode) return LK_ARGUS_ERR_ARGUS;

  const Argus::Size2D<uint32_t> res = i_mode->getResolution();
  const Argus::Range<uint64_t> dur = i_mode->getFrameDurationRange();
  out->width = res.width();
  out->height = res.height();
  out->min_frame_duration_ns = dur.min();
  out->max_frame_duration_ns = dur.max();
  out->bit_depth = i_mode->getInputBitDepth();
  return LK_ARGUS_OK;
}

int32_t lk_argus_session_create(const LkArgusSessionConfig* config,
                                LkArgusSession** out_session) {
  if (!config || !out_session || config->width <= 0 || config->height <= 0 ||
      config->fps <= 0 || config->device_index < 0 ||
      config->num_dma_bufs < 0) {
    return LK_ARGUS_ERR_INVALID_ARG;
  }
  *out_session = nullptr;

  const int width = config->width;
  const int height = config->height;
  const int fps = config->fps;
  int num_dma_bufs =
      config->num_dma_bufs == 0 ? kDefaultNumDmaBufs : config->num_dma_bufs;
  if (num_dma_bufs < kMinNumDmaBufs) num_dma_bufs = kMinNumDmaBufs;
  if (num_dma_bufs > LK_ARGUS_MAX_DMA_BUFS) num_dma_bufs = LK_ARGUS_MAX_DMA_BUFS;

  auto* s = new LkArgusSession();
  s->num_dma_bufs = num_dma_bufs;
  for (int i = 0; i < LK_ARGUS_MAX_DMA_BUFS; i++) {
    s->dmabuf_fds[i] = -1;
    s->dmabuf_surfaces[i] = nullptr;
    s->leased[i] = false;
  }
  s->width = width;
  s->height = height;

  Argus::CameraDevice* device = nullptr;
  Argus::Status status;
  {
    std::lock_guard<std::mutex> lock(g_provider_mutex);
    Argus::ICameraProvider* i_provider = provider_locked();
    if (!i_provider) {
      delete s;
      return LK_ARGUS_ERR_NO_PROVIDER;
    }

    std::vector<Argus::CameraDevice*> devices;
    i_provider->getCameraDevices(&devices);
    if (config->device_index >= static_cast<int32_t>(devices.size())) {
      lk_log(LK_ARGUS_LOG_ERROR, "no camera device at index %d (found %zu)",
             config->device_index, devices.size());
      delete s;
      return LK_ARGUS_ERR_NO_DEVICE;
    }
    device = devices[config->device_index];

    s->session = Argus::UniqueObj<Argus::CaptureSession>(
        i_provider->createCaptureSession(device, &status));
  }
  if (status != Argus::STATUS_OK) {
    lk_log(LK_ARGUS_LOG_ERROR, "failed to create CaptureSession: %d",
           static_cast<int>(status));
    s->last_argus_status = static_cast<int32_t>(status);
    delete s;
    return LK_ARGUS_ERR_ARGUS;
  }
  auto* i_session = Argus::interface_cast<Argus::ICaptureSession>(s->session);
  if (!i_session) {
    delete s;
    return LK_ARGUS_ERR_ARGUS;
  }

  // Capture-complete event queue (fallback source for sensor timestamps).
  auto* i_event_provider =
      Argus::interface_cast<Argus::IEventProvider>(s->session);
  if (i_event_provider) {
    std::vector<Argus::EventType> event_types;
    event_types.push_back(Argus::EVENT_TYPE_CAPTURE_COMPLETE);
    s->event_queue = Argus::UniqueObj<Argus::EventQueue>(
        i_event_provider->createEventQueue(event_types, &status));
    if (status != Argus::STATUS_OK || !s->event_queue) {
      lk_log(LK_ARGUS_LOG_WARN,
             "failed to create capture-complete event queue: %d",
             static_cast<int>(status));
    } else {
      s->event_metadata_enabled = true;
    }
  } else {
    lk_log(LK_ARGUS_LOG_WARN,
           "capture session has no event provider interface");
  }

  // EGLStream-backed OutputStream delivering ISP-scaled NV12.
  s->stream_settings = Argus::UniqueObj<Argus::OutputStreamSettings>(
      i_session->createOutputStreamSettings(Argus::STREAM_TYPE_EGL, &status));
  auto* i_stream_settings =
      Argus::interface_cast<Argus::IEGLOutputStreamSettings>(s->stream_settings);
  if (!i_stream_settings) {
    lk_log(LK_ARGUS_LOG_ERROR, "failed to get IEGLOutputStreamSettings");
    delete s;
    return LK_ARGUS_ERR_ARGUS;
  }
  i_stream_settings->setPixelFormat(Argus::PIXEL_FMT_YCbCr_420_888);
  i_stream_settings->setResolution(Argus::Size2D<uint32_t>(width, height));
  status = i_stream_settings->setMode(Argus::EGL_STREAM_MODE_MAILBOX);
  if (status != Argus::STATUS_OK) {
    lk_log(LK_ARGUS_LOG_WARN, "failed to set EGLStream mailbox mode: %d",
           static_cast<int>(status));
  }
  status = i_stream_settings->setFifoLength(1);
  if (status != Argus::STATUS_OK) {
    lk_log(LK_ARGUS_LOG_WARN, "failed to set EGLStream FIFO length: %d",
           static_cast<int>(status));
  }
  status = i_stream_settings->setMetadataEnable(true);
  if (status != Argus::STATUS_OK) {
    lk_log(LK_ARGUS_LOG_WARN, "failed to enable EGLStream metadata: %d",
           static_cast<int>(status));
  }
  s->metadata_enabled = i_stream_settings->getMetadataEnable();

  s->stream = Argus::UniqueObj<Argus::OutputStream>(
      i_session->createOutputStream(s->stream_settings.get(), &status));
  if (status != Argus::STATUS_OK) {
    lk_log(LK_ARGUS_LOG_ERROR, "failed to create OutputStream: %d",
           static_cast<int>(status));
    s->last_argus_status = static_cast<int32_t>(status);
    delete s;
    return LK_ARGUS_ERR_ARGUS;
  }

  s->consumer = Argus::UniqueObj<EGLStream::FrameConsumer>(
      EGLStream::FrameConsumer::create(s->stream.get()));
  if (!Argus::interface_cast<EGLStream::IFrameConsumer>(s->consumer)) {
    lk_log(LK_ARGUS_LOG_ERROR, "failed to create FrameConsumer");
    delete s;
    return LK_ARGUS_ERR_ARGUS;
  }

  s->request = Argus::UniqueObj<Argus::Request>(
      i_session->createRequest(Argus::CAPTURE_INTENT_VIDEO_RECORD, &status));
  if (status != Argus::STATUS_OK) {
    lk_log(LK_ARGUS_LOG_ERROR, "failed to create Request: %d",
           static_cast<int>(status));
    s->last_argus_status = static_cast<int32_t>(status);
    delete s;
    return LK_ARGUS_ERR_ARGUS;
  }
  auto* i_request = Argus::interface_cast<Argus::IRequest>(s->request);
  i_request->enableOutputStream(s->stream.get());

  // Sensor mode: use the explicitly requested mode, or auto-select the
  // smallest mode that covers the requested resolution and frame rate
  // (Argus's own auto-selection often picks the highest-resolution mode and
  // runs at that mode's lower frame rate).
  auto* i_props = Argus::interface_cast<Argus::ICameraProperties>(device);
  auto* i_source =
      Argus::interface_cast<Argus::ISourceSettings>(i_request->getSourceSettings());
  const uint64_t requested_dur_ns = 1000000000ULL / fps;
  if (i_props) {
    std::vector<Argus::SensorMode*> modes;
    i_props->getAllSensorModes(&modes);

    Argus::SensorMode* selected = nullptr;
    if (config->sensor_mode_index >= 0) {
      if (config->sensor_mode_index >= static_cast<int32_t>(modes.size())) {
        lk_log(LK_ARGUS_LOG_ERROR, "sensor mode index %d out of range (%zu)",
               config->sensor_mode_index, modes.size());
        delete s;
        return LK_ARGUS_ERR_INVALID_ARG;
      }
      selected = modes[config->sensor_mode_index];
    } else {
      uint64_t best_pixels = UINT64_MAX;
      for (size_t i = 0; i < modes.size(); i++) {
        auto* i_mode = Argus::interface_cast<Argus::ISensorMode>(modes[i]);
        if (!i_mode) continue;
        const Argus::Size2D<uint32_t> res = i_mode->getResolution();
        const Argus::Range<uint64_t> dur = i_mode->getFrameDurationRange();
        // Compare frame durations instead of floating-point fps. Sensor
        // durations are in nanoseconds and often off by 1 ns from the ideal
        // value (e.g. 33333334 vs 33333333 for 30 fps); a 1 ms tolerance
        // handles this rounding.
        if (static_cast<int>(res.width()) >= width &&
            static_cast<int>(res.height()) >= height &&
            dur.min() <= requested_dur_ns + 1000000) {
          const uint64_t pixels =
              static_cast<uint64_t>(res.width()) * res.height();
          if (pixels < best_pixels) {
            best_pixels = pixels;
            selected = modes[i];
          }
        }
      }
    }

    if (selected) {
      auto* i_selected = Argus::interface_cast<Argus::ISensorMode>(selected);
      const Argus::Size2D<uint32_t> res = i_selected->getResolution();
      lk_log(LK_ARGUS_LOG_INFO, "selected sensor mode %ux%u for %dx%d @ %d fps",
             res.width(), res.height(), width, height, fps);
      if (i_source) {
        i_source->setSensorMode(selected);
      }
    } else {
      lk_log(LK_ARGUS_LOG_WARN,
             "no sensor mode found for %dx%d @ %d fps, using Argus default",
             width, height, fps);
    }
  } else {
    lk_log(LK_ARGUS_LOG_WARN, "could not query sensor modes");
  }
  if (i_source) {
    // Fix the frame duration and cap exposure so auto-exposure can never
    // stretch the frame interval below the requested rate.
    i_source->setFrameDurationRange(
        Argus::Range<uint64_t>(requested_dur_ns, requested_dur_ns));
    i_source->setExposureTimeRange(Argus::Range<uint64_t>(0, requested_dur_ns));
  }

  // Persistent NvBufSurface ring the acquired frames are blitted into.
  for (int i = 0; i < num_dma_bufs; i++) {
    NvBufSurfaceCreateParams create_params = {};
    create_params.gpuId = 0;
    create_params.width = static_cast<uint32_t>(width);
    create_params.height = static_cast<uint32_t>(height);
    create_params.size = 0;
    create_params.colorFormat = NVBUF_COLOR_FORMAT_NV12;
    create_params.layout = NVBUF_LAYOUT_PITCH;
    create_params.memType = NVBUF_MEM_SURFACE_ARRAY;

    NvBufSurface* surface = nullptr;
    if (NvBufSurfaceCreate(&surface, 1, &create_params) != 0 || !surface) {
      lk_log(LK_ARGUS_LOG_ERROR, "failed to create NvBufSurface[%d]", i);
      destroy_dmabuf_surfaces(s, i);
      delete s;
      return LK_ARGUS_ERR_NVBUF;
    }
    surface->numFilled = 1;
    s->dmabuf_fds[i] = surface->surfaceList[0].bufferDesc;
    s->dmabuf_surfaces[i] = surface;
  }

  status = i_session->repeat(s->request.get());
  if (status != Argus::STATUS_OK) {
    lk_log(LK_ARGUS_LOG_ERROR, "failed to start repeating capture: %d",
           static_cast<int>(status));
    s->last_argus_status = static_cast<int32_t>(status);
    destroy_dmabuf_surfaces(s, num_dma_bufs);
    delete s;
    return LK_ARGUS_ERR_ARGUS;
  }

  lk_log(LK_ARGUS_LOG_INFO,
         "session created: %dx%d @ %d fps, device %d, %d DMA buffers", width,
         height, fps, config->device_index, num_dma_bufs);
  *out_session = s;
  return LK_ARGUS_OK;
}

int32_t lk_argus_session_interrupt(LkArgusSession* s) {
  if (!s) return LK_ARGUS_ERR_INVALID_ARG;
  s->interrupted = true;
  return LK_ARGUS_OK;
}

int32_t lk_argus_session_last_argus_status(const LkArgusSession* s) {
  if (!s) return 0;
  return s->last_argus_status.load();
}

int32_t lk_argus_frame_acquire(LkArgusSession* s,
                               uint64_t timeout_ns,
                               LkArgusFrame* out) {
  using Clock = std::chrono::steady_clock;

  if (!s || !out) return LK_ARGUS_ERR_INVALID_ARG;
  memset(out, 0, sizeof(*out));
  out->dmabuf_fd = -1;
  out->buffer_index = -1;

  if (s->interrupted.load()) return LK_ARGUS_ERR_INTERRUPTED;

  auto* i_consumer =
      Argus::interface_cast<EGLStream::IFrameConsumer>(s->consumer);
  if (!i_consumer) return LK_ARGUS_ERR_ARGUS;

  // Reserve a free ring slot before consuming a frame so backpressure is
  // visible without discarding sensor output.
  int slot = -1;
  {
    std::lock_guard<std::mutex> lock(s->lease_mutex);
    for (int i = 0; i < s->num_dma_bufs; i++) {
      const int candidate = (s->next_slot + i) % s->num_dma_bufs;
      if (!s->leased[candidate]) {
        slot = candidate;
        break;
      }
    }
  }
  if (slot < 0) return LK_ARGUS_ERR_NO_FREE_BUFFER;

  const auto t0 = Clock::now();

  Argus::Status status;
  Argus::UniqueObj<EGLStream::Frame> frame(
      i_consumer->acquireFrame(timeout_ns, &status));
  if (s->interrupted.load()) return LK_ARGUS_ERR_INTERRUPTED;
  if (status == Argus::STATUS_TIMEOUT) return LK_ARGUS_ERR_TIMEOUT;
  if (status == Argus::STATUS_DISCONNECTED) {
    lk_log(LK_ARGUS_LOG_ERROR, "EGLStream disconnected");
    return LK_ARGUS_ERR_DISCONNECTED;
  }
  if (status != Argus::STATUS_OK || !frame) {
    s->last_argus_status = static_cast<int32_t>(status);
    lk_log(LK_ARGUS_LOG_ERROR, "acquireFrame failed: %d",
           static_cast<int>(status));
    return LK_ARGUS_ERR_ARGUS;
  }

  const auto t1 = Clock::now();

  auto* i_frame = Argus::interface_cast<EGLStream::IFrame>(frame);
  if (!i_frame) return LK_ARGUS_ERR_ARGUS;

  uint64_t sensor_timestamp_ns = 0;
  Argus::Status metadata_status = Argus::STATUS_OK;
  const SensorTimestampStatus ts_status =
      read_sensor_timestamp_ns(s, &sensor_timestamp_ns, &metadata_status);
  const bool has_sensor_timestamp = ts_status == SensorTimestampStatus::Available;
  if (!has_sensor_timestamp && ts_status != s->last_logged_ts_status) {
    lk_log(LK_ARGUS_LOG_WARN,
           "sensor timestamp unavailable: %s (event metadata=%s, EGL "
           "metadata=%s, status=%d)",
           sensor_timestamp_status_name(ts_status),
           s->event_metadata_enabled ? "yes" : "no",
           s->metadata_enabled ? "yes" : "no",
           static_cast<int>(metadata_status));
    s->last_logged_ts_status = ts_status;
  } else if (has_sensor_timestamp &&
             s->last_logged_ts_status != SensorTimestampStatus::Available) {
    lk_log(LK_ARGUS_LOG_INFO, "sensor timestamp available");
    s->last_logged_ts_status = SensorTimestampStatus::Available;
  }

  EGLStream::Image* image = i_frame->getImage();
  if (!image) return LK_ARGUS_ERR_ARGUS;

  auto* i_native =
      Argus::interface_cast<EGLStream::NV::IImageNativeBuffer>(image);
  if (!i_native) {
    lk_log(LK_ARGUS_LOG_ERROR, "image does not support IImageNativeBuffer");
    return LK_ARGUS_ERR_ARGUS;
  }

  // Blit (VIC) the acquired frame into the reserved slot. The EGLStream
  // frame is released when `frame` goes out of scope; the pixel data lives
  // on in the persistent NvBufSurface.
  const int fd = s->dmabuf_fds[slot];
  status = i_native->copyToNvBuffer(fd);

  const auto t2 = Clock::now();

  if (status != Argus::STATUS_OK) {
    s->last_argus_status = static_cast<int32_t>(status);
    lk_log(LK_ARGUS_LOG_ERROR, "copyToNvBuffer failed: %d",
           static_cast<int>(status));
    return LK_ARGUS_ERR_ARGUS;
  }

  {
    std::lock_guard<std::mutex> lock(s->lease_mutex);
    s->leased[slot] = true;
  }
  s->next_slot = (slot + 1) % s->num_dma_bufs;

  const NvBufSurfaceParams& params = s->dmabuf_surfaces[slot]->surfaceList[0];
  out->dmabuf_fd = fd;
  out->buffer_index = slot;
  out->width = static_cast<uint32_t>(s->width);
  out->height = static_cast<uint32_t>(s->height);
  out->pitch[0] = params.planeParams.pitch[0];
  out->pitch[1] = params.planeParams.pitch[1];
  out->offset[0] = params.planeParams.offset[0];
  out->offset[1] = params.planeParams.offset[1];
  out->sensor_timestamp_ns = sensor_timestamp_ns;
  out->acquire_wait_ns = static_cast<uint64_t>(
      std::chrono::duration_cast<std::chrono::nanoseconds>(t1 - t0).count());
  out->blit_ns = static_cast<uint64_t>(
      std::chrono::duration_cast<std::chrono::nanoseconds>(t2 - t1).count());
  return LK_ARGUS_OK;
}

int32_t lk_argus_frame_release(LkArgusSession* s, int32_t buffer_index) {
  if (!s || buffer_index < 0 || buffer_index >= s->num_dma_bufs) {
    return LK_ARGUS_ERR_INVALID_ARG;
  }
  std::lock_guard<std::mutex> lock(s->lease_mutex);
  s->leased[buffer_index] = false;
  return LK_ARGUS_OK;
}

int32_t lk_argus_frame_copy_to_i420(LkArgusSession* s,
                                    int32_t buffer_index,
                                    uint8_t* dst_y,
                                    int32_t dst_stride_y,
                                    uint8_t* dst_u,
                                    int32_t dst_stride_u,
                                    uint8_t* dst_v,
                                    int32_t dst_stride_v) {
  if (!s || buffer_index < 0 || buffer_index >= s->num_dma_bufs || !dst_y ||
      !dst_u || !dst_v) {
    return LK_ARGUS_ERR_INVALID_ARG;
  }

  const int width = s->width;
  const int height = s->height;
  const int chroma_width = (width + 1) / 2;
  const int chroma_height = (height + 1) / 2;
  if (dst_stride_y < width || dst_stride_u < chroma_width ||
      dst_stride_v < chroma_width) {
    return LK_ARGUS_ERR_INVALID_ARG;
  }

  NvBufSurface* surface = s->dmabuf_surfaces[buffer_index];
  if (!surface || surface->batchSize < 1) {
    return LK_ARGUS_ERR_NVBUF;
  }

  int ret = NvBufSurfaceMap(surface, 0, -1, NVBUF_MAP_READ);
  if (ret != 0) {
    lk_log(LK_ARGUS_LOG_ERROR, "NvBufSurfaceMap failed: %d", ret);
    return LK_ARGUS_ERR_NVBUF;
  }

  ret = NvBufSurfaceSyncForCpu(surface, 0, -1);
  if (ret != 0) {
    NvBufSurfaceUnMap(surface, 0, -1);
    lk_log(LK_ARGUS_LOG_ERROR, "NvBufSurfaceSyncForCpu failed: %d", ret);
    return LK_ARGUS_ERR_NVBUF;
  }

  const NvBufSurfaceParams& params = surface->surfaceList[0];
  const uint8_t* src_y = static_cast<const uint8_t*>(params.mappedAddr.addr[0]);
  const uint8_t* src_uv = static_cast<const uint8_t*>(params.mappedAddr.addr[1]);
  const int src_stride_y = static_cast<int>(params.planeParams.pitch[0]);
  const int src_stride_uv = static_cast<int>(params.planeParams.pitch[1]);

  if (!src_y || !src_uv || src_stride_y < width ||
      src_stride_uv < chroma_width * 2) {
    NvBufSurfaceUnMap(surface, 0, -1);
    return LK_ARGUS_ERR_NVBUF;
  }

  for (int row = 0; row < height; row++) {
    memcpy(dst_y + row * dst_stride_y, src_y + row * src_stride_y,
           static_cast<size_t>(width));
  }
  for (int row = 0; row < chroma_height; row++) {
    const uint8_t* src_row = src_uv + row * src_stride_uv;
    uint8_t* dst_u_row = dst_u + row * dst_stride_u;
    uint8_t* dst_v_row = dst_v + row * dst_stride_v;
    for (int col = 0; col < chroma_width; col++) {
      dst_u_row[col] = src_row[col * 2];
      dst_v_row[col] = src_row[col * 2 + 1];
    }
  }

  ret = NvBufSurfaceUnMap(surface, 0, -1);
  if (ret != 0) {
    lk_log(LK_ARGUS_LOG_ERROR, "NvBufSurfaceUnMap failed: %d", ret);
    return LK_ARGUS_ERR_NVBUF;
  }
  return LK_ARGUS_OK;
}

void lk_argus_session_destroy(LkArgusSession* s) {
  if (!s) return;

  s->interrupted = true;

  auto* i_session = Argus::interface_cast<Argus::ICaptureSession>(s->session);
  if (i_session) {
    i_session->stopRepeat();
    i_session->waitForIdle();
  }

  // Callers are expected to release every frame before destroying the
  // session (the Rust wrapper keeps the session alive until they do); this
  // bounded wait is defense in depth against a misbehaving consumer.
  constexpr int kMaxWaitMs = 500;
  constexpr int kPollMs = 10;
  for (int waited_ms = 0; waited_ms < kMaxWaitMs; waited_ms += kPollMs) {
    bool any_leased = false;
    {
      std::lock_guard<std::mutex> lock(s->lease_mutex);
      for (int i = 0; i < s->num_dma_bufs; i++) {
        any_leased |= s->leased[i];
      }
    }
    if (!any_leased) break;
    std::this_thread::sleep_for(std::chrono::milliseconds(kPollMs));
  }
  {
    std::lock_guard<std::mutex> lock(s->lease_mutex);
    for (int i = 0; i < s->num_dma_bufs; i++) {
      if (s->leased[i]) {
        lk_log(LK_ARGUS_LOG_ERROR,
               "destroying session with ring slot %d still leased", i);
      }
    }
  }

  destroy_dmabuf_surfaces(s, s->num_dma_bufs);
  delete s;
  lk_log(LK_ARGUS_LOG_INFO, "session destroyed");
}

}  // extern "C"
