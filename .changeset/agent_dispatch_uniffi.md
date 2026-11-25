---
livekit-api: patch
livekit-ffi: minor
---

Expose the AgentDispatchService client over UniFFI for `room-apis` consumers. `AgentDispatchClient` has async `create_dispatch`, `delete_dispatch`, `list_dispatch` and `get_dispatch` methods that wrap `livekit_api`. The `livekit-ffi` TLS features now also configure the `livekit-api` HTTP client, so HTTPS server API calls work.

Server API errors without a Twirp JSON body, such as the plain-text 401 from the server auth check, now map to a Twirp code from the HTTP status (`unauthenticated` for 401). Before, they failed as a response decode error.
