# libargus-sys

Native shim and raw FFI bindings for capturing NVIDIA Jetson MIPI CSI cameras
through [libargus] as NV12 DMA buffers, used by the `source-device-argus`
feature of `livekit-capture`.

[libargus]: https://docs.nvidia.com/jetson/l4t-multimedia/group__LibargusAPI.html

## How it builds

The C++ shim (`src/lk_argus.cpp`, ABI in `src/lk_argus.h`) is compiled only
when **all** of the following hold at build time:

- the target is aarch64 Linux,
- the Jetson Multimedia API headers are present
  (`/usr/src/jetson_multimedia_api`, package
  `nvidia-l4t-jetson-multimedia-api`),
- the Tegra userspace libraries `libnvargus_socketclient.so` and
  `libnvbufsurface.so` are present (`/usr/lib/aarch64-linux-gnu/tegra`).

On every other target — or when the probe fails — the crate still builds and
exposes the same API, but every entry point is an inert stub returning
`LK_ARGUS_ERR_UNAVAILABLE`, and the `AVAILABLE` constant is `false`.
Consumers therefore need no build script or `cfg` of their own: "not a
Jetson" degrades exactly like "Jetson with zero cameras".

### Cross-compilation

Point the probe at a sysroot copy of the Jetson filesystem:

| Environment variable | Default |
| --- | --- |
| `JETSON_MULTIMEDIA_API_DIR` | `/usr/src/jetson_multimedia_api` |
| `JETSON_TEGRA_LIB_DIR` | `/usr/lib/aarch64-linux-gnu/tegra` |

The C++ toolchain itself is configured through the standard [`cc` crate
variables][cc-env] (`CXX_aarch64_unknown_linux_gnu`, `CXXFLAGS=--sysroot=…`).

[cc-env]: https://docs.rs/cc/latest/cc/#external-configuration-via-environment-variables

## Supported JetPack versions

JetPack 5 through 7 (L4T r35–r39). The Argus API has been stable across this
range; JetPack 6.1 rewrote the stack internals but kept the API. JetPack 4's
`nvbuf_utils`-era buffer API is not supported. NVIDIA's long-term successor
to Argus is SIPL (JetPack 7+); Argus is in sustaining mode but remains the
supported CSI capture path on Orin-class devices.

## Runtime requirements

- A Jetson device with the Argus stack: the `nvargus-daemon` service must be
  running (the shim talks to it through `libnvargus_socketclient`).
- A CSI camera module with a device-tree entry and ISP tuning file. USB (UVC)
  webcams do not go through Argus.

If `nvargus-daemon` dies mid-capture, frame acquisition reports
`LK_ARGUS_ERR_DISCONNECTED`; destroy the session and re-create it once the
daemon has restarted (`systemctl restart nvargus-daemon`).

## Design notes

- Frames are ISP-processed NV12 delivered as DMA buffer fds: zero CPU copies,
  one VIC hardware blit from the EGLStream frame into a persistent
  `NvBufSurface` ring.
- Ring slots are *leased*: a slot's fd stays valid until
  `lk_argus_frame_release`, however long a consumer (e.g. a hardware encoder)
  holds the frame. An exhausted ring is reported as retryable backpressure
  (`LK_ARGUS_ERR_NO_FREE_BUFFER`), never overwritten.
- The process keeps a single shared `CameraProvider` (created lazily,
  intentionally never destroyed): repeated create/destroy cycles are flaky on
  some JetPack releases.
- See `src/lk_argus.h` for the full ABI and the thread-safety contract.
