---
livekit-api: patch
---

fix: surface full error chain in region fetch failures for better TLS error diagnosis.

When connecting to LiveKit Cloud from containers without CA certificates installed, the error message now includes the full error chain (e.g., "invalid peer certificate: UnknownIssuer") instead of just "error sending request for url (...)". This makes TLS certificate issues self-diagnosing.

Also added documentation for TLS features in Cargo.toml, highlighting `rustls-tls-webpki-roots` as the recommended option for container deployments.
