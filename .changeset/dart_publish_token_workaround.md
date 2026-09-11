---
livekit-uniffi: patch
---

# Publish the Dart package without sending the publishing token on dependency resolution

pub.dev has started answering 403 to package metadata requests that carry a
bearer token, and the publishing token was registered for the whole host, so
`dart pub get` failed before the upload could start. Resolution and validation
now run without the token, which is added back only for the upload.
