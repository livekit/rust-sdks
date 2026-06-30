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
//
// Exposes a simple C API for the Rust FFI in argus.rs:
//   lk_argus_create_session  - open sensor, configure ISP, start repeating capture
//   lk_argus_acquire_frame   - dequeue next frame, return NvBufSurface DMA fd
//   lk_argus_release_frame   - release frame back to Argus buffer pool
//   lk_argus_destroy_session - tear down everything

#include <chrono>
#include <cstdio>
#include <cstdint>
#include <cstdlib>
#include <cstring>

#include <Argus/Argus.h>
#include <Argus/CaptureMetadata.h>
#include <Argus/Event.h>
#include <Argus/EventProvider.h>
#include <Argus/EventQueue.h>
#include <EGLStream/EGLStream.h>
#include <EGLStream/MetadataContainer.h>
#include <EGLStream/NV/ImageNativeBuffer.h>
#include "NvBufSurface.h"

// Ring buffer size for persistent NvBufSurface DMA allocations.
// The encoder may hold 1-2 buffers while encoding, and the blit writes to
// another.  4 buffers gives comfortable headroom to avoid the "Wrong buffer
// index" errors that occur when the capture loop laps the encoder.
static constexpr int kNumDmaBufs = 4;

struct LkArgusSession {
    Argus::UniqueObj<Argus::CameraProvider> provider;
    Argus::UniqueObj<Argus::CaptureSession>  session;
    Argus::UniqueObj<Argus::OutputStreamSettings> stream_settings;
    Argus::UniqueObj<Argus::OutputStream>    stream;
    Argus::UniqueObj<Argus::Request>         request;
    Argus::UniqueObj<Argus::EventQueue>      event_queue;
    Argus::UniqueObj<EGLStream::FrameConsumer> consumer;

    // Most recently acquired frame (kept alive until release/next acquire).
    Argus::UniqueObj<EGLStream::Frame> current_frame;

    // Ring of DMA fds so the encoder can hold one buffer while we blit the
    // next frame into a different one.  Avoids the "Wrong buffer index"
    // errors caused by the encoder and Argus racing on a single buffer.
    int dmabuf_fds[kNumDmaBufs];
    NvBufSurface* dmabuf_surfaces[kNumDmaBufs];  // original surface ptrs for cleanup
    int dmabuf_write_idx;  // next buffer to blit into
    int width;
    int height;
    bool metadata_enabled;
    bool event_metadata_enabled;
};

static const uint64_t kAcquireTimeoutNs = 1000000000ULL; // 1 second

static constexpr int kCopyI420InvalidArgument = -1;
static constexpr int kCopyI420SurfaceNotFound = -2;
static constexpr int kCopyI420InvalidSurface = -4;

static int copy_i420_error_code(int ret) {
    return ret < 0 ? -ret : ret;
}

static int copy_i420_map_error(int ret) {
    return -1000 - copy_i420_error_code(ret);
}

static int copy_i420_sync_error(int ret) {
    return -2000 - copy_i420_error_code(ret);
}

static int copy_i420_unmap_error(int ret) {
    return -100 - copy_i420_error_code(ret);
}

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

static const char* sensor_timestamp_status_name(SensorTimestampStatus status) {
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

static SensorTimestampStatus read_sensor_timestamp_ns_from_event(
        LkArgusSession* s,
        uint64_t* sensor_timestamp_ns,
        Argus::Status* metadata_status) {
    if (metadata_status) *metadata_status = Argus::STATUS_OK;
    if (!s || !sensor_timestamp_ns) return SensorTimestampStatus::InvalidArgs;
    *sensor_timestamp_ns = 0;

    auto* i_event_provider = Argus::interface_cast<Argus::IEventProvider>(s->session);
    auto* i_event_queue = Argus::interface_cast<Argus::IEventQueue>(s->event_queue);
    if (!i_event_provider || !i_event_queue) {
        return SensorTimestampStatus::NoEventQueue;
    }

    Argus::Status status = i_event_provider->waitForEvents(s->event_queue.get(), 1000000);
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
        Argus::interface_cast<const Argus::IEventCaptureComplete>(newest_capture_complete);
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

    auto* i_metadata = Argus::interface_cast<const Argus::ICaptureMetadata>(metadata);
    if (!i_metadata) {
        return SensorTimestampStatus::NoCaptureMetadata;
    }

    *sensor_timestamp_ns = i_metadata->getSensorTimestamp();
    if (*sensor_timestamp_ns == 0) {
        return SensorTimestampStatus::ZeroTimestamp;
    }
    return SensorTimestampStatus::Available;
}

static SensorTimestampStatus read_sensor_timestamp_ns_from_egl_metadata(
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
        i_stream->getEGLDisplay(),
        i_stream->getEGLStream(),
        EGLStream::MetadataContainer::CONSUMER,
        &status);
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

static SensorTimestampStatus read_sensor_timestamp_ns(
        LkArgusSession* s,
        uint64_t* sensor_timestamp_ns,
        Argus::Status* metadata_status) {
    SensorTimestampStatus status =
        read_sensor_timestamp_ns_from_egl_metadata(s, sensor_timestamp_ns, metadata_status);
    if (status == SensorTimestampStatus::Available) {
        return status;
    }

    // Fall back to capture-complete events only when embedded EGLStream metadata
    // is unavailable. Event queues are session-scoped, so they can lag or lead
    // the exact frame returned by FrameConsumer::acquireFrame().
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

extern "C" {

void* lk_argus_create_session(int sensor_index, int width, int height, int fps) {
    auto* s = new LkArgusSession();
    for (int i = 0; i < kNumDmaBufs; i++) {
        s->dmabuf_fds[i] = -1;
        s->dmabuf_surfaces[i] = nullptr;
    }
    s->dmabuf_write_idx = 0;
    s->width = width;
    s->height = height;
    s->metadata_enabled = false;
    s->event_metadata_enabled = false;

    // Create CameraProvider
    s->provider = Argus::UniqueObj<Argus::CameraProvider>(
        Argus::CameraProvider::create());
    auto* i_provider = Argus::interface_cast<Argus::ICameraProvider>(s->provider);
    if (!i_provider) {
        fprintf(stderr, "[lk_argus] Failed to create CameraProvider\n");
        delete s;
        return nullptr;
    }

    // Enumerate camera devices
    std::vector<Argus::CameraDevice*> devices;
    i_provider->getCameraDevices(&devices);
    if (devices.empty() || sensor_index >= static_cast<int>(devices.size())) {
        fprintf(stderr, "[lk_argus] No camera device at index %d (found %zu)\n",
                sensor_index, devices.size());
        delete s;
        return nullptr;
    }

    // Create CaptureSession
    Argus::Status status;
    s->session = Argus::UniqueObj<Argus::CaptureSession>(
        i_provider->createCaptureSession(devices[sensor_index], &status));
    if (status != Argus::STATUS_OK) {
        fprintf(stderr, "[lk_argus] Failed to create CaptureSession: %d\n",
                static_cast<int>(status));
        delete s;
        return nullptr;
    }
    auto* i_session = Argus::interface_cast<Argus::ICaptureSession>(s->session);
    auto* i_event_provider = Argus::interface_cast<Argus::IEventProvider>(s->session);
    if (i_event_provider) {
        std::vector<Argus::EventType> event_types;
        event_types.push_back(Argus::EVENT_TYPE_CAPTURE_COMPLETE);
        s->event_queue = Argus::UniqueObj<Argus::EventQueue>(
            i_event_provider->createEventQueue(event_types, &status));
        if (status != Argus::STATUS_OK || !s->event_queue) {
            fprintf(stderr,
                    "[lk_argus] WARNING: failed to create capture-complete event queue: %d\n",
                    static_cast<int>(status));
        } else {
            s->event_metadata_enabled = true;
            fprintf(stderr, "[lk_argus] Capture-complete metadata events enabled: yes\n");
        }
    } else {
        fprintf(stderr, "[lk_argus] WARNING: capture session has no event provider interface\n");
    }

    // Create OutputStream (EGLStream-backed)
    s->stream_settings = Argus::UniqueObj<Argus::OutputStreamSettings>(
        i_session->createOutputStreamSettings(Argus::STREAM_TYPE_EGL, &status));
    auto* i_stream_settings =
        Argus::interface_cast<Argus::IEGLOutputStreamSettings>(s->stream_settings);
    if (!i_stream_settings) {
        fprintf(stderr, "[lk_argus] Failed to get IEGLOutputStreamSettings\n");
        delete s;
        return nullptr;
    }
    i_stream_settings->setPixelFormat(Argus::PIXEL_FMT_YCbCr_420_888);
    i_stream_settings->setResolution(Argus::Size2D<uint32_t>(width, height));
    status = i_stream_settings->setMode(Argus::EGL_STREAM_MODE_MAILBOX);
    if (status != Argus::STATUS_OK) {
        fprintf(stderr, "[lk_argus] WARNING: failed to set EGLStream mailbox mode: %d\n",
                static_cast<int>(status));
    }
    status = i_stream_settings->setFifoLength(1);
    if (status != Argus::STATUS_OK) {
        fprintf(stderr, "[lk_argus] WARNING: failed to set EGLStream FIFO length: %d\n",
                static_cast<int>(status));
    }
    fprintf(stderr, "[lk_argus] EGLStream mode: mailbox, fifo length: %u\n",
            i_stream_settings->getFifoLength());
    status = i_stream_settings->setMetadataEnable(true);
    if (status != Argus::STATUS_OK) {
        fprintf(stderr, "[lk_argus] WARNING: failed to enable EGLStream metadata: %d\n",
                static_cast<int>(status));
    }
    s->metadata_enabled = i_stream_settings->getMetadataEnable();
    fprintf(stderr, "[lk_argus] EGLStream metadata enabled: %s\n",
            s->metadata_enabled ? "yes" : "no");

    s->stream = Argus::UniqueObj<Argus::OutputStream>(
        i_session->createOutputStream(s->stream_settings.get(), &status));
    if (status != Argus::STATUS_OK) {
        fprintf(stderr, "[lk_argus] Failed to create OutputStream: %d\n",
                static_cast<int>(status));
        delete s;
        return nullptr;
    }

    // Create FrameConsumer
    s->consumer = Argus::UniqueObj<EGLStream::FrameConsumer>(
        EGLStream::FrameConsumer::create(s->stream.get()));
    auto* i_consumer =
        Argus::interface_cast<EGLStream::IFrameConsumer>(s->consumer);
    if (!i_consumer) {
        fprintf(stderr, "[lk_argus] Failed to create FrameConsumer\n");
        delete s;
        return nullptr;
    }

    // Create capture Request
    s->request = Argus::UniqueObj<Argus::Request>(
        i_session->createRequest(Argus::CAPTURE_INTENT_VIDEO_RECORD, &status));
    if (status != Argus::STATUS_OK) {
        fprintf(stderr, "[lk_argus] Failed to create Request: %d\n",
                static_cast<int>(status));
        delete s;
        return nullptr;
    }
    auto* i_request =
        Argus::interface_cast<Argus::IRequest>(s->request);
    i_request->enableOutputStream(s->stream.get());

    // --- Sensor mode selection ---
    // Argus auto-selects a sensor mode, but often picks the highest-resolution
    // mode and downscales, running at that mode's (lower) framerate.  We
    // explicitly pick the smallest mode that covers the requested resolution
    // and supports the requested framerate.
    auto* i_props = Argus::interface_cast<Argus::ICameraProperties>(
        devices[sensor_index]);
    if (i_props) {
        std::vector<Argus::SensorMode*> modes;
        i_props->getAllSensorModes(&modes);
        fprintf(stderr, "[lk_argus] %zu sensor modes available:\n", modes.size());

        Argus::SensorMode* best_mode = nullptr;
        uint64_t best_pixels = UINT64_MAX;
        uint64_t requested_dur_ns = 1000000000ULL / fps;

        for (size_t i = 0; i < modes.size(); i++) {
            auto* i_mode = Argus::interface_cast<Argus::ISensorMode>(modes[i]);
            if (!i_mode) continue;
            auto res = i_mode->getResolution();
            auto dur = i_mode->getFrameDurationRange();
            double min_fps_mode = 1e9 / static_cast<double>(dur.max());
            double max_fps_mode = 1e9 / static_cast<double>(dur.min());
            fprintf(stderr, "  [%zu] %ux%u  fps %.1f-%.1f  duration %lu-%lu ns\n",
                    i, res.width(), res.height(),
                    min_fps_mode, max_fps_mode,
                    dur.min(), dur.max());

            // Compare frame durations instead of floating-point fps.
            // Sensor durations are in nanoseconds and often off by 1 ns
            // from the ideal value (e.g., 33333334 vs 33333333 for 30fps).
            // A 1ms tolerance handles this rounding.
            if (static_cast<int>(res.width()) >= width &&
                static_cast<int>(res.height()) >= height &&
                dur.min() <= requested_dur_ns + 1000000) {
                uint64_t pixels = static_cast<uint64_t>(res.width()) * res.height();
                if (pixels < best_pixels) {
                    best_pixels = pixels;
                    best_mode = modes[i];
                }
            }
        }

        auto* i_source = Argus::interface_cast<Argus::ISourceSettings>(
            i_request->getSourceSettings());

        if (best_mode) {
            auto* i_best = Argus::interface_cast<Argus::ISensorMode>(best_mode);
            auto res = i_best->getResolution();
            auto dur = i_best->getFrameDurationRange();
            fprintf(stderr, "[lk_argus] Selected sensor mode: %ux%u  fps %.1f-%.1f\n",
                    res.width(), res.height(),
                    1e9 / static_cast<double>(dur.max()),
                    1e9 / static_cast<double>(dur.min()));
            if (i_source) {
                i_source->setSensorMode(best_mode);
            }
        } else {
            fprintf(stderr, "[lk_argus] WARNING: no sensor mode found for %dx%d @ %d fps, "
                    "using Argus default (may be slower)\n", width, height, fps);
        }

        if (i_source) {
            uint64_t frame_dur_ns = 1000000000ULL / fps;
            i_source->setFrameDurationRange(
                Argus::Range<uint64_t>(frame_dur_ns, frame_dur_ns));
            i_source->setExposureTimeRange(
                Argus::Range<uint64_t>(0, frame_dur_ns));
            fprintf(stderr, "[lk_argus] Frame duration: %lu ns, max exposure: %lu ns\n",
                    frame_dur_ns, frame_dur_ns);
        }
    } else {
        fprintf(stderr, "[lk_argus] WARNING: could not query sensor modes\n");
        auto* i_source = Argus::interface_cast<Argus::ISourceSettings>(
            i_request->getSourceSettings());
        if (i_source) {
            i_source->setFrameDurationRange(
                Argus::Range<uint64_t>(1000000000ULL / fps, 1000000000ULL / fps));
        }
    }

    // Allocate a ring of persistent NvBufSurface buffers so the encoder can
    // hold one while we blit the next frame into a different one.
    for (int i = 0; i < kNumDmaBufs; i++) {
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
            fprintf(stderr, "[lk_argus] Failed to create NvBufSurface[%d]\n", i);
            delete s;
            return nullptr;
        }
        s->dmabuf_fds[i] = surface->surfaceList[0].bufferDesc;
        s->dmabuf_surfaces[i] = surface;
    }

    // Start repeating capture
    status = i_session->repeat(s->request.get());
    if (status != Argus::STATUS_OK) {
        fprintf(stderr, "[lk_argus] Failed to start repeating capture: %d\n",
                static_cast<int>(status));
        delete s;
        return nullptr;
    }

    fprintf(stderr, "[lk_argus] Session created: %dx%d @ %d fps, sensor %d, %d DMA buffers (fds:",
            width, height, fps, sensor_index, kNumDmaBufs);
    for (int i = 0; i < kNumDmaBufs; i++) fprintf(stderr, " %d", s->dmabuf_fds[i]);
    fprintf(stderr, ")\n");
    return s;
}

int lk_argus_acquire_frame_with_metadata(
        void* handle,
        uint64_t* sensor_timestamp_ns,
        uint64_t* acquire_wait_ns,
        uint64_t* blit_ns) {
    using Clock = std::chrono::steady_clock;

    auto* s = static_cast<LkArgusSession*>(handle);
    if (!s) return -1;
    if (sensor_timestamp_ns) *sensor_timestamp_ns = 0;
    if (acquire_wait_ns) *acquire_wait_ns = 0;
    if (blit_ns) *blit_ns = 0;

    auto* i_consumer =
        Argus::interface_cast<EGLStream::IFrameConsumer>(s->consumer);
    if (!i_consumer) return -1;

    // Release any previously held frame
    s->current_frame.reset();

    auto t0 = Clock::now();

    Argus::Status status;
    s->current_frame = Argus::UniqueObj<EGLStream::Frame>(
        i_consumer->acquireFrame(kAcquireTimeoutNs, &status));
    if (status != Argus::STATUS_OK || !s->current_frame) {
        return -1;
    }

    auto t1 = Clock::now();

    auto* i_frame =
        Argus::interface_cast<EGLStream::IFrame>(s->current_frame);
    if (!i_frame) return -1;

    Argus::Status metadata_status = Argus::STATUS_OK;
    SensorTimestampStatus sensor_timestamp_status =
        read_sensor_timestamp_ns(s, sensor_timestamp_ns, &metadata_status);
    bool has_sensor_timestamp =
        sensor_timestamp_status == SensorTimestampStatus::Available;
    static SensorTimestampStatus last_logged_sensor_timestamp_status =
        SensorTimestampStatus::Available;
    if (!has_sensor_timestamp &&
        sensor_timestamp_status != last_logged_sensor_timestamp_status) {
        fprintf(stderr,
                "[lk_argus] Sensor timestamp unavailable: %s "
                "(event metadata enabled=%s, EGL metadata enabled=%s, status=%d)\n",
                sensor_timestamp_status_name(sensor_timestamp_status),
                s->event_metadata_enabled ? "yes" : "no",
                s->metadata_enabled ? "yes" : "no",
                static_cast<int>(metadata_status));
        last_logged_sensor_timestamp_status = sensor_timestamp_status;
    } else if (has_sensor_timestamp &&
               last_logged_sensor_timestamp_status != SensorTimestampStatus::Available) {
        fprintf(stderr, "[lk_argus] Sensor timestamp available\n");
        last_logged_sensor_timestamp_status = SensorTimestampStatus::Available;
    }

    auto* image = i_frame->getImage();
    if (!image) return -1;

    // Get the NativeBuffer interface to extract the DMA fd
    auto* i_native =
        Argus::interface_cast<EGLStream::NV::IImageNativeBuffer>(image);
    if (!i_native) {
        fprintf(stderr, "[lk_argus] Image does not support IImageNativeBuffer\n");
        return -1;
    }

    // Pick the next buffer in the ring so we don't overwrite a buffer the
    // encoder may still be reading from.
    int idx = s->dmabuf_write_idx;
    s->dmabuf_write_idx = (s->dmabuf_write_idx + 1) % kNumDmaBufs;
    int fd = s->dmabuf_fds[idx];

    // Copy (blit) the acquired frame into the selected NvBufSurface.
    status = i_native->copyToNvBuffer(fd);

    auto t2 = Clock::now();
    auto acquire_duration_ns =
        std::chrono::duration_cast<std::chrono::nanoseconds>(t1 - t0).count();
    auto blit_duration_ns =
        std::chrono::duration_cast<std::chrono::nanoseconds>(t2 - t1).count();
    if (acquire_wait_ns) *acquire_wait_ns = static_cast<uint64_t>(acquire_duration_ns);
    if (blit_ns) *blit_ns = static_cast<uint64_t>(blit_duration_ns);

    // Release the Argus frame immediately - the pixel data has been blitted
    // into our persistent NvBufSurface so we no longer need the EGLStream frame.
    s->current_frame.reset();

    if (status != Argus::STATUS_OK) {
        fprintf(stderr, "[lk_argus] copyToNvBuffer failed: %d\n",
                static_cast<int>(status));
        return -1;
    }

    return fd;
}

int lk_argus_acquire_frame(void* handle) {
    return lk_argus_acquire_frame_with_metadata(handle, nullptr, nullptr, nullptr);
}

int lk_argus_copy_frame_to_i420(
        void* handle,
        int dmabuf_fd,
        uint8_t* dst_y,
        int dst_stride_y,
        uint8_t* dst_u,
        int dst_stride_u,
        uint8_t* dst_v,
        int dst_stride_v,
        uint64_t* copy_to_i420_ns) {
    using Clock = std::chrono::steady_clock;

    auto* s = static_cast<LkArgusSession*>(handle);
    if (!s || dmabuf_fd < 0 || !dst_y || !dst_u || !dst_v) {
        return kCopyI420InvalidArgument;
    }

    const int width = s->width;
    const int height = s->height;
    const int chroma_width = (width + 1) / 2;
    const int chroma_height = (height + 1) / 2;
    if (width <= 0 || height <= 0 ||
        dst_stride_y < width ||
        dst_stride_u < chroma_width ||
        dst_stride_v < chroma_width) {
        return kCopyI420InvalidArgument;
    }

    NvBufSurface* surface = nullptr;
    for (int i = 0; i < kNumDmaBufs; i++) {
        if (s->dmabuf_fds[i] == dmabuf_fd) {
            surface = s->dmabuf_surfaces[i];
            break;
        }
    }
    if (!surface || surface->batchSize < 1) {
        return kCopyI420SurfaceNotFound;
    }

    auto t0 = Clock::now();
    int ret = NvBufSurfaceMap(surface, 0, -1, NVBUF_MAP_READ);
    if (ret != 0) {
        return copy_i420_map_error(ret);
    }

    ret = NvBufSurfaceSyncForCpu(surface, 0, -1);
    if (ret != 0) {
        int unmap_ret = NvBufSurfaceUnMap(surface, 0, -1);
        if (unmap_ret != 0) {
            return copy_i420_unmap_error(unmap_ret);
        }
        return copy_i420_sync_error(ret);
    }

    const NvBufSurfaceParams& params = surface->surfaceList[0];
    const uint8_t* src_y =
        static_cast<const uint8_t*>(params.mappedAddr.addr[0]);
    const uint8_t* src_uv =
        static_cast<const uint8_t*>(params.mappedAddr.addr[1]);
    const int src_stride_y = static_cast<int>(params.planeParams.pitch[0]);
    const int src_stride_uv = static_cast<int>(params.planeParams.pitch[1]);

    if (!src_y || !src_uv ||
        src_stride_y < width ||
        src_stride_uv < chroma_width * 2) {
        ret = NvBufSurfaceUnMap(surface, 0, -1);
        if (ret != 0) {
            return copy_i420_unmap_error(ret);
        }
        return kCopyI420InvalidSurface;
    }

    for (int row = 0; row < height; row++) {
        std::memcpy(dst_y + row * dst_stride_y,
                    src_y + row * src_stride_y,
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
    auto t1 = Clock::now();
    if (copy_to_i420_ns) {
        *copy_to_i420_ns = static_cast<uint64_t>(
            std::chrono::duration_cast<std::chrono::nanoseconds>(t1 - t0).count());
    }
    if (ret != 0) {
        return copy_i420_unmap_error(ret);
    }
    return 0;
}

void lk_argus_release_frame(void* handle) {
    auto* s = static_cast<LkArgusSession*>(handle);
    if (!s) return;
    s->current_frame.reset();
}

void lk_argus_destroy_session(void* handle) {
    auto* s = static_cast<LkArgusSession*>(handle);
    if (!s) return;

    // Stop repeating capture
    auto* i_session = Argus::interface_cast<Argus::ICaptureSession>(s->session);
    if (i_session) {
        i_session->stopRepeat();
        i_session->waitForIdle();
    }

    s->current_frame.reset();

    // Free all persistent NvBufSurface buffers using the original pointers.
    for (int i = 0; i < kNumDmaBufs; i++) {
        if (s->dmabuf_surfaces[i]) {
            NvBufSurfaceDestroy(s->dmabuf_surfaces[i]);
            s->dmabuf_surfaces[i] = nullptr;
        }
        s->dmabuf_fds[i] = -1;
    }

    delete s;
    fprintf(stderr, "[lk_argus] Session destroyed\n");
}

}  // extern "C"
