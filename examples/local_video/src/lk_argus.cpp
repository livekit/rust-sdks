// C shim around NVIDIA libargus for MIPI CSI camera capture on Jetson.
//
// Exposes a simple C API for the Rust FFI in argus.rs:
//   lk_argus_create_session  – open sensor, configure ISP, start repeating capture
//   lk_argus_acquire_frame   – dequeue next frame, return NvBufSurface DMA fd
//   lk_argus_release_frame   – release frame back to Argus buffer pool
//   lk_argus_destroy_session – tear down everything

#include <cstdio>
#include <cstdlib>
#include <cstring>

#include <Argus/Argus.h>
#include <EGLStream/EGLStream.h>
#include <EGLStream/NV/ImageNativeBuffer.h>
#include "NvBufSurface.h"

struct LkArgusSession {
    Argus::UniqueObj<Argus::CameraProvider> provider;
    Argus::UniqueObj<Argus::CaptureSession>  session;
    Argus::UniqueObj<Argus::OutputStreamSettings> stream_settings;
    Argus::UniqueObj<Argus::OutputStream>    stream;
    Argus::UniqueObj<Argus::Request>         request;
    Argus::UniqueObj<EGLStream::FrameConsumer> consumer;

    // Most recently acquired frame (kept alive until release/next acquire).
    Argus::UniqueObj<EGLStream::Frame> current_frame;

    // DMA fd for the NvBufSurface allocated for the current frame.
    // We allocate one persistent buffer and blit each acquired frame into it
    // via NvBufSurfaceTransform so the fd stays valid across acquire/release.
    int dmabuf_fd;
    int width;
    int height;
};

static const uint64_t kAcquireTimeoutNs = 1000000000ULL; // 1 second

extern "C" {

void* lk_argus_create_session(int sensor_index, int width, int height, int fps) {
    auto* s = new LkArgusSession();
    s->dmabuf_fd = -1;
    s->width = width;
    s->height = height;

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
    i_stream_settings->setMetadataEnable(false);

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

    // Set framerate via source settings
    auto* i_source =
        Argus::interface_cast<Argus::ISourceSettings>(i_request->getSourceSettings());
    if (i_source) {
        i_source->setFrameDurationRange(
            Argus::Range<uint64_t>(1000000000ULL / fps, 1000000000ULL / fps));
    }

    // Allocate a persistent NvBufSurface for DMA output
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
        fprintf(stderr, "[lk_argus] Failed to create NvBufSurface\n");
        delete s;
        return nullptr;
    }
    s->dmabuf_fd = surface->surfaceList[0].bufferDesc;

    // Start repeating capture
    status = i_session->repeat(s->request.get());
    if (status != Argus::STATUS_OK) {
        fprintf(stderr, "[lk_argus] Failed to start repeating capture: %d\n",
                static_cast<int>(status));
        delete s;
        return nullptr;
    }

    fprintf(stderr, "[lk_argus] Session created: %dx%d @ %d fps, sensor %d, dmabuf_fd=%d\n",
            width, height, fps, sensor_index, s->dmabuf_fd);
    return s;
}

int lk_argus_acquire_frame(void* handle) {
    auto* s = static_cast<LkArgusSession*>(handle);
    if (!s) return -1;

    auto* i_consumer =
        Argus::interface_cast<EGLStream::IFrameConsumer>(s->consumer);
    if (!i_consumer) return -1;

    // Release any previously held frame
    s->current_frame.reset();

    Argus::Status status;
    s->current_frame = Argus::UniqueObj<EGLStream::Frame>(
        i_consumer->acquireFrame(kAcquireTimeoutNs, &status));
    if (status != Argus::STATUS_OK || !s->current_frame) {
        return -1;
    }

    auto* i_frame =
        Argus::interface_cast<EGLStream::IFrame>(s->current_frame);
    if (!i_frame) return -1;

    auto* image = i_frame->getImage();
    if (!image) return -1;

    // Get the NativeBuffer interface to extract the DMA fd
    auto* i_native =
        Argus::interface_cast<EGLStream::NV::IImageNativeBuffer>(image);
    if (!i_native) {
        fprintf(stderr, "[lk_argus] Image does not support IImageNativeBuffer\n");
        return -1;
    }

    // Copy (blit) the acquired frame into our persistent NvBufSurface.
    // createNvBuffer is deprecated on newer JetPack; use copyToNvBuffer.
    status = i_native->copyToNvBuffer(s->dmabuf_fd);
    if (status != Argus::STATUS_OK) {
        fprintf(stderr, "[lk_argus] copyToNvBuffer failed: %d\n",
                static_cast<int>(status));
        return -1;
    }

    return s->dmabuf_fd;
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

    // Free the persistent NvBufSurface
    if (s->dmabuf_fd >= 0) {
        NvBufSurface* surface = nullptr;
        if (NvBufSurfaceFromFd(s->dmabuf_fd,
                               reinterpret_cast<void**>(&surface)) == 0 &&
            surface) {
            NvBufSurfaceDestroy(surface);
        }
        s->dmabuf_fd = -1;
    }

    delete s;
    fprintf(stderr, "[lk_argus] Session destroyed\n");
}

}  // extern "C"
