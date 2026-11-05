---
livekit-net: patch
---

Add `HttpStreamClient`, which streams an HTTP response body chunk by chunk (for example server-sent events), with a native implementation, `set_http_stream_client` and `http_stream_client`, which falls back to a host-registered `HttpClient` and then gives the whole body as one chunk.
