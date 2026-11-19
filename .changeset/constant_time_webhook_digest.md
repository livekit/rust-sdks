---
livekit-api: patch
---

Compare the webhook body digest in constant time. `WebhookReceiver::receive` used a short-circuiting slice comparison for the final body-integrity check; it now uses `subtle::ConstantTimeEq`, which is also length-independent.
