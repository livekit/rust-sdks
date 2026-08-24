/*
 * Copyright 2026 LiveKit, Inc.
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

/*
 * C ABI of the LiveKit libargus capture shim.
 *
 * This header is the single source of truth for the FFI surface; the Rust
 * declarations in lib.rs mirror it and must be kept in sync.
 *
 * Thread-safety contract:
 *   - Enumeration functions, lk_argus_set_logger, lk_argus_session_create,
 *     and lk_argus_session_destroy may be called from any thread.
 *   - Per session, lk_argus_frame_acquire and lk_argus_frame_copy_to_i420
 *     must be driven by a single consumer thread at a time.
 *   - lk_argus_frame_release and lk_argus_session_interrupt may be called
 *     from any thread (e.g. from an encoder thread releasing a frame).
 */

#ifndef LK_ARGUS_H_
#define LK_ARGUS_H_

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* ABI version of this shim. Incremented on breaking changes. */
#define LK_ARGUS_ABI_VERSION 1

/* Maximum supported DMA buffer ring size. */
#define LK_ARGUS_MAX_DMA_BUFS 16

/* Status codes. 0 is success; negative values are errors. */
enum {
  LK_ARGUS_OK = 0,
  /* Invalid arguments crossed the FFI boundary (caller bug). */
  LK_ARGUS_ERR_INVALID_ARG = -1,
  /* CameraProvider creation failed (nvargus-daemon not running?). */
  LK_ARGUS_ERR_NO_PROVIDER = -2,
  /* Device index out of range. */
  LK_ARGUS_ERR_NO_DEVICE = -3,
  /* Generic Argus failure; see lk_argus_session_last_argus_status. */
  LK_ARGUS_ERR_ARGUS = -4,
  /* Frame acquire timed out. Not fatal: retry. */
  LK_ARGUS_ERR_TIMEOUT = -5,
  /* EGLStream disconnected (nvargus-daemon died or stream ended). The
   * session is dead and must be destroyed and re-created. */
  LK_ARGUS_ERR_DISCONNECTED = -6,
  /* NvBufSurface operation failed. */
  LK_ARGUS_ERR_NVBUF = -7,
  /* Every ring slot is leased to an in-flight frame. Not fatal:
   * backpressure; retry after frames are released. */
  LK_ARGUS_ERR_NO_FREE_BUFFER = -8,
  /* lk_argus_session_interrupt was called. */
  LK_ARGUS_ERR_INTERRUPTED = -9,
  /* Returned by the Rust stubs when the native shim was not compiled in;
   * the shim itself never returns this. */
  LK_ARGUS_ERR_UNAVAILABLE = -10,
};

/* Log levels passed to LkArgusLogFn. */
enum {
  LK_ARGUS_LOG_ERROR = 0,
  LK_ARGUS_LOG_WARN = 1,
  LK_ARGUS_LOG_INFO = 2,
  LK_ARGUS_LOG_DEBUG = 3,
};

/* Opaque capture session handle. */
typedef struct LkArgusSession LkArgusSession;

typedef struct LkArgusDeviceInfo {
  /* ICameraProperties::getUUID(), formatted, NUL-terminated. */
  char uuid[37];
  /* Best-effort human-readable module name; empty string when the JetPack
   * version exposes none. */
  char name[64];
  int32_t sensor_mode_count;
} LkArgusDeviceInfo;

typedef struct LkArgusSensorModeInfo {
  uint32_t width;
  uint32_t height;
  /* ISensorMode::getFrameDurationRange(). */
  uint64_t min_frame_duration_ns;
  uint64_t max_frame_duration_ns;
  /* ISensorMode::getInputBitDepth(). */
  uint32_t bit_depth;
} LkArgusSensorModeInfo;

typedef struct LkArgusSessionConfig {
  int32_t device_index;
  /* Sensor mode to use, or -1 to auto-select the smallest mode covering the
   * requested resolution and frame rate. */
  int32_t sensor_mode_index;
  /* Output (ISP-scaled) resolution and frame rate. */
  int32_t width;
  int32_t height;
  int32_t fps;
  /* DMA buffer ring depth; 0 selects the default (4). Clamped to
   * [2, LK_ARGUS_MAX_DMA_BUFS]. */
  int32_t num_dma_bufs;
} LkArgusSessionConfig;

typedef struct LkArgusFrame {
  /* NV12 DMA buffer fd, BORROWED from the session ring. Valid until
   * lk_argus_frame_release(buffer_index) is called; never close it. */
  int32_t dmabuf_fd;
  /* Ring slot index; token for lk_argus_frame_release. */
  int32_t buffer_index;
  uint32_t width;
  uint32_t height;
  /* Actual plane pitches/offsets (NvBufSurfaceParams.planeParams), Y then
   * interleaved UV. */
  uint32_t pitch[2];
  uint32_t offset[2];
  /* Argus sensor timestamp (CLOCK_MONOTONIC domain), 0 when unavailable. */
  uint64_t sensor_timestamp_ns;
  /* Diagnostics: time spent waiting in acquireFrame and blitting. */
  uint64_t acquire_wait_ns;
  uint64_t blit_ns;
} LkArgusFrame;

/*
 * Installs a log callback, replacing stderr output. `msg` is only valid for
 * the duration of the call. Pass a null `log_fn` to restore stderr logging.
 */
typedef void (*LkArgusLogFn)(int32_t level, const char* msg, void* user_data);
int32_t lk_argus_set_logger(LkArgusLogFn log_fn, void* user_data);

/*
 * Copies the Argus version string (ICameraProvider::getVersion) into `buf`,
 * NUL-terminated and truncated to `buf_len`.
 */
int32_t lk_argus_version(char* buf, size_t buf_len);

/* Returns the number of camera devices (>= 0), or a negative status. */
int32_t lk_argus_device_count(void);

int32_t lk_argus_device_info(int32_t device_index, LkArgusDeviceInfo* out);

int32_t lk_argus_sensor_mode_info(int32_t device_index,
                                  int32_t mode_index,
                                  LkArgusSensorModeInfo* out);

int32_t lk_argus_session_create(const LkArgusSessionConfig* config,
                                LkArgusSession** out_session);

/*
 * Tears down a session. Interrupts any pending acquire, stops the repeating
 * capture, and destroys the DMA buffer ring. Waits a bounded time for
 * outstanding frame leases to be released; leases still outstanding after
 * the wait are logged and the buffers destroyed anyway, so callers must
 * ensure all frames are released before destroying the session.
 */
void lk_argus_session_destroy(LkArgusSession* session);

/*
 * Makes the pending (and every subsequent) lk_argus_frame_acquire return
 * LK_ARGUS_ERR_INTERRUPTED. Note the current acquireFrame wait itself cannot
 * be cut short; interruption latency is bounded by the acquire timeout.
 */
int32_t lk_argus_session_interrupt(LkArgusSession* session);

/* Raw Argus::Status of the session's last failing Argus call. */
int32_t lk_argus_session_last_argus_status(const LkArgusSession* session);

/*
 * Acquires the next frame, blocking at most `timeout_ns`. On success blits
 * the frame into a free ring slot, marks that slot leased, and fills `out`.
 * The lease (and the fd's validity) lasts until lk_argus_frame_release is
 * called with out->buffer_index.
 */
int32_t lk_argus_frame_acquire(LkArgusSession* session,
                               uint64_t timeout_ns,
                               LkArgusFrame* out);

/* Returns a leased ring slot for reuse. Callable from any thread. */
int32_t lk_argus_frame_release(LkArgusSession* session, int32_t buffer_index);

/*
 * CPU fallback: copies the NV12 contents of a leased ring slot into
 * caller-owned I420 planes.
 */
int32_t lk_argus_frame_copy_to_i420(LkArgusSession* session,
                                    int32_t buffer_index,
                                    uint8_t* dst_y,
                                    int32_t dst_stride_y,
                                    uint8_t* dst_u,
                                    int32_t dst_stride_u,
                                    uint8_t* dst_v,
                                    int32_t dst_stride_v);

#ifdef __cplusplus
}  /* extern "C" */
#endif

#endif  /* LK_ARGUS_H_ */
