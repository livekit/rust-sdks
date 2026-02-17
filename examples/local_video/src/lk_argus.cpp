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

static constexpr int kNumDmaBufs = 3;

struct LkArgusSession {
    Argus::UniqueObj<Argus::CameraProvider> provider;
    Argus::UniqueObj<Argus::CaptureSession>  session;
    Argus::UniqueObj<Argus::OutputStreamSettings> stream_settings;
    Argus::UniqueObj<Argus::OutputStream>    stream;
    Argus::UniqueObj<Argus::Request>         request;
    Argus::UniqueObj<EGLStream::FrameConsumer> consumer;

    // Most recently acquired frame (kept alive until release/next acquire).
    Argus::UniqueObj<EGLStream::Frame> current_frame;

    // Ring of DMA fds so the encoder can hold one buffer while we blit the
    // next frame into a different one.  Avoids the "Wrong buffer index"
    // errors caused by the encoder and Argus racing on a single buffer.
    int dmabuf_fds[kNumDmaBufs];
    NvBufSurface* dmabuf_surfaces[kNumDmaBufs];  // original surface ptrs for sync
    int dmabuf_write_idx;  // next buffer to blit into
    int width;
    int height;
};

static const uint64_t kAcquireTimeoutNs = 1000000000ULL; // 1 second

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

    // Pick the next buffer in the ring so we don't overwrite a buffer the
    // encoder may still be reading from.
    int idx = s->dmabuf_write_idx;
    s->dmabuf_write_idx = (s->dmabuf_write_idx + 1) % kNumDmaBufs;
    int fd = s->dmabuf_fds[idx];

    // Copy (blit) the acquired frame into the selected NvBufSurface.
    status = i_native->copyToNvBuffer(fd);

    // Release the Argus frame immediately – the pixel data has been blitted
    // into our persistent NvBufSurface so we no longer need the EGLStream frame.
    s->current_frame.reset();

    if (status != Argus::STATUS_OK) {
        fprintf(stderr, "[lk_argus] copyToNvBuffer failed: %d\n",
                static_cast<int>(status));
        return -1;
    }

    // Sync the buffer for device (encoder) access.  We use the original
    // NvBufSurface pointer from NvBufSurfaceCreate -- this avoids the
    // "Wrong buffer index" errors that occur when syncing a surface
    // obtained via NvBufSurfaceFromFd on some JetPack versions.
    NvBufSurface* surface = s->dmabuf_surfaces[idx];
    if (surface) {
        NvBufSurfaceSyncForDevice(surface, 0, -1);
    }

    return fd;
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
